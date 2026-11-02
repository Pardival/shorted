//! Preview frames on Apple platforms, shown by the Flutter texture. Called by the platform plugin
//! on its own queue.

use std::ffi::c_void;
use std::ptr;

use objc2_io_surface::IOSurfaceRef;
use shorted_render::apple::texture_from_iosurface;
use shorted_render::compositor::Compositor;

use crate::frame_composer::FrameComposer;
use crate::transport::transport;

pub struct PreviewSession {
    composer: FrameComposer,
    surfaces: Vec<wgpu::Texture>,
    drawn: Option<(i64, u64)>,
}

impl PreviewSession {
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
        if self
            .composer
            .draw(project.as_deref(), &folder, time_us, target)
            .is_err()
        {
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
        Ok(compositor) => Box::into_raw(Box::new(PreviewSession {
            composer: FrameComposer::new(compositor),
            surfaces: Vec::new(),
            drawn: None,
        })),
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
    let compositor = &session.composer.compositor;
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
