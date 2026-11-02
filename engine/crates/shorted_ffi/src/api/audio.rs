//! The Audio section: original sound of clips, audio items and audio tracks.

// Peaks only serve the Apple decoder until other platforms have one.
#![cfg_attr(not(target_vendor = "apple"), allow(dead_code))]

use std::path::Path;

use shorted_audio::mixer::{CHANNELS, SAMPLE_RATE};
use shorted_core::command::{Command, TrackKind};
use shorted_core::model::*;
use shorted_core::validate::MAX_AUDIO_TRACKS;
use uuid::Uuid;

use crate::api::clips::{edit, parse};
use crate::api::media::resolve;
use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView, TrackKindView};
use crate::error::{EngineError, EngineFailure};
use crate::transport::transport;

/// Peaks per second of sound: fine enough for the closest timeline zoom.
const PEAKS_PER_SECOND: usize = 100;
const MIN_ITEM_US: i64 = 100_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AudioRoleView {
    VoiceOver,
    ImportedVoice,
    Music,
    SoundEffect,
}

impl From<AudioRole> for AudioRoleView {
    fn from(role: AudioRole) -> Self {
        match role {
            AudioRole::VoiceOver => Self::VoiceOver,
            AudioRole::ImportedVoice => Self::ImportedVoice,
            AudioRole::Music => Self::Music,
            AudioRole::SoundEffect => Self::SoundEffect,
        }
    }
}

impl From<AudioRoleView> for AudioRole {
    fn from(role: AudioRoleView) -> Self {
        match role {
            AudioRoleView::VoiceOver => Self::VoiceOver,
            AudioRoleView::ImportedVoice => Self::ImportedVoice,
            AudioRoleView::Music => Self::Music,
            AudioRoleView::SoundEffect => Self::SoundEffect,
        }
    }
}

pub struct ClipSoundView {
    pub clip_id: String,
    pub volume: f32,
    pub muted: bool,
    pub fade_in_us: i64,
    pub fade_out_us: i64,
}

pub struct AudioItemView {
    pub id: String,
    pub media_id: String,
    pub role: AudioRoleView,
    pub volume: f32,
    pub fade_in_us: i64,
    pub fade_out_us: i64,
    pub looped: bool,
}

pub struct AudioTrackView {
    /// None for the Voice-over track.
    pub id: Option<String>,
    pub name: Option<String>,
    pub muted: bool,
    pub items: Vec<AudioItemView>,
}

pub struct SoundView {
    pub clips: Vec<ClipSoundView>,
    pub tracks: Vec<AudioTrackView>,
}

fn item_views(project: &Project, track: AudioTrackRef) -> Vec<AudioItemView> {
    let mut items: Vec<&AudioItem> = project
        .timeline
        .audio
        .iter()
        .filter(|item| item.track == track)
        .collect();
    items.sort_by_key(|item| item.start_us);
    items
        .into_iter()
        .map(|item| AudioItemView {
            id: item.id.to_string(),
            media_id: item.media_id.to_string(),
            role: item.role.into(),
            volume: item.volume,
            fade_in_us: item.fade_in_us,
            fade_out_us: item.fade_out_us,
            looped: item.loop_,
        })
        .collect()
}

pub(crate) fn sound_view(project: &Project) -> SoundView {
    let timeline = &project.timeline;
    let mut tracks = vec![AudioTrackView {
        id: None,
        name: None,
        muted: timeline.track_flags.voice_over.muted,
        items: item_views(project, AudioTrackRef::VoiceOver),
    }];
    tracks.extend(timeline.audio_tracks.iter().map(|track| AudioTrackView {
        id: Some(track.id.to_string()),
        name: Some(track.name.clone()),
        muted: track.flags.muted,
        items: item_views(project, AudioTrackRef::User(track.id)),
    }));
    SoundView {
        clips: timeline
            .clips
            .iter()
            .map(|clip| ClipSoundView {
                clip_id: clip.id.to_string(),
                volume: clip.original_audio.volume,
                muted: clip.original_audio.muted,
                fade_in_us: clip.original_audio.fade_in_us,
                fade_out_us: clip.original_audio.fade_out_us,
            })
            .collect(),
        tracks,
    }
}

/// The original sound of a clip (FR-AUD-02).
pub async fn set_clip_sound(
    id: String,
    volume: f32,
    muted: bool,
    fade_in_us: i64,
    fade_out_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let clip = project.timeline.clips.iter().find(|clip| clip.id == id);
        let clip = clip.ok_or_else(|| EngineError::Internal("unknown clip".into()))?;
        Ok(Command::SetClipAudio {
            id,
            audio: ClipAudio {
                volume,
                muted,
                fade_in_us,
                fade_out_us,
                ..clip.original_audio
            },
        })
    })
}

