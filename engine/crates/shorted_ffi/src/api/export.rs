//! Export (T-0901): a deterministic loop over the frames of a project snapshot.

// Size and timing helpers only serve the Apple encoder until other platforms have one.
#![cfg_attr(not(target_vendor = "apple"), allow(dead_code))]

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use shorted_audio::mixer::us_to_frames;
use shorted_core::model::{Project, MAX_PROJECT_DURATION_MS};

use crate::error::{EngineError, EngineFailure, ErrorCode};
use crate::frb_generated::StreamSink;
use crate::runtime::{self, Priority, TaskContext, TaskEvent};
use crate::transport::transport;

/// Reference bitrate: 1080×1920 at 30 fps.
const REFERENCE_BITS_PER_SECOND: f64 = 12_000_000.0;
const REFERENCE_PIXEL_RATE: f64 = 1080.0 * 1920.0 * 30.0;
const AUDIO_CHUNK_FRAMES: i64 = 1_024;
const AUDIO_BITS_PER_SECOND: f64 = 192_000.0;
/// How often a paused export looks whether it may go on.
const PAUSE_POLL: Duration = Duration::from_millis(50);

/// Set while a phone app is in the background, where the GPU may not be used (FR-EXP-06).
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Pauses or resumes the running export. Phones call it when the app leaves or comes back.
#[flutter_rust_bridge::frb(sync)]
pub fn set_export_paused(paused: bool) {
    PAUSED.store(paused, Ordering::SeqCst);
}

/// Waits while the export is paused. Cancelling still works meanwhile.
fn wait_while_paused(task: &TaskContext<'_>) -> Result<(), EngineError> {
    while PAUSED.load(Ordering::SeqCst) {
        task.check_cancelled().map_err(|_| EngineError::Cancelled)?;
        std::thread::sleep(PAUSE_POLL);
    }
    Ok(())
}

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

/// How far the sound should have reached once video frame `index` is written.
fn audio_target(index: i64, frames: i64, fps: u32, audio_frames: i64) -> i64 {
    if index + 1 >= frames {
        audio_frames
    } else {
        us_to_frames(frame_time_us(index + 1, fps)).min(audio_frames)
    }
}

/// The sound frames that fit the next chunk, from `next` up to `limit`.
fn audio_chunk(next: i64, limit: i64) -> Range<i64> {
    next..(next + AUDIO_CHUNK_FRAMES).min(limit)
}

#[cfg(target_vendor = "apple")]
fn run_export(
    project: &Project,
    folder: &Path,
    output: &Path,
    settings: ExportSettings,
    task: &TaskContext<'_>,
) -> Result<(), EngineError> {
    use std::collections::HashMap;

    use shorted_audio::mixer::{mix, CHANNELS};
    use shorted_media::apple_writer::AppleEncoder;
    use shorted_media::MediaError;
    use shorted_render::apple::texture_from_iosurface;
    use shorted_render::compositor::Compositor;

    use crate::audio_output::load_sources;
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
    let audio_frames = us_to_frames(project.duration_us());
    let mut sources = HashMap::new();
    load_sources(project, folder, &mut sources);
    let mut sound = Vec::new();
    let mut next_audio = 0;
    // Writes what the encoder will take right now, without ever waiting for it. Returns how far
    // the sound has reached.
    let mut feed_audio = |limit: i64| -> Result<i64, MediaError> {
        while next_audio < limit && encoder.audio_ready() {
            let chunk = audio_chunk(next_audio, limit);
            sound.resize((chunk.end - chunk.start) as usize * CHANNELS, 0.0);
            mix(project, &sources, chunk.start, &mut sound);
            encoder.append_audio(&sound, chunk.start)?;
            next_audio = chunk.end;
        }
        if next_audio == audio_frames {
            encoder.finish_audio();
        }
        Ok(next_audio)
    };
    for index in 0..frames {
        if task.check_cancelled().is_err() || wait_while_paused(task).is_err() {
            encoder.cancel();
            let _ = std::fs::remove_file(output);
            return Err(EngineError::Cancelled);
        }
        feed_audio(audio_target(index, frames, settings.fps, audio_frames)).map_err(failed)?;
        encoder
            .wait_for_video(|| feed_audio(audio_frames).map(drop))
            .map_err(failed)?;
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
    encoder.finish_video();
    while feed_audio(audio_frames).map_err(failed)? < audio_frames {
        encoder.wait_for_audio().map_err(failed)?;
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
    set_export_paused(false);
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            let project = project.ok_or(EngineError::ExportFailed)?;
            if project.duration_us() == 0 || project.duration_us() > MAX_PROJECT_DURATION_MS * 1000
            {
                return Err(EngineError::ExportFailed);
            }
            let partial = partial_path();
            let result = run_export(&project, &folder, &partial, settings, task)
                .and_then(|()| move_into_place(&partial, Path::new(&path)));
            let _ = std::fs::remove_file(&partial);
            result
        });
}

