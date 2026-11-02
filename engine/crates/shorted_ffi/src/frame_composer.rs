//! Composes project frames from decoded media on Apple platforms.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use objc2_core_foundation::CFRetained;
use objc2_io_surface::IOSurfaceRef;
use shorted_core::evaluate::{evaluate, ClipLayer, TransitionState};
use shorted_core::model::{ColorAdjust, Id, MediaKind, MediaRef, Project, TransitionKind};
use shorted_media::apple::{AppleBackend, AppleDecoder, AppleFrame};
use shorted_media::{DecodedFrame, MediaBackend, MediaInfo, VideoDecoder};
use shorted_render::apple::texture_from_iosurface;
use shorted_render::compositor::{load_image, Compositor, LayerEffect, LayerPlacement};
use shorted_render::spike::RenderError;

use crate::api::media::resolve;

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
        }
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
        }
        let mut layers = Vec::with_capacity(2);
        if let Some((project, frame)) = &frame {
            for (index, layer) in frame.clips.iter().enumerate() {
                let media = project.media.iter().find(|m| m.id == layer.media_id);
                let effect = layer_effect(index, frame.transition, layer.color);
                layers.push(self.placed(media, layer, effect));
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