fn new_track(number: usize) -> AudioTrack {
    let flags = TrackFlags {
        muted: false,
        locked: false,
        hidden: false,
    };
    AudioTrack {
        id: Uuid::now_v7(),
        name: format!("Audio {number}"),
        flags,
    }
}

fn audio_item(media: &MediaRef, role: AudioRole, start_us: i64, track: Id) -> AudioItem {
    AudioItem {
        id: Uuid::now_v7(),
        media_id: media.id,
        role,
        track: AudioTrackRef::User(track),
        start_us,
        source_in_us: 0,
        source_out_us: media.probe.duration_us,
        volume: 1.0,
        fade_in_us: 0,
        fade_out_us: 0,
        loop_: false,
        ducking: None,
        transcript: None,
    }
}

/// Places an imported sound on the first audio track, or on a new one.
pub async fn add_audio(
    media_id: String,
    role: AudioRoleView,
    start_us: i64,
) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        with_active(|session| {
            let project = session.project();
            let media = project.media.iter().find(|m| m.id.to_string() == media_id);
            let Some(media) = media.cloned() else {
                return Ok(view_of(session));
            };
            let first = project.timeline.audio_tracks.first().map(|track| track.id);
            let mut commands = Vec::new();
            let track = first.unwrap_or_else(|| {
                let track = new_track(1);
                let id = track.id;
                commands.push(Command::AddAudioTrack { track });
                id
            });
            let item = audio_item(&media, role.into(), start_us.max(0), track);
            commands.push(Command::AddAudio {
                item: Box::new(item),
            });
            session.execute(&Command::Group {
                label: "add_audio".into(),
                commands,
            })?;
            Ok(view_of(session))
        })
    })
}

pub async fn set_audio_sound(
    id: String,
    volume: f32,
    fade_in_us: i64,
    fade_out_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        let id = parse(&id)?;
        Ok(Command::Group {
            label: "set_audio_sound".into(),
            commands: vec![
                Command::SetVolume { id, volume },
                Command::SetFades {
                    id,
                    fade_in_us,
                    fade_out_us,
                },
            ],
        })
    })
}

pub async fn set_audio_loop(id: String, looped: bool) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::SetAudioLoop {
            id: parse(&id)?,
            looped,
        })
    })
}

pub async fn set_audio_role(id: String, role: AudioRoleView) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::SetAudioRole {
            id: parse(&id)?,
            role: role.into(),
        })
    })
}

pub async fn delete_audio(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |_| Ok(Command::DeleteAudio { id: parse(&id)? }))
}

/// Moves an item along its track, or to another one.
pub async fn move_audio(
    id: String,
    start_us: i64,
    track_id: Option<String>,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::MoveAudio {
            id: parse(&id)?,
            start_us: start_us.max(0),
            track: match track_id {
                Some(track) => AudioTrackRef::User(parse(&track)?),
                None => AudioTrackRef::VoiceOver,
            },
        })
    })
}

/// The commands for dragging one edge of an item by `delta_us`, within its file. Dragging the
/// start keeps the sound where it was: the item shortens or grows from the left.
fn trim_commands(item: &AudioItem, media_us: i64, start: bool, delta_us: i64) -> Vec<Command> {
    let id = item.id;
    if start {
        let delta = delta_us
            .max(-item.source_in_us)
            .max(-item.start_us)
            .min(item.source_out_us - item.source_in_us - MIN_ITEM_US);
        return vec![
            Command::TrimAudio {
                id,
                source_in_us: item.source_in_us + delta,
                source_out_us: item.source_out_us,
            },
            Command::MoveAudio {
                id,
                start_us: item.start_us + delta,
                track: item.track,
            },
        ];
    }
    let shortest = item.source_in_us + MIN_ITEM_US;
    vec![Command::TrimAudio {
        id,
        source_in_us: item.source_in_us,
        source_out_us: (item.source_out_us + delta_us).clamp(shortest, media_us),
    }]
}

/// Drags one edge of an item on the timeline, as one undo step.
pub async fn trim_audio(
    id: String,
    start: bool,
    delta_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let item = project.timeline.audio.iter().find(|item| item.id == id);
        let item = item.ok_or_else(|| EngineError::Internal("unknown sound".into()))?;
        let media = project.media.iter().find(|m| m.id == item.media_id);
        let media_us = media.map_or(item.source_out_us, |m| m.probe.duration_us);
        Ok(Command::Group {
            label: "trim_audio".into(),
            commands: trim_commands(item, media_us, start, delta_us),
        })
    })
}

/// Moves a user track up or down the list.
pub async fn reorder_audio_track(id: String, to: u32) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::ReorderAudioTrack {
            id: parse(&id)?,
            to: to as usize,
        })
    })
}

