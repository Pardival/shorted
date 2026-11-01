//! Preview frames on Apple platforms: decoded video, composed by the shared compositor, into
//! surfaces shown by the Flutter texture. Called by the platform plugin on its own queue.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::ptr;

use objc2_core_foundation::CFRetained;
use objc2_io_surface::IOSurfaceRef;
use shorted_core::evaluate::{evaluate, ClipLayer, TransitionState};
use shorted_core::model::{Id, MediaKind, MediaRef, Project, TransitionKind};
use shorted_media::apple::{AppleBackend, AppleDecoder, AppleFrame};
use shorted_media::{DecodedFrame, MediaBackend, MediaInfo, VideoDecoder};
use shorted_render::apple::texture_from_iosurface;
use shorted_render::compositor::{load_image, Compositor, LayerPlacement};

use crate::api::media::resolve;
use crate::transport::transport;

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

pub struct PreviewSession {
    compositor: Compositor,
    surfaces: Vec<wgpu::Texture>,
    videos: HashMap<Id, VideoSource>,
    images: HashMap<Id, Picture>,
    offline: wgpu::Texture,
    drawn: Option<(i64, u64)>,
}

fn layer_opacity(index: usize, transition: Option<TransitionState>) -> f32 {
    match transition {
        Some(state) if index > 0 => match state.kind {
            TransitionKind::Crossfade => state.progress,
            // The other kinds are drawn as a cut until T-0709.
            _ if state.progress >= 0.5 => 1.0,
            _ => 0.0,
        },
        _ => 1.0,
    }
}

impl PreviewSession {
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
        opacity: f32,
    ) -> (&wgpu::Texture, LayerPlacement) {
        let crop = LayerPlacement {
            width: 1,
            height: 1,
            quarter_turns: 0,
            center: [layer.crop.center.x, layer.crop.center.y],
            zoom: layer.crop.scale,
            opacity,
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

    /// Draws the frame for the transport's time. False if the surface already shows it.
    fn render(&mut self, index: usize) -> bool {
        let (project, folder, time_us, revision) = {
            let mut state = transport();
            let time = state.position_us();
            (
                state.project.clone(),
                state.folder.clone(),
                time,
                state.revision,
            )
        };
        if self.drawn == Some((time_us, revision)) {
            return false;
        }
        let Some(target) = self.surfaces.get(index) else {
            return false;
        };
        let target = target.clone();
        let frame = project.as_deref().map(|p| (p, evaluate(p, time_us)));
        let mut layers = Vec::with_capacity(2);
        if let Some((project, frame)) = &frame {
            for layer in &frame.clips {
                self.prepare(project, &folder, layer);
            }
            for (index, layer) in frame.clips.iter().enumerate() {
                let media = project.media.iter().find(|m| m.id == layer.media_id);
                layers.push(self.placed(media, layer, layer_opacity(index, frame.transition)));
            }
        }
        let drawn: Vec<(&wgpu::Texture, LayerPlacement)> = layers;
        if self.compositor.render(&drawn, &target).is_err() {
            return false;
        }
        self.drawn = Some((time_us, revision));
        true
    }
}

/// Returns a session, or null without a GPU.
#[no_mangle]
pub extern "C" fn shorted_preview_create(width: u32, height: u32) -> *mut PreviewSession {
    match Compositor::new(width, height) {
        Ok(compositor) => {
            let offline = compositor.texture_from_rgba(1, 1, &OFFLINE_RGBA);
            Box::into_raw(Box::new(PreviewSession {
                compositor,
                surfaces: Vec::new(),
                videos: HashMap::new(),
                images: HashMap::new(),
                offline,
                drawn: None,
            }))
        }
        Err(error) => {
            tracing::error!(%error, "preview failed to start");
            ptr::null_mut()
        }
    }
}

/// # Safety
///
/// `session` must come from [`shorted_preview_create`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn shorted_preview_destroy(session: *mut PreviewSession) {
    if !session.is_null() {
        // SAFETY: the caller hands back the unique pointer from `Box::into_raw`.
        drop(unsafe { Box::from_raw(session) });
    }
}

/// Registers a target surface; false on failure.
///
/// # Safety
///
/// `session` must be live; `surface` must be a BGRA8 IOSurface of the session's size that outlives
/// the session.
#[no_mangle]
pub unsafe extern "C" fn shorted_preview_attach_surface(
    session: *mut PreviewSession,
    surface: *const c_void,
) -> bool {
    // SAFETY: guaranteed by the caller, and the plugin's queue is the only user.
    let (session, surface) = unsafe { (&mut *session, &*surface.cast::<IOSurfaceRef>()) };
    let compositor = &session.compositor;
    // SAFETY: format, size and lifetime are guaranteed by the caller.
    let texture = unsafe {
        texture_from_iosurface(
            &compositor.device,
            surface,
            compositor.width,
            compositor.height,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
    };
    match texture {
        Ok(texture) => {
            session.surfaces.push(texture);
            true
        }
        Err(error) => {
            tracing::error!(%error, "preview surface could not be shared");
            false
        }
    }
}

/// Draws into surface `index` if the frame changed; true if it did.
///
/// # Safety
///
/// `session` must be live and used from one thread at a time.
#[no_mangle]
pub unsafe extern "C" fn shorted_preview_render(session: *mut PreviewSession, index: u32) -> bool {
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *session }.render(index as usize)
}
