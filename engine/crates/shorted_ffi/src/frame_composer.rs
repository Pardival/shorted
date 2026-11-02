//! Composes project frames from decoded media on Apple platforms.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use objc2_core_foundation::CFRetained;
use objc2_io_surface::IOSurfaceRef;
#[cfg(test)]
use shorted_core::evaluate::CaptionRun;
use shorted_core::evaluate::{
    evaluate, ClipLayer, FrameDescription, OverlayLayer, TransitionState,
};
use shorted_core::model::{
    CaptionAnimation, CaptionPreset, ColorAdjust, Id, MediaKind, MediaRef, OverlayKind, Project,
    TransitionKind,
};
use shorted_media::apple::{AppleBackend, AppleDecoder, AppleFrame};
use shorted_media::{DecodedFrame, MediaBackend, MediaInfo, VideoDecoder};
use shorted_render::apple::texture_from_iosurface;
use shorted_render::captions::{parse_color, CaptionLook, CaptionPainter, CaptionWord};
use shorted_render::compositor::{load_image, Compositor, LayerEffect, LayerPlacement, Sprite};
use shorted_render::overlays::{shape_pixels, CANVAS_HEIGHT, CANVAS_WIDTH};
use shorted_render::pictures::{decode_animation, Animation};
use shorted_render::spike::RenderError;

use crate::api::media::resolve;
use crate::api::overlays::natural_size;

/// A seek further than this is cheaper than decoding the frames between.
const MAX_DECODE_AHEAD_US: i64 = 500_000;
/// Stands out as "something is missing" without looking like footage.
const OFFLINE_RGBA: [u8; 4] = [90, 60, 20, 255];

struct ShownFrame {
    _frame: AppleFrame,
    pts_us: i64,
    // The texture reads this surface: it must not go first.
    _surface: CFRetained<IOSurfaceRef>,
    texture: wgpu::Texture,
}

struct VideoSource {
    decoder: AppleDecoder,
    info: MediaInfo,
    pending: Option<AppleFrame>,
    shown: Option<ShownFrame>,
}

impl VideoSource {
    fn open(path: &Path) -> Option<Self> {
        let info = AppleBackend.probe(path).ok()?;
        let decoder = AppleBackend.open_decoder(path).ok()?;
        Some(Self {
            decoder,
            info,
            pending: None,
            shown: None,
        })
    }

    fn next(&mut self) -> Option<AppleFrame> {
        match self.pending.take() {
            Some(frame) => Some(frame),
            None => self.decoder.next_frame().ok().flatten(),
        }
    }

    /// Moves to the frame showing at `target_us`. Timestamps come from the file, so variable frame
    /// rate clips stay right.
    fn seek_to(&mut self, target_us: i64, compositor: &Compositor) -> Option<&wgpu::Texture> {
        let shown_at = self.shown.as_ref().map(|shown| shown.pts_us);
        if shown_at.is_none_or(|pts| pts > target_us || target_us - pts > MAX_DECODE_AHEAD_US) {
            self.decoder.seek(target_us).ok()?;
            self.pending = None;
            self.shown = None;
        }
        let mut latest = None;
        while let Some(frame) = self.next() {
            let first = self.shown.is_none() && latest.is_none();
            if frame.pts_us() > target_us && !first {
                self.pending = Some(frame);
                break;
            }
            latest = Some(frame);
        }
        if let Some(frame) = latest {
            let surface = frame.surface()?;
            // SAFETY: decoder surfaces are BGRA8 of the probed size; `ShownFrame` keeps them alive.
            let texture = unsafe {
                texture_from_iosurface(
                    &compositor.device,
                    &surface,
                    self.info.width,
                    self.info.height,
                    wgpu::TextureUsages::TEXTURE_BINDING,
                )
            }
            .ok()?;
            self.shown = Some(ShownFrame {
                pts_us: frame.pts_us(),
                _frame: frame,
                _surface: surface,
                texture,
            });
        }
        self.shown.as_ref().map(|shown| &shown.texture)
    }
}

