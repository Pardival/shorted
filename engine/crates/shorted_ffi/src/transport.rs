//! Shared state of the preview: what to show and at which time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use shorted_audio::mixer::SAMPLE_RATE;
use shorted_core::model::Project;

pub(crate) struct Transport {
    pub project: Option<Arc<Project>>,
    pub folder: PathBuf,
    /// Changes with each edit, so that a paused frame is redrawn.
    pub revision: u64,
    position_us: i64,
    generation: u64,
    playing_since: Option<Instant>,
    /// Frames the sound output has played since `play`. It paces the video when present.
    pub audio_frames: Option<Arc<AtomicU64>>,
}

static TRANSPORT: Mutex<Transport> = Mutex::new(Transport {
    project: None,
    folder: PathBuf::new(),
    revision: 0,
    position_us: 0,
    generation: 0,
    playing_since: None,
    audio_frames: None,
});

pub(crate) fn transport() -> MutexGuard<'static, Transport> {
    TRANSPORT.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Transport {
    fn duration_us(&self) -> i64 {
        self.project
            .as_ref()
            .map_or(0, |project| project.duration_us())
    }

    /// Without audio yet, the clock is the system's. T-0403 moves it to the audio output.
    pub fn position_us(&mut self) -> i64 {
        let duration = self.duration_us();
        let elapsed = self
            .playing_since
            .map_or(0, |since| match &self.audio_frames {
                Some(frames) => {
                    frames.load(Ordering::Relaxed) as i64 * 1_000_000 / i64::from(SAMPLE_RATE)
                }
                None => since.elapsed().as_micros() as i64,
            });
        let position = self.position_us + elapsed;
        if position >= duration && self.playing_since.is_some() {
            self.playing_since = None;
            self.position_us = duration;
        }
        position.clamp(0, duration)
    }

    fn reset_audio_clock(&mut self) {
        if let Some(frames) = &self.audio_frames {
            frames.store(0, Ordering::Relaxed);
        }
        self.generation += 1;
    }

    /// Where playback started, and a number that changes on every jump.
    pub fn playback_start(&self) -> (i64, u64) {
        (self.position_us, self.generation)
    }

    pub fn play(&mut self) {
        if self.playing() {
            return;
        }
        // Playing from the very end starts over.
        if self.position_us() >= self.duration_us() {
            self.position_us = 0;
        }
        self.reset_audio_clock();
        self.playing_since = Some(Instant::now());
    }

    pub fn pause(&mut self) {
        self.position_us = self.position_us();
        self.playing_since = None;
    }

    pub fn playing(&self) -> bool {
        self.playing_since.is_some()
    }

    pub fn set_position(&mut self, time_us: i64) {
        self.position_us = time_us.clamp(0, self.duration_us());
        if self.playing_since.is_some() {
            self.playing_since = Some(Instant::now());
            self.reset_audio_clock();
        }
    }
}

/// Hands the latest project to the preview. A shared copy: rendering never touches the editor's.
pub(crate) fn publish(project: &Project, folder: Option<PathBuf>) {
    let mut transport = transport();
    let unchanged = transport.project.as_deref() == Some(project);
    if unchanged {
        return;
    }
    transport.project = Some(Arc::new(project.clone()));
    transport.folder = folder.unwrap_or_default();
    transport.revision += 1;
    let position = transport.position_us();
    transport.set_position(position);
}

pub(crate) fn clear() {
    let mut transport = transport();
    transport.project = None;
    transport.position_us = 0;
    transport.playing_since = None;
    transport.revision += 1;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_audio_clock_paces_playback_when_there_is_one() {
        let project = super::Arc::new(project());
        let frames = super::Arc::new(super::AtomicU64::new(0));
        let mut state = Transport {
            project: Some(project),
            folder: PathBuf::new(),
            revision: 0,
            position_us: 0,
            generation: 0,
            playing_since: None,
            audio_frames: Some(frames.clone()),
        };
        state.play();
        frames.store(24_000, super::Ordering::Relaxed);
        assert_eq!(state.position_us(), 500_000);
        state.set_position(0);
        assert_eq!(state.position_us(), 0);
        assert_eq!(frames.load(super::Ordering::Relaxed), 0);
    }

    use shorted_core::model::Project;
    use uuid::Uuid;

    use super::*;

    fn project() -> Project {
        let json = include_str!("../../../fixtures/projects/v1_full.json");
        Project::from_json(json).expect("fixture")
    }

    #[test]
    fn transport_clamps_plays_and_stops_at_the_end() {
        let project = project();
        let duration = project.duration_us();
        let mut state = Transport {
            project: Some(Arc::new(project)),
            folder: PathBuf::new(),
            revision: 0,
            position_us: 0,
            generation: 0,
            playing_since: None,
            audio_frames: None,
        };
        state.set_position(-5);
        assert_eq!(state.position_us(), 0);
        state.set_position(duration + 5);
        assert_eq!(state.position_us(), duration);

        state.set_position(duration - 1);
        state.playing_since = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert_eq!(state.position_us(), duration);
        assert!(!state.playing());
    }

    #[test]
    fn empty_transport_stays_at_zero() {
        let mut state = Transport {
            project: Some(Arc::new(Project::new(
                Uuid::nil(),
                "x".into(),
                "now".into(),
            ))),
            folder: PathBuf::new(),
            revision: 0,
            position_us: 0,
            generation: 0,
            playing_since: None,
            audio_frames: None,
        };
        state.set_position(1_000);
        assert_eq!(state.position_us(), 0);
    }
}