/// Where an export is written until it is complete. The chosen file is only ever replaced by a
/// finished video, so leaving the app, a crash or a cancel never leaves a broken one (FR-EXP-06).
fn partial_path() -> PathBuf {
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("shorted-export-{}-{n}.mp4", std::process::id()))
}

/// Renames when both are on one disk; otherwise copies, then the caller removes the original.
fn move_into_place(partial: &Path, output: &Path) -> Result<(), EngineError> {
    if std::fs::rename(partial, output).is_ok() {
        return Ok(());
    }
    std::fs::copy(partial, output)
        .map(drop)
        .map_err(|_| EngineError::ExportFailed)
}

/// Size of a cover picture: the 1080p frame.
const COVER_WIDTH: u32 = 1080;

/// Saves the frame at `time_us` as a JPEG cover picture (FR-EXP-05).
pub async fn write_cover(path: String, time_us: i64) -> Result<(), EngineFailure> {
    let (project, folder): (Option<Arc<Project>>, PathBuf) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    let project = project.ok_or(EngineError::ExportFailed)?;
    let time_us = time_us.clamp(0, (project.duration_us() - 1).max(0));
    let jpeg = cover_jpeg(&project, &folder, time_us)?;
    std::fs::write(path, jpeg)
        .map_err(shorted_store::session::StoreError::from)
        .map_err(EngineError::from)?;
    Ok(())
}

fn cover_jpeg(project: &Project, folder: &Path, time_us: i64) -> Result<Vec<u8>, EngineError> {
    use shorted_render::compositor::Compositor;
    use shorted_render::pictures::jpeg_from_bgra;

    use crate::frame_composer::FrameComposer;

    let (width, height) = output_size(COVER_WIDTH);
    let compositor = Compositor::new(width, height).map_err(|_| EngineError::ExportFailed)?;
    let mut composer = FrameComposer::new(compositor);
    let target = composer.compositor.offscreen_target();
    composer
        .draw(Some(project), folder, time_us, &target)
        .map_err(|_| EngineError::ExportFailed)?;
    let pixels = composer
        .compositor
        .read_pixels(&target)
        .map_err(|_| EngineError::ExportFailed)?;
    jpeg_from_bgra(&pixels, width, height).ok_or(EngineError::ExportFailed)
}

/// About how many bytes an export will take, sound included.
#[flutter_rust_bridge::frb(sync)]
pub fn export_size_estimate(settings: ExportSettings, duration_us: i64) -> i64 {
    let (width, height) = output_size(settings.width);
    let bits = f64::from(bits_per_second(width, height, settings.fps)) + AUDIO_BITS_PER_SECOND;
    (bits / 8.0 * duration_us as f64 / 1e6) as i64
}

/// Short side of a 4K export.
const UHD_WIDTH: u32 = 2160;

/// Whether this device's encoder takes 2160 × 3840 (T-1207): asked once, by opening one.
#[flutter_rust_bridge::frb(sync)]
pub fn export_supports_4k() -> bool {
    use std::sync::OnceLock;
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(probe_4k)
}

#[cfg(target_vendor = "apple")]
fn probe_4k() -> bool {
    use shorted_media::apple_writer::AppleEncoder;

    let (width, height) = output_size(UHD_WIDTH);
    let file = std::env::temp_dir().join(format!("shorted-4k-{}.mp4", std::process::id()));
    let opened = AppleEncoder::new(&file, width, height, bits_per_second(width, height, 30));
    let supported = match opened {
        Ok(encoder) => {
            encoder.cancel();
            true
        }
        Err(_) => false,
    };
    let _ = std::fs::remove_file(&file);
    supported
}