pub async fn add_audio_track() -> Result<ProjectView, EngineFailure> {
    edit(|project| {
        let count = project.timeline.audio_tracks.len();
        if count >= MAX_AUDIO_TRACKS {
            return Err(EngineError::Internal("too many audio tracks".into()));
        }
        Ok(Command::AddAudioTrack {
            track: new_track(count + 1),
        })
    })
}

pub async fn remove_audio_track(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |_| Ok(Command::RemoveAudioTrack { id: parse(&id)? }))
}

pub async fn rename_audio_track(id: String, name: String) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::RenameAudioTrack {
            id: parse(&id)?,
            name,
        })
    })
}

/// Mute, lock and hide for any track: a user audio track when `id` is set, else `kind`.
pub async fn set_track_flags(
    kind: TrackKindView,
    id: Option<String>,
    muted: bool,
    locked: bool,
    hidden: bool,
) -> Result<ProjectView, EngineFailure> {
    let flags = TrackFlags {
        muted,
        locked,
        hidden,
    };
    edit(move |_| match id {
        Some(id) => Ok(Command::SetAudioTrackFlags {
            id: parse(&id)?,
            flags,
        }),
        None => Ok(Command::SetTrackFlags {
            track: match kind {
                TrackKindView::Widgets => TrackKind::Widgets,
                TrackKindView::Video => TrackKind::Video,
                TrackKindView::Captions => TrackKind::Captions,
                _ => TrackKind::VoiceOver,
            },
            flags,
        }),
    })
}

/// Starts recording the microphone into the project (FR-AUD-03).
#[cfg(target_vendor = "apple")]
pub async fn start_voice_over() -> Result<(), EngineFailure> {
    let folder = transport().folder.clone();
    let media = folder.join("media");
    std::fs::create_dir_all(&media)
        .map_err(shorted_store::session::StoreError::from)
        .map_err(EngineError::from)?;
    crate::recorder::start(media.join(format!("voice-{}.wav", Uuid::now_v7())))?;
    Ok(())
}

/// Stops recording and places the take on the Voice-over track at `start_us`.
#[cfg(target_vendor = "apple")]
pub async fn stop_voice_over(start_us: i64) -> Result<ProjectView, EngineFailure> {
    let take = crate::recorder::stop();
    on_command_thread(move || {
        with_active(|session| {
            let Some(path) = take else {
                return Ok(view_of(session));
            };
            let folder = crate::api::projects::active_folder(session.project().id)
                .ok_or_else(|| shorted_store::session::StoreError::from(no_folder()))?;
            let media = crate::api::media::make_media(&path, &folder).map_err(|error| {
                tracing::error!(%error, "voice-over not imported");
                shorted_store::session::StoreError::from(no_folder())
            })?;
            let mut item = audio_item(&media, AudioRole::VoiceOver, start_us.max(0), Uuid::nil());
            item.track = AudioTrackRef::VoiceOver;
            session.execute(&Command::Group {
                label: "voice_over".into(),
                commands: vec![
                    Command::ImportMedia { media: vec![media] },
                    Command::AddAudio {
                        item: Box::new(item),
                    },
                ],
            })?;
            Ok(view_of(session))
        })
    })
}

#[cfg(target_vendor = "apple")]
fn no_folder() -> std::io::Error {
    std::io::Error::other("voice-over not saved")
}

/// The microphone's level while recording, from 0 to 1.
#[cfg(target_vendor = "apple")]
#[flutter_rust_bridge::frb(sync)]
pub fn voice_level() -> f32 {
    crate::recorder::level()
}

#[cfg(target_vendor = "apple")]
#[flutter_rust_bridge::frb(sync)]
pub fn pause_voice_over(paused: bool) {
    crate::recorder::set_paused(paused);
}

/// No microphone capture on this platform yet.
#[cfg(not(target_vendor = "apple"))]
pub async fn start_voice_over() -> Result<(), EngineFailure> {
    Err(EngineError::Internal("no microphone capture here".into()).into())
}

#[cfg(not(target_vendor = "apple"))]
pub async fn stop_voice_over(_start_us: i64) -> Result<ProjectView, EngineFailure> {
    Err(EngineError::Internal("no microphone capture here".into()).into())
}

#[cfg(not(target_vendor = "apple"))]
#[flutter_rust_bridge::frb(sync)]
pub fn voice_level() -> f32 {
    0.0
}

#[cfg(not(target_vendor = "apple"))]
#[flutter_rust_bridge::frb(sync)]
pub fn pause_voice_over(_paused: bool) {}

/// Loudest absolute sample of each span, scaled to a byte.
fn peaks(samples: &[f32]) -> Vec<u8> {
    let span = (SAMPLE_RATE as usize / PEAKS_PER_SECOND * CHANNELS).max(1);
    samples
        .chunks(span)
        .map(|chunk| {
            let loudest = chunk.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
            (loudest.clamp(0.0, 1.0) * 255.0) as u8
        })
        .collect()
}