struct Picture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

/// Draws frames of a project. Preview and export both go through `draw`, which is what keeps them
/// identical (ADR-0003).
pub struct FrameComposer {
    pub compositor: Compositor,
    videos: HashMap<Id, VideoSource>,
    images: HashMap<Id, Picture>,
    offline: wgpu::Texture,
    painter: Option<CaptionPainter>,
    /// The last caption drawn, reused for as long as the words stay.
    caption: Option<(String, wgpu::Texture)>,
    /// Each overlay's picture, with the look it was drawn from.
    overlays: HashMap<Id, (OverlayKind, OverlayPicture)>,
}

/// What an overlay draws from. Animations and videos change with time; stills do not.
enum OverlayPicture {
    Still(wgpu::Texture),
    Moving {
        animation: Animation,
        shown: usize,
        texture: wgpu::Texture,
    },
    Video(Box<VideoSource>),
}

const POP_US: i64 = 120_000;
const FADE_US: i64 = 150_000;
const POP_START: f32 = 0.85;
const TYPEWRITER_US_PER_CHARACTER: i64 = 30_000;

fn caption_look(project: &Project) -> CaptionLook {
    let settings = &project.captions;
    CaptionLook {
        font: settings.style.font.clone(),
        size: settings.style.size,
        color: parse_color(&settings.style.color),
        highlight: parse_color(&settings.emphasis_color),
        uppercase: settings.style.uppercase,
        outline: matches!(
            settings.preset,
            CaptionPreset::Outline | CaptionPreset::Bold
        ),
        boxed: settings.preset == CaptionPreset::Box,
        anchor: settings.vertical_anchor,
    }
}

/// Words as they show at this instant. Typewriter reveals one character at a time.
fn caption_words(
    frame: &FrameDescription,
    karaoke: bool,
    animation: CaptionAnimation,
) -> Vec<CaptionWord> {
    let mut left = if animation == CaptionAnimation::Typewriter {
        (frame.caption_age_us / TYPEWRITER_US_PER_CHARACTER) as usize + 1
    } else {
        usize::MAX
    };
    let mut words = Vec::new();
    for run in &frame.captions {
        if left == 0 {
            break;
        }
        let text: String = run.text.chars().take(left).collect();
        left = left.saturating_sub(run.text.chars().count());
        words.push(CaptionWord {
            text,
            highlighted: if karaoke { run.active } else { run.emphasis },
        });
    }
    words
}

/// The caption's entrance. Pop grows around the block, so the block stays where it is.
fn caption_effect(animation: CaptionAnimation, age_us: i64, anchor: f32) -> LayerEffect {
    let base = LayerEffect {
        keeps_alpha: true,
        ..LayerEffect::default()
    };
    match animation {
        CaptionAnimation::Pop if age_us < POP_US => {
            let zoom = POP_START + (1.0 - POP_START) * (age_us as f32 / POP_US as f32);
            LayerEffect {
                frame_zoom: zoom,
                shift: [0.0, (anchor - 0.5) * (1.0 / zoom - 1.0)],
                ..base
            }
        }
        CaptionAnimation::Fade if age_us < FADE_US => LayerEffect {
            opacity: age_us as f32 / FADE_US as f32,
            ..base
        },
        _ => base,
    }
}

const BLACK: [f32; 3] = [0.0; 3];
const WHITE: [f32; 3] = [1.0; 3];

fn dip(color: [f32; 3], incoming: bool, progress: f32) -> LayerEffect {
    // Up to the colour on the first half, back down from it on the second.
    let amount = if incoming {
        (2.0 - progress * 2.0).min(1.0)
    } else {
        (progress * 2.0).min(1.0)
    };
    LayerEffect {
        opacity: if incoming && progress < 0.5 { 0.0 } else { 1.0 },
        tint: [color[0], color[1], color[2], amount],
        ..LayerEffect::default()
    }
}

fn eased(progress: f32) -> f32 {
    progress * progress * (3.0 - 2.0 * progress)
}

