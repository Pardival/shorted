//! Local derush (T-0801): each clip's sound and picture are analysed, cached by content, and
//! turned into a local edit plan. Kept out of `api` so its types stay out of the bridge.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use shorted_ai::plan::{validate, Aliases, ValidPlan};
use shorted_analysis::frames::{FrameConfig, FrameMetrics};
use shorted_analysis::local::{add_duplicate_takes, analyse_clip, local_plan, ClipAnalysis};
use shorted_analysis::speech::{Range, SpeechConfig};
use shorted_core::model::{Id, MediaKind, MediaRef, Project};
use uuid::Uuid;

use crate::api::media::resolve;
use crate::error::EngineError;
use crate::runtime::TaskContext;

/// Frames sampled per second for the picture signals (docs/07 §2).
const FRAMES_PER_SECOND: i64 = 5;
const FRAME_STEP_US: i64 = 1_000_000 / FRAMES_PER_SECOND;
/// Small frames are enough for blur, exposure and motion, and keep the analysis quick.
const FRAME_SIZE: (usize, usize) = (72, 128);
/// Long sources are analysed for this long at most.
const MAX_ANALYSED_US: i64 = 10 * 60 * 1_000_000;
/// Bump when the signals change, so old caches are not reused.
const CACHE_VERSION: u32 = 1;

/// What the analysis keeps of one media file.
#[derive(Default, Serialize, Deserialize)]
struct Signals {
    version: u32,
    /// `None` without a sound track.
    speech: Option<Vec<Range>>,
    frames: Vec<FrameMetrics>,
}

/// The plan waiting for review.
static PENDING: Mutex<Option<ValidPlan>> = Mutex::new(None);

pub(crate) fn pending() -> MutexGuard<'static, Option<ValidPlan>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The hash comes from the project file, so it must not be able to name a path.
fn cache_file(media: &MediaRef, folder: &Path) -> Option<PathBuf> {
    let hash = &media.content_hash;
    (!hash.is_empty() && hash.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| folder.join("cache").join(format!("derush-{hash}.json")))
}

fn cached(file: &Path) -> Option<Signals> {
    let signals: Signals = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    (signals.version == CACHE_VERSION).then_some(signals)
}

fn store(file: &Path, signals: &Signals) {
    let written = serde_json::to_vec(signals)
        .map_err(std::io::Error::other)
        .and_then(|bytes| {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, bytes)
        });
    if let Err(error) = written {
        // The analysis is only redone next time.
        tracing::warn!(%error, "derush cache not written");
    }
}

#[cfg(target_vendor = "apple")]
fn measure_media(media: &MediaRef, path: &Path) -> Signals {
    use shorted_analysis::frames::measure;
    use shorted_analysis::local::Sound;

    let speech = media.probe.has_audio.then(|| {
        let stereo = shorted_media::apple::decode_audio(path).unwrap_or_default();
        let mono: Vec<f32> = stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (pair[0] + pair[1]) / 2.0)
            .collect();
        Sound {
            mono: &mono,
            rate: 48_000,
        }
        .speech(&SpeechConfig::default())
    });
    let mut frames = Vec::new();
    if media.kind == MediaKind::Video {
        let length = media.probe.duration_us.min(MAX_ANALYSED_US);
        let times = (0..length).step_by(FRAME_STEP_US as usize);
        let mut previous: Option<Vec<f32>> = None;
        let sampled = shorted_media::apple::filmstrip(
            path,
            times,
            media.probe.rotation,
            FRAME_SIZE,
            |time_us, rgba| {
                let (metrics, light) = measure(
                    time_us,
                    &rgba,
                    FRAME_SIZE.0,
                    FRAME_SIZE.1,
                    previous.as_deref(),
                );
                frames.push(metrics);
                previous = Some(light);
                true
            },
        );
        if let Err(error) = sampled {
            tracing::warn!(%error, "frames not analysed");
        }
    }
    Signals {
        version: CACHE_VERSION,
        speech,
        frames,
    }
}

/// No decoder on this platform yet (T-0401): transcripts still give speech.
#[cfg(not(target_vendor = "apple"))]
fn measure_media(_media: &MediaRef, _path: &Path) -> Signals {
    Signals {
        version: CACHE_VERSION,
        speech: None,
        frames: Vec::new(),
    }
}

fn signals_of(media: &MediaRef, folder: &Path) -> Signals {
    let file = cache_file(media, folder);
    if let Some(found) = file.as_deref().and_then(cached) {
        return found;
    }
    let Some(path) = resolve(media, folder) else {
        return Signals::default();
    };
    let signals = measure_media(media, &path);
    if let Some(file) = file {
        store(&file, &signals);
    }
    signals
}

/// Analyses every clip and builds the local plan, cancellable between clips.
pub(crate) fn plan_locally(
    project: &Project,
    folder: &Path,
    task: Option<&TaskContext<'_>>,
) -> Result<ValidPlan, EngineError> {
    let clips = &project.timeline.clips;
    let mut analyses: Vec<ClipAnalysis> = Vec::with_capacity(clips.len());
    for (index, clip) in clips.iter().enumerate() {
        if let Some(task) = task {
            task.check_cancelled()?;
            task.progress(index as f64 / clips.len().max(1) as f64);
        }
        let Some(media) = project.media.iter().find(|m| m.id == clip.media_id) else {
            continue;
        };
        let signals = signals_of(media, folder);
        analyses.push(analyse_clip(
            clip,
            signals.speech.as_deref(),
            &signals.frames,
            FRAME_STEP_US,
            &SpeechConfig::default(),
            &FrameConfig::default(),
        ));
    }
    add_duplicate_takes(project, &mut analyses);
    let aliases = Aliases::of(project);
    let plan = local_plan(project, &analyses, &aliases);
    validate(&plan, project, &aliases, &mut || -> Id { Uuid::now_v7() }).map_err(|errors| {
        // A local plan that does not validate is a bug, never the user's doing.
        EngineError::Internal(format!("local plan refused: {errors:?}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shorted_core::model::{ColorRange, MediaLocation, MediaProbe};

    fn media(hash: &str) -> MediaRef {
        MediaRef {
            id: Uuid::nil(),
            kind: MediaKind::Video,
            location: MediaLocation::ProjectCopy {
                rel_path: "media/x.mov".into(),
            },
            content_hash: hash.into(),
            probe: MediaProbe {
                duration_us: 1,
                width: 1,
                height: 1,
                rotation: 0,
                fps: 30.0,
                has_audio: true,
                codec: String::new(),
                color: ColorRange::Sdr,
            },
            captured_at: None,
        }
    }

    #[test]
    fn signals_are_cached_by_content_and_a_hash_cannot_name_a_path() {
        let folder = tempfile::tempdir().unwrap();
        assert!(cache_file(&media("../../etc"), folder.path()).is_none());
        let file = cache_file(&media("abc123"), folder.path()).unwrap();
        let signals = Signals {
            version: CACHE_VERSION,
            speech: Some(vec![(0, 1_000_000)]),
            frames: Vec::new(),
        };
        store(&file, &signals);
        let back = cached(&file).unwrap();
        assert_eq!(back.speech, Some(vec![(0, 1_000_000)]));
        let stale = Signals {
            version: CACHE_VERSION + 1,
            ..signals
        };
        store(&file, &stale);
        assert!(cached(&file).is_none());
    }
}