#[cfg(target_vendor = "apple")]
fn decode_peaks(path: &Path) -> Vec<u8> {
    shorted_media::apple::decode_audio(path).map_or_else(|_| Vec::new(), |s| peaks(&s))
}

/// No decoder on this platform yet (T-0401).
#[cfg(not(target_vendor = "apple"))]
fn decode_peaks(_path: &Path) -> Vec<u8> {
    Vec::new()
}

/// The waveform of a media, 100 values per second, from the cache when possible.
pub async fn media_peaks(media_id: String) -> Vec<u8> {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    let media = project
        .as_ref()
        .and_then(|project| project.media.iter().find(|m| m.id.to_string() == media_id));
    let Some(media) = media.filter(|media| media.probe.has_audio) else {
        return Vec::new();
    };
    let hash = &media.content_hash;
    // The hash comes from the project file, so it must not be able to name a path.
    let cache = (!hash.is_empty() && hash.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| folder.join("cache").join(format!("peaks-{hash}.bin")));
    if let Some(bytes) = cache.as_ref().and_then(|file| std::fs::read(file).ok()) {
        return bytes;
    }
    let Some(path) = resolve(media, &folder) else {
        return Vec::new();
    };
    let values = decode_peaks(&path);
    if let Some(file) = cache.filter(|_| !values.is_empty()) {
        // Losing the cache only costs decoding again.
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, &values));
        if let Err(error) = written {
            tracing::warn!(%error, "waveform not cached");
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_keep_the_loudest_sample_of_each_span() {
        let mut samples = vec![0.0; 960];
        samples[3] = -1.0;
        samples.extend(vec![0.5; 960]);
        assert_eq!(peaks(&samples), [255, 127]);
    }

    /// The fixture comes from scripts/make_fixtures.swift.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn a_real_file_gives_its_length_and_a_waveform() {
        let root = env!("CARGO_MANIFEST_DIR");
        let clip = Path::new(root).join("../../fixtures/media/h264_audio_440hz.mov");
        if !clip.exists() {
            return;
        }
        let duration = shorted_media::apple::audio_duration_us(&clip).unwrap();
        assert!(duration > 1_900_000 && duration < 2_200_000, "{duration}");
        let values = decode_peaks(&clip);
        assert!(values.len().abs_diff(200) < 10, "{}", values.len());
        assert!(values[100] > 10, "{}", values[100]);
    }

    fn placed_item() -> AudioItem {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let media = Project::from_json(json).unwrap().media.remove(0);
        let mut item = audio_item(&media, AudioRole::Music, 2_000_000, Uuid::nil());
        item.source_in_us = 1_000_000;
        item.source_out_us = 3_000_000;
        item
    }

    #[test]
    fn dragging_the_start_keeps_the_sound_in_place() {
        let item = placed_item();
        let commands = trim_commands(&item, 9_000_000, true, 500_000);
        assert!(matches!(
            commands[0],
            Command::TrimAudio {
                source_in_us: 1_500_000,
                ..
            }
        ));
        assert!(matches!(
            commands[1],
            Command::MoveAudio {
                start_us: 2_500_000,
                ..
            }
        ));
    }

    #[test]
    fn trims_stop_at_the_file_and_at_a_minimum_length() {
        let item = placed_item();
        let early = trim_commands(&item, 9_000_000, true, -9_000_000);
        assert!(matches!(
            early[0],
            Command::TrimAudio {
                source_in_us: 0,
                ..
            }
        ));
        let late = trim_commands(&item, 9_000_000, false, 99_000_000);
        assert!(matches!(
            late[0],
            Command::TrimAudio {
                source_out_us: 9_000_000,
                ..
            }
        ));
        let short = trim_commands(&item, 9_000_000, false, -99_000_000);
        assert!(matches!(
            short[0],
            Command::TrimAudio {
                source_out_us: 1_100_000,
                ..
            }
        ));
    }

    #[test]
    fn roles_cross_the_bridge_both_ways() {
        for role in [
            AudioRole::VoiceOver,
            AudioRole::ImportedVoice,
            AudioRole::Music,
            AudioRole::SoundEffect,
        ] {
            assert_eq!(AudioRole::from(AudioRoleView::from(role)), role);
        }
    }

    #[test]
    fn a_new_item_plays_its_whole_file() {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let media = Project::from_json(json).unwrap().media.remove(0);
        let item = audio_item(&media, AudioRole::Music, 5, Uuid::nil());
        assert_eq!(item.start_us, 5);
        assert_eq!(item.source_out_us, media.probe.duration_us);
        assert_eq!(item.track, AudioTrackRef::User(Uuid::nil()));
    }
}