/// One layer of a transition. Layer 0 is the outgoing clip, drawn first; Spin and Glitch are
/// drawn as a cut until they have a look of their own, and are not offered in the app.
fn transition_effect(index: usize, state: TransitionState) -> LayerEffect {
    let incoming = index > 0;
    let progress = state.progress.clamp(0.0, 1.0);
    let base = LayerEffect::default();
    match state.kind {
        TransitionKind::Crossfade if incoming => LayerEffect {
            opacity: progress,
            ..base
        },
        TransitionKind::DipToBlack => dip(BLACK, incoming, progress),
        TransitionKind::DipToWhite | TransitionKind::Flash => dip(WHITE, incoming, progress),
        TransitionKind::Slide if incoming => LayerEffect {
            shift: [1.0 - eased(progress), 0.0],
            ..base
        },
        TransitionKind::Push => push(incoming, eased(progress)),
        TransitionKind::WhipPan => push(incoming, eased(eased(progress))),
        TransitionKind::ZoomIn => LayerEffect {
            opacity: if incoming { progress } else { 1.0 },
            frame_zoom: if incoming { 1.0 } else { 1.0 + progress },
            ..base
        },
        TransitionKind::ZoomOut => LayerEffect {
            opacity: if incoming { progress } else { 1.0 },
            frame_zoom: if incoming { 1.5 - progress / 2.0 } else { 1.0 },
            ..base
        },
        TransitionKind::Crossfade | TransitionKind::Slide => base,
        _ if incoming => LayerEffect {
            opacity: if progress >= 0.5 { 1.0 } else { 0.0 },
            ..base
        },
        _ => base,
    }
}

fn push(incoming: bool, progress: f32) -> LayerEffect {
    let shift = if incoming { 1.0 - progress } else { -progress };
    LayerEffect {
        shift: [shift, 0.0],
        ..LayerEffect::default()
    }
}

fn layer_effect(
    index: usize,
    transition: Option<TransitionState>,
    color: ColorAdjust,
) -> LayerEffect {
    LayerEffect {
        brightness: color.brightness,
        contrast: color.contrast,
        saturation: color.saturation,
        ..transition.map_or_else(LayerEffect::default, |state| {
            transition_effect(index, state)
        })
    }
}

impl FrameComposer {
    pub fn new(compositor: Compositor) -> Self {
        let offline = compositor.texture_from_rgba(1, 1, &OFFLINE_RGBA);
        Self {
            compositor,
            videos: HashMap::new(),
            images: HashMap::new(),
            offline,
            painter: None,
            caption: None,
            overlays: HashMap::new(),
        }
    }

    /// Draws each overlay's picture when its look changes, then moves animations and videos to
    /// this frame's time. Pictures of removed overlays go.
    fn prepare_overlays(&mut self, project: &Project, folder: &Path, frame: &FrameDescription) {
        self.overlays
            .retain(|id, _| project.timeline.overlays.iter().any(|item| item.id == *id));
        for layer in &frame.overlays {
            let Some(item) = project
                .timeline
                .overlays
                .iter()
                .find(|item| item.id == layer.overlay_id)
            else {
                continue;
            };
            let current = self
                .overlays
                .get(&item.id)
                .is_some_and(|(kind, _)| *kind == item.kind);
            if !current {
                if let Some(picture) = self.load_overlay(project, folder, &item.kind) {
                    self.overlays.insert(item.id, (item.kind.clone(), picture));
                }
            }
            match self.overlays.get_mut(&item.id) {
                Some((
                    _,
                    OverlayPicture::Moving {
                        animation,
                        shown,
                        texture,
                    },
                )) => {
                    let index = animation.frame_at(frame.time_us - item.start_us);
                    if index != *shown {
                        let (width, height) = (animation.width, animation.height);
                        self.compositor.write_rgba(
                            texture,
                            width,
                            height,
                            &animation.frames[index].1,
                        );
                        *shown = index;
                    }
                }
                Some((_, OverlayPicture::Video(source))) => {
                    source.seek_to(layer.source_time_us.unwrap_or(0), &self.compositor);
                }
                _ => {}
            }
        }
    }

