//! C entry points for the T-0005 spike, called by the platform texture plugin on its own queue.
//! Spike only: they bypass `api` because they must run outside the Dart isolate.

#[cfg(target_vendor = "apple")]
use std::ffi::c_void;
use std::ptr;

use shorted_render::spike::SpikeRenderer;

pub struct SpikeSession {
    renderer: SpikeRenderer,
    surfaces: Vec<wgpu::Texture>,
}

/// Returns a new session, or null if no GPU device could be created.
#[no_mangle]
pub extern "C" fn shorted_spike_create(width: u32, height: u32) -> *mut SpikeSession {
    match SpikeRenderer::new(width, height) {
        Ok(renderer) => Box::into_raw(Box::new(SpikeSession {
            renderer,
            surfaces: Vec::new(),
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
    let Some(target) = session.surfaces.get(index as usize) else {
        return false;
    };
    session.renderer.render_to(target, time).is_ok()
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