#[cfg(not(target_vendor = "apple"))]
fn probe_4k() -> bool {
    false
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
    fn sound_keeps_pace_with_frames_and_ends_with_the_project() {
        let duration_us = 1_234_567;
        let (frames, audio) = (frame_count(duration_us, 30), us_to_frames(duration_us));
        assert_eq!(audio_target(0, frames, 30, audio), 1_600);
        assert_eq!(audio_target(frames - 1, frames, 30, audio), audio);
    }

    #[test]
    fn sound_chunks_stop_at_the_limit() {
        assert_eq!(audio_chunk(0, 5_000), 0..1_024);
        assert_eq!(audio_chunk(4_096, 5_000), 4_096..5_000);
    }

    /// A 4K export comes out at full resolution (T-1207).
    #[cfg(target_vendor = "apple")]
    #[test]
    fn a_4k_export_has_the_full_resolution() {
        use shorted_core::model::{MediaKind, MediaLocation};
        use shorted_media::MediaBackend;

        let clip = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/media/h264_audio_440hz.mov"
        );
        if !Path::new(clip).exists() || !export_supports_4k() {
            return;
        }
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.transitions.clear();
        project.timeline.clips.truncate(1);
        project.timeline.audio.clear();
        project.timeline.overlays.clear();
        project.captions.sources.clear();
        let id = project.timeline.clips[0].media_id;
        let media = project.media.iter_mut().find(|m| m.id == id).unwrap();
        media.kind = MediaKind::Video;
        media.location = MediaLocation::External {
            abs_path: clip.into(),
            bookmark: None,
        };
        let entry = &mut project.timeline.clips[0];
        entry.speed = 1.0;
        entry.source_in_us = 0;
        entry.source_out_us = 500_000;
        let output =
            std::env::temp_dir().join(format!("shorted-4k-out-{}.mp4", std::process::id()));
        let settings = ExportSettings {
            width: UHD_WIDTH,
            fps: 30,
        };
        crate::runtime::with_test_context(|task| {
            run_export(&project, Path::new("/"), &output, settings, task)
        })
        .unwrap();
        let info = shorted_media::apple::AppleBackend.probe(&output).unwrap();
        std::fs::remove_file(&output).unwrap();
        assert_eq!((info.width, info.height), (2160, 3840));
    }

    /// Real clip, real mix, real encoder. The fixture comes from scripts/make_fixtures.swift.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn an_exported_clip_keeps_its_sound_and_its_volume() {
        use shorted_core::model::{MediaKind, MediaLocation};
        use shorted_media::apple::decode_audio;

        let clip = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/media/h264_audio_440hz.mov"
        );
        if !Path::new(clip).exists() {
            return;
        }
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.transitions.clear();
        project.timeline.clips.truncate(1);
        project.timeline.audio.clear();
        project.timeline.overlays.clear();
        project.captions.sources.clear();
        let id = project.timeline.clips[0].media_id;
        let media = project.media.iter_mut().find(|m| m.id == id).unwrap();
        media.kind = MediaKind::Video;
        media.probe.has_audio = true;
        media.location = MediaLocation::External {
            abs_path: clip.into(),
            bookmark: None,
        };
        let entry = &mut project.timeline.clips[0];
        entry.speed = 1.0;
        entry.source_in_us = 0;
        entry.source_out_us = 2_000_000;
        entry.original_audio.volume = 0.5;
        entry.original_audio.muted = false;
        entry.original_audio.fade_in_us = 0;
        entry.original_audio.fade_out_us = 0;

        let output =
            std::env::temp_dir().join(format!("shorted-export-{}.mp4", std::process::id()));
        let settings = ExportSettings {
            width: 720,
            fps: 30,
        };
        crate::runtime::with_test_context(|task| {
            run_export(&project, Path::new("/"), &output, settings, task)
        })
        .unwrap();

        let exported = decode_audio(&output).unwrap();
        std::fs::remove_file(&output).unwrap();
        let source = decode_audio(Path::new(clip)).unwrap();
        let rms = |samples: &[f32]| {
            let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
            (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt()
        };
        assert!(
            exported.len().abs_diff(192_000) < 8_192,
            "{}",
            exported.len()
        );
        let ratio = rms(&exported) / rms(&source);
        assert!((ratio - 0.5).abs() < 0.05, "{ratio}");
    }

    #[test]
    fn size_estimates_follow_bitrate_and_duration() {
        let settings = ExportSettings {
            width: 1080,
            fps: 30,
        };
        assert_eq!(export_size_estimate(settings, 8_000_000), 12_192_000);
        assert_eq!(export_size_estimate(settings, 0), 0);
    }

    #[test]
    fn empty_or_too_long_projects_are_blocked() {
        assert!(export_blocked(0).is_some());
        assert!(export_blocked(120_000_001).is_some());
        assert!(export_blocked(120_000_000).is_none());
    }

    #[test]
    fn a_finished_export_replaces_the_chosen_file_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("short.mp4");
        std::fs::write(&output, b"old").unwrap();
        let partial = partial_path();
        assert_ne!(partial, partial_path());
        std::fs::write(&partial, b"new video").unwrap();
        move_into_place(&partial, &output).unwrap();
        let _ = std::fs::remove_file(&partial);
        assert_eq!(std::fs::read(&output).unwrap(), b"new video");
        assert!(!partial.exists());
    }

    #[test]
    fn a_paused_export_waits_and_a_cancel_still_ends_it() {
        set_export_paused(true);
        let ended = crate::runtime::with_test_context(|task| {
            task.cancel_for_test();
            wait_while_paused(task)
        });
        set_export_paused(false);
        assert!(matches!(ended, Err(EngineError::Cancelled)));
        let ran = crate::runtime::with_test_context(wait_while_paused);
        assert!(ran.is_ok());
    }

    /// The cover is the frame the user chose, as a 1080 × 1920 JPEG.
    #[test]
    fn a_cover_is_a_full_size_jpeg() {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.overlays.clear();
        project.captions.sources.clear();
        let Ok(jpeg) = cover_jpeg(&project, Path::new("/"), 0) else {
            // No GPU on this machine.
            return;
        };
        let picture = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((picture.width(), picture.height()), (1080, 1920));
    }
}