    fn load_overlay(
        &self,
        project: &Project,
        folder: &Path,
        kind: &OverlayKind,
    ) -> Option<OverlayPicture> {
        let file = |id: &Id| {
            project
                .media
                .iter()
                .find(|media| media.id == *id)
                .and_then(|media| resolve(media, folder))
        };
        let still = |(width, height, rgba): (u32, u32, Vec<u8>)| {
            OverlayPicture::Still(self.compositor.texture_from_rgba(width, height, &rgba))
        };
        match kind {
            OverlayKind::Shape { shape, fill } => {
                Some(still(shape_pixels(*shape, parse_color(fill))))
            }
            OverlayKind::Image { asset } => file(asset).and_then(|p| load_image(&p)).map(still),
            OverlayKind::AnimatedImage { asset } => {
                let animation = decode_animation(&file(asset)?).ok()?;
                let texture = self.compositor.texture_from_rgba(
                    animation.width,
                    animation.height,
                    &animation.frames[0].1,
                );
                Some(OverlayPicture::Moving {
                    animation,
                    shown: 0,
                    texture,
                })
            }
            OverlayKind::Video { media, .. } => VideoSource::open(&file(media)?)
                .map(|source| OverlayPicture::Video(Box::new(source))),
            // Text comes with T-0705.
            OverlayKind::Text { .. } => None,
        }
    }

    fn placed_overlay(
        &self,
        project: &Project,
        layer: &OverlayLayer,
    ) -> Option<(&wgpu::Texture, LayerPlacement)> {
        let (kind, picture) = self.overlays.get(&layer.overlay_id)?;
        let texture = match picture {
            OverlayPicture::Still(texture) | OverlayPicture::Moving { texture, .. } => texture,
            OverlayPicture::Video(source) => &source.shown.as_ref()?.texture,
        };
        let (width, height) = natural_size(kind, project);
        let sprite = Sprite {
            center: [
                layer.position.x / CANVAS_WIDTH,
                layer.position.y / CANVAS_HEIGHT,
            ],
            size: [
                width * layer.scale / CANVAS_WIDTH,
                height * layer.scale / CANVAS_HEIGHT,
            ],
            rotation: layer.rotation,
            flip: layer.flip,
        };
        Some((
            texture,
            LayerPlacement {
                width: self.compositor.width,
                height: self.compositor.height,
                quarter_turns: 0,
                center: [0.5, 0.5],
                zoom: 1.0,
                effect: LayerEffect {
                    opacity: layer.opacity,
                    keeps_alpha: true,
                    sprite: Some(sprite),
                    ..LayerEffect::default()
                },
            },
        ))
    }

    fn prepare(&mut self, project: &Project, folder: &Path, layer: &ClipLayer) {
        let Some(media) = project.media.iter().find(|m| m.id == layer.media_id) else {
            return;
        };
        match media.kind {
            MediaKind::Video => {
                if let Entry::Vacant(slot) = self.videos.entry(media.id) {
                    if let Some(source) = resolve(media, folder).and_then(|p| VideoSource::open(&p))
                    {
                        slot.insert(source);
                    }
                }
                if let Some(source) = self.videos.get_mut(&media.id) {
                    source.seek_to(layer.source_time_us, &self.compositor);
                }
            }
            _ => {
                if let Entry::Vacant(slot) = self.images.entry(media.id) {
                    if let Some((width, height, rgba)) =
                        resolve(media, folder).and_then(|p| load_image(&p))
                    {
                        let texture = self.compositor.texture_from_rgba(width, height, &rgba);
                        slot.insert(Picture {
                            texture,
                            width,
                            height,
                        });
                    }
                }
            }
        }
    }

