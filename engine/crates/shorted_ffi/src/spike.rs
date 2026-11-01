//! C entry points for the T-0005 spike, called by the platform texture plugin on its own queue.
//! Spike only: they bypass `api` because they must run outside the Dart isolate.

#[cfg(target_vendor = "apple")]
use std::ffi::c_void;
use std::ptr;

#[cfg(target_vendor = "apple")]
use objc2_core_foundation::CFRetained;
#[cfg(target_vendor = "apple")]
use objc2_io_surface::IOSurfaceRef;
#[cfg(target_vendor = "apple")]
use shorted_media::apple::{AppleBackend, AppleDecoder, AppleFrame};
#[cfg(target_vendor = "apple")]
use shorted_media::{DecodedFrame, MediaBackend, MediaInfo, VideoDecoder};
use shorted_render::spike::SpikeRenderer;

pub struct SpikeSession {
    renderer: SpikeRenderer,
    surfaces: Vec<wgpu::Texture>,
    #[cfg(target_vendor = "apple")]
    video: Option<VideoSource>,
}

/// T-0006: decoded clip behind the scene. Frames are decoded on the render queue; real prefetch
/// comes with the playback engine (T-0403).
#[cfg(target_vendor = "apple")]
struct VideoSource {
    decoder: AppleDecoder,
    info: MediaInfo,
    pending: Option<AppleFrame>,
    /// Added to the clip's own time when it loops.
    loop_start_us: i64,
    // Keep the shown frame alive while the GPU may sample it.
    shown: Option<(AppleFrame, CFRetained<IOSurfaceRef>, wgpu::Texture)>,
}

/// Returns a new session, or null if no GPU device could be created.
#[no_mangle]
pub extern "C" fn shorted_spike_create(width: u32, height: u32) -> *mut SpikeSession {
    match SpikeRenderer::new(width, height) {
        Ok(renderer) => Box::into_raw(Box::new(SpikeSession {
            renderer,
            surfaces: Vec::new(),
            #[cfg(target_vendor = "apple")]
            video: None,
        })),
        Err(error) => {
            tracing::error!(%error, "spike renderer failed to start");
            ptr::null_mut()
        }
    }
}

/// # Safety
///
/// `session` must come from [`shorted_spike_create`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn shorted_spike_destroy(session: *mut SpikeSession) {
    if !session.is_null() {
        // SAFETY: the caller hands back the unique pointer from `Box::into_raw`.
        drop(unsafe { Box::from_raw(session) });
    }
}

/// Registers a shared surface and returns its index, or -1.
///
/// # Safety
///
/// `session` must be live; `surface` must be a BGRA8 IOSurface of the session's size that outlives
/// the session.
#[cfg(target_vendor = "apple")]
#[no_mangle]
pub unsafe extern "C" fn shorted_spike_attach_surface(
    session: *mut SpikeSession,
    surface: *const c_void,
) -> i32 {
    // SAFETY: the caller guarantees both pointers, and the plugin's queue is the only user.
    let (session, surface) = unsafe {
        (
            &mut *session,
            &*surface.cast::<objc2_io_surface::IOSurfaceRef>(),
        )
    };
    let renderer = &session.renderer;
    // SAFETY: format, size and lifetime of `surface` are guaranteed by the caller.
    let texture = unsafe {
        shorted_render::apple::texture_from_iosurface(
            &renderer.device,
            surface,
            renderer.width,
            renderer.height,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
    };
    match texture {
        Ok(texture) => {
            session.surfaces.push(texture);
            session.surfaces.len() as i32 - 1
        }
        Err(error) => {
            tracing::error!(%error, "spike surface could not be shared");
            -1
        }
    }
}

/// Renders into an attached surface and waits for the GPU.
///
/// # Safety
///
/// `session` must be live and used from one thread at a time.
#[no_mangle]
pub unsafe extern "C" fn shorted_spike_render_surface(
    session: *mut SpikeSession,
    index: u32,
    time: f32,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let session = unsafe { &mut *session };
    #[cfg(target_vendor = "apple")]
    if !session.advance_video(time) {
        return false;
    }
    let Some(target) = session.surfaces.get(index as usize) else {
        return false;
    };
    session.renderer.render_to(target, time).is_ok()
}

/// Plays a clip behind the scene. `path` is a UTF-8 string of `length` bytes.
///
/// # Safety
///
/// `session` must be live and `path` valid for reads of `length` bytes.
#[cfg(target_vendor = "apple")]
#[no_mangle]
pub unsafe extern "C" fn shorted_spike_open_video(
    session: *mut SpikeSession,
    path: *const u8,
    length: usize,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (session, path) = unsafe { (&mut *session, std::slice::from_raw_parts(path, length)) };
    let Ok(path) = std::str::from_utf8(path) else {
        return false;
    };
    let path = std::path::Path::new(path);
    let (Ok(info), Ok(decoder)) = (AppleBackend.probe(path), AppleBackend.open_decoder(path))
    else {
        tracing::error!("spike clip could not be opened");
        return false;
    };
    session.video = Some(VideoSource {
        decoder,
        info,
        pending: None,
        loop_start_us: 0,
        shown: None,
    });
    true
}

#[cfg(target_vendor = "apple")]
impl SpikeSession {
    /// Moves the clip to the frame for `time`, looping at the end. Late frames are skipped by
    /// timestamp, so variable frame rate clips stay on time.
    fn advance_video(&mut self, time: f32) -> bool {
        let Some(video) = &mut self.video else {
            return true;
        };
        let time_us = (f64::from(time) * 1e6) as i64;
        let mut latest = None;
        loop {
            let frame = match video.pending.take() {
                Some(frame) => Some(frame),
                None => match video.decoder.next_frame() {
                    Ok(frame) => frame,
                    Err(_) => return false,
                },
            };
            let Some(frame) = frame else {
                video.loop_start_us += video.info.duration_us;
                if video.decoder.seek(0).is_err() {
                    return false;
                }
                continue;
            };
            if frame.pts_us() + video.loop_start_us > time_us {
                video.pending = Some(frame);
                break;
            }
            latest = Some(frame);
        }
        let Some(frame) = latest else {
            return true;
        };
        let Some(surface) = frame.surface() else {
            return false;
        };
        let renderer = &mut self.renderer;
        // SAFETY: decoder surfaces are BGRA8 of the probed size, and `shown` keeps them alive.
        let texture = unsafe {
            shorted_render::apple::texture_from_iosurface(
                &renderer.device,
                &surface,
                video.info.width,
                video.info.height,
                wgpu::TextureUsages::TEXTURE_BINDING,
            )
        };
        let Ok(texture) = texture else {
            return false;
        };
        let (width, height) = video.info.display_size();
        let rotated = video.info.rotation_degrees.rem_euclid(180) == 90;
        renderer.set_video_frame(&texture, width, height, rotated);
        video.shown = Some((frame, surface, texture));
        true
    }
}

/// Renders and copies BGRA pixels to `destination`: the pixel-buffer path.
///
/// # Safety
///
/// `session` must be live and used from one thread at a time; `destination` must be valid for
/// writes of `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn shorted_spike_render_bytes(
    session: *mut SpikeSession,
    time: f32,
    destination: *mut u8,
    length: usize,
    stride: usize,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (session, destination) = unsafe {
        (
            &mut *session,
            std::slice::from_raw_parts_mut(destination, length),
        )
    };
    session
        .renderer
        .render_to_bytes(time, destination, stride)
        .is_ok()
}
