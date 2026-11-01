//! Export (T-0901): a deterministic loop over the frames of a project snapshot.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use shorted_core::model::{Project, MAX_PROJECT_DURATION_MS};

use crate::error::{EngineError, ErrorCode};
use crate::frb_generated::StreamSink;
use crate::runtime::{self, Priority, TaskContext, TaskEvent};
use crate::transport::transport;

/// Reference bitrate: 1080×1920 at 30 fps.
const REFERENCE_BITS_PER_SECOND: f64 = 12_000_000.0;
const REFERENCE_PIXEL_RATE: f64 = 1080.0 * 1920.0 * 30.0;

#[derive(Clone, Copy)]
pub struct ExportSettings {
    /// Short side: 720, 1080 or 2160.
    pub width: u32,
    /// 30 or 60.
    pub fps: u32,
}

fn output_size(width: u32) -> (u32, u32) {
    // Encoders want even sizes; 16:9 of the listed widths already is.
    (width, (width * 16 / 9) & !1)
}

fn bits_per_second(width: u32, height: u32, fps: u32) -> u32 {
    let rate = f64::from(width) * f64::from(height) * f64::from(fps);
    (REFERENCE_BITS_PER_SECOND * rate / REFERENCE_PIXEL_RATE) as u32
}

fn frame_count(duration_us: i64, fps: u32) -> i64 {
    (duration_us as f64 * f64::from(fps) / 1e6).ceil() as i64
}

fn frame_time_us(frame: i64, fps: u32) -> i64 {
    (frame as f64 * 1e6 / f64::from(fps)).round() as i64
}

#[cfg(target_vendor = "apple")]
fn run_export(
    project: &Project,
    folder: &Path,
    output: &Path,
    settings: ExportSettings,
    task: &TaskContext<'_>,
) -> Result<(), EngineError> {
    use shorted_media::apple_writer::AppleEncoder;
    use shorted_render::apple::texture_from_iosurface;
    use shorted_render::compositor::Compositor;

    use crate::frame_composer::FrameComposer;

    let (width, height) = output_size(settings.width);
    let failed = |_| EngineError::ExportFailed;
    let compositor = Compositor::new(width, height).map_err(|_| EngineError::ExportFailed)?;
    let mut composer = FrameComposer::new(compositor);
    let encoder = AppleEncoder::new(
        output,
        width,
        height,
        bits_per_second(width, height, settings.fps),
    )
    .map_err(failed)?;
    let frames = frame_count(project.duration_us(), settings.fps);
    for index in 0..frames {
        if task.check_cancelled().is_err() {
            encoder.cancel();
            let _ = std::fs::remove_file(output);
            return Err(EngineError::Cancelled);
        }
        let frame = encoder.next_frame().map_err(failed)?;
        let time_us = frame_time_us(index, settings.fps);
        // SAFETY: the encoder's buffers are BGRA8 of this size; `frame` outlives the target.
        let target = unsafe {
            texture_from_iosurface(
                &composer.compositor.device,
                &frame.surface,
                width,
                height,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
        }
        .map_err(|_| EngineError::ExportFailed)?;
        composer
            .draw(Some(project), folder, time_us, &target)
            .map_err(|_| EngineError::ExportFailed)?;
        encoder.append(&frame, time_us).map_err(failed)?;
        task.progress((index + 1) as f64 / frames as f64);
    }
    encoder.finish().map_err(failed)
}

/// Needs an encoder for this platform (T-0901).
#[cfg(not(target_vendor = "apple"))]
fn run_export(
    _project: &Project,
    _folder: &Path,
    _output: &Path,
    _settings: ExportSettings,
    _task: &TaskContext<'_>,
) -> Result<(), EngineError> {
    Err(EngineError::ExportFailed)
}

/// Starts exporting the open project. The first event carries the id for `cancel_task`.
pub fn start_export(path: String, settings: ExportSettings, events: StreamSink<TaskEvent>) {
    let (project, folder): (Option<Arc<Project>>, PathBuf) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            let project = project.ok_or(EngineError::ExportFailed)?;
            if project.duration_us() == 0 || project.duration_us() > MAX_PROJECT_DURATION_MS * 1000
            {
                return Err(EngineError::ExportFailed);
            }
            run_export(&project, &folder, Path::new(&path), settings, task)
        });
}

/// The code an export ends on when the project cannot be exported as it is.
#[flutter_rust_bridge::frb(sync)]
pub fn export_blocked(duration_us: i64) -> Option<ErrorCode> {
    (duration_us == 0 || duration_us > MAX_PROJECT_DURATION_MS * 1000)
        .then_some(ErrorCode::ExportFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_stay_even_and_9_16() {
        assert_eq!(output_size(1080), (1080, 1920));
        assert_eq!(output_size(720), (720, 1280));
        assert_eq!(output_size(2160), (2160, 3840));
    }

    #[test]
    fn bitrate_scales_with_pixels_and_frames() {
        assert_eq!(bits_per_second(1080, 1920, 30), 12_000_000);
        assert_eq!(bits_per_second(1080, 1920, 60), 24_000_000);
    }

    #[test]
    fn frames_cover_the_whole_duration_on_a_regular_grid() {
        assert_eq!(frame_count(1_000_000, 30), 30);
        assert_eq!(frame_count(1_000_001, 30), 31);
        assert_eq!(frame_count(10_000_000, 60), 600);
        assert_eq!(frame_time_us(1, 30), 33_333);
        assert_eq!(frame_time_us(30, 30), 1_000_000);
    }

    #[test]
    fn empty_or_too_long_projects_are_blocked() {
        assert!(export_blocked(0).is_some());
        assert!(export_blocked(120_000_001).is_some());
        assert!(export_blocked(120_000_000).is_none());
    }
}