    fn placed(
        &self,
        media: Option<&MediaRef>,
        layer: &ClipLayer,
        effect: LayerEffect,
    ) -> (&wgpu::Texture, LayerPlacement) {
        let crop = LayerPlacement {
            width: 1,
            height: 1,
            quarter_turns: 0,
            center: [layer.crop.center.x, layer.crop.center.y],
            zoom: layer.crop.scale,
            effect,
        };
        let Some(media) = media else {
            return (&self.offline, crop);
        };
        let quarter_turns = (media.probe.rotation.rem_euclid(360) / 90) as u32;
        if let Some(shown) = self.videos.get(&media.id).and_then(|v| v.shown.as_ref()) {
            let info = &self.videos[&media.id].info;
            let placement = LayerPlacement {
                width: info.width,
                height: info.height,
                quarter_turns,
                ..crop
            };
            return (&shown.texture, placement);
        }
        if let Some(picture) = self.images.get(&media.id) {
            let placement = LayerPlacement {
                width: picture.width,
                height: picture.height,
                ..crop
            };
            return (&picture.texture, placement);
        }
        (&self.offline, crop)
    }

    /// Draws the caption of this frame once, then keeps it for as long as the words stay.
    fn prepare_caption(&mut self, project: &Project, frame: &FrameDescription) {
        if frame.captions.is_empty() {
            return;
        }
        let karaoke = project.captions.preset == CaptionPreset::Karaoke;
        let words = caption_words(frame, karaoke, project.captions.animation);
        let look = caption_look(project);
        let key = format!("{words:?}{look:?}");
        if self
            .caption
            .as_ref()
            .is_some_and(|(drawn, _)| *drawn == key)
        {
            return;
        }
        let (width, height) = (self.compositor.width, self.compositor.height);
        let painter = self.painter.get_or_insert_with(CaptionPainter::new);
        let pixels = painter.paint(&words, &look, width, height);
        let texture = self.compositor.texture_from_rgba(width, height, &pixels);
        self.caption = Some((key, texture));
    }

    /// Draws `project` at `time_us` into `target`, and waits for the GPU.
    pub fn draw(
        &mut self,
        project: Option<&Project>,
        folder: &Path,
        time_us: i64,
        target: &wgpu::Texture,
    ) -> Result<(), RenderError> {
        let frame = project.map(|p| (p, evaluate(p, time_us)));
        if let Some((project, frame)) = &frame {
            for layer in &frame.clips {
                self.prepare(project, folder, layer);
            }
            self.prepare_overlays(project, folder, frame);
            self.prepare_caption(project, frame);
        }
        let mut layers = Vec::with_capacity(2);
        if let Some((project, frame)) = &frame {
            for (index, layer) in frame.clips.iter().enumerate() {
                let media = project.media.iter().find(|m| m.id == layer.media_id);
                let effect = layer_effect(index, frame.transition, layer.color);
                layers.push(self.placed(media, layer, effect));
            }
            for layer in &frame.overlays {
                layers.extend(self.placed_overlay(project, layer));
            }
            if let Some((_, caption)) = self.caption.as_ref().filter(|_| !frame.captions.is_empty())
            {
                let anchor = project.captions.vertical_anchor;
                let effect =
                    caption_effect(project.captions.animation, frame.caption_age_us, anchor);
                layers.push((
                    caption,
                    LayerPlacement {
                        width: self.compositor.width,
                        height: self.compositor.height,
                        quarter_turns: 0,
                        center: [0.5, 0.5],
                        zoom: 1.0,
                        effect,
                    },
                ));
            }
        }
        self.compositor.render(&layers, target)
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;

    fn at(kind: TransitionKind, index: usize, progress: f32) -> LayerEffect {
        transition_effect(index, TransitionState { kind, progress })
    }

    #[test]
    fn every_offered_transition_starts_on_the_old_clip_and_ends_on_the_new_one() {
        for kind in [
            TransitionKind::Crossfade,
            TransitionKind::DipToBlack,
            TransitionKind::DipToWhite,
            TransitionKind::Flash,
            TransitionKind::Slide,
            TransitionKind::Push,
            TransitionKind::WhipPan,
            TransitionKind::ZoomIn,
            TransitionKind::ZoomOut,
        ] {
            let incoming = at(kind, 1, 0.0);
            let hidden = incoming.opacity == 0.0 || incoming.shift[0] == 1.0;
            assert!(hidden, "{kind:?} shows the new clip too early");
            let end = at(kind, 1, 1.0);
            assert_eq!(end.opacity, 1.0, "{kind:?}");
            assert_eq!(end.shift, [0.0, 0.0], "{kind:?}");
            assert_eq!(end.tint[3], 0.0, "{kind:?}");
            assert_eq!(end.frame_zoom, 1.0, "{kind:?}");
        }
    }

    #[test]
    fn a_pop_ends_at_full_size_and_keeps_the_block_in_place() {
        let start = caption_effect(CaptionAnimation::Pop, 0, 0.7);
        assert_eq!(start.frame_zoom, POP_START);
        let moved = (0.7 - 0.5) / start.frame_zoom + 0.5 - start.shift[1];
        assert!((moved - 0.7).abs() < 1e-5);
        let end = caption_effect(CaptionAnimation::Pop, POP_US, 0.7);
        assert_eq!(end.frame_zoom, 1.0);
        assert!(end.keeps_alpha);
    }

    #[test]
    fn a_typewriter_reveals_characters_in_order() {
        let frame = FrameDescription {
            time_us: 0,
            clips: Vec::new(),
            transition: None,
            overlays: Vec::new(),
            captions: vec![
                CaptionRun {
                    text: "ab".into(),
                    emphasis: false,
                    active: true,
                },
                CaptionRun {
                    text: "cd".into(),
                    emphasis: true,
                    active: false,
                },
            ],
            caption_age_us: TYPEWRITER_US_PER_CHARACTER * 2,
        };
        let shown = caption_words(&frame, false, CaptionAnimation::Typewriter);
        assert_eq!(shown.len(), 2);
        assert_eq!(
            (shown[0].text.as_str(), shown[1].text.as_str()),
            ("ab", "c")
        );
        let all = caption_words(&frame, false, CaptionAnimation::Pop);
        assert_eq!(all[1].text, "cd");
    }

    #[test]
    fn a_fade_ends_opaque() {
        assert_eq!(caption_effect(CaptionAnimation::Fade, 0, 0.7).opacity, 0.0);
        assert_eq!(
            caption_effect(CaptionAnimation::Fade, FADE_US, 0.7).opacity,
            1.0
        );
        assert_eq!(caption_effect(CaptionAnimation::None, 0, 0.7).opacity, 1.0);
    }

    #[test]
    fn a_dip_reaches_its_colour_halfway() {
        assert_eq!(at(TransitionKind::DipToWhite, 0, 0.25).tint[3], 0.5);
        assert_eq!(at(TransitionKind::DipToWhite, 0, 0.5).tint[3], 1.0);
        assert_eq!(at(TransitionKind::DipToWhite, 1, 0.5).tint[3], 1.0);
        assert_eq!(at(TransitionKind::DipToWhite, 1, 0.75).tint[3], 0.5);
        assert_eq!(
            at(TransitionKind::DipToBlack, 0, 0.5).tint,
            [0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn a_push_moves_both_clips_together() {
        let (out, new) = (
            at(TransitionKind::Push, 0, 0.5),
            at(TransitionKind::Push, 1, 0.5),
        );
        assert_eq!(new.shift[0] - out.shift[0], 1.0);
    }

    #[test]
    fn colour_applies_with_or_without_a_transition() {
        let color = ColorAdjust {
            brightness: 0.1,
            contrast: 1.2,
            saturation: 0.5,
        };
        let plain = layer_effect(0, None, color);
        assert_eq!((plain.brightness, plain.opacity), (0.1, 1.0));
        let state = TransitionState {
            kind: TransitionKind::Crossfade,
            progress: 0.25,
        };
        let faded = layer_effect(1, Some(state), color);
        assert_eq!((faded.saturation, faded.opacity), (0.5, 0.25));
    }
}
