//! Edits (docs/06 §4). Applying a command validates the result and returns its inverse, itself a
//! command, so undo, redo, the journal and tests share one path.

use serde::{Deserialize, Serialize};

use crate::error::ModelError;
use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum TranscriptOwner {
    Clip(Id),
    Audio(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimSlot {
    In,
    Loop,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Widgets,
    Captions,
    VoiceOver,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    // Project
    Rename {
        title: String,
    },
    SetLanguage {
        language: Option<String>,
    },
    SetCanvasFps {
        fps: u32,
    },
    // Media
    ImportMedia {
        media: Vec<MediaRef>,
    },
    RemoveMedia {
        id: Id,
    },
    RelinkMedia {
        id: Id,
        location: MediaLocation,
    },
    // Clips
    InsertClips {
        at: usize,
        clips: Vec<ClipItem>,
    },
    MoveClip {
        id: Id,
        to: usize,
    },
    TrimClip {
        id: Id,
        source_in_us: i64,
        source_out_us: i64,
    },
    /// `new_id` names the right part: this crate generates no ids.
    SplitClip {
        id: Id,
        at_source_us: i64,
        new_id: Id,
    },
    DeleteClip {
        id: Id,
    },
    SetSpeed {
        id: Id,
        speed: f32,
    },
    SetCrop {
        id: Id,
        crop: Crop,
    },
    SetColor {
        id: Id,
        color: ColorAdjust,
    },
    // Transitions
    SetTransition {
        left: Id,
        right: Id,
        kind: TransitionKind,
        duration_us: i64,
    },
    SetTransitionForAll {
        kind: TransitionKind,
        duration_us: i64,
    },
    // Audio
    AddAudio {
        item: Box<AudioItem>,
    },
    DeleteAudio {
        id: Id,
    },
    MoveAudio {
        id: Id,
        start_us: i64,
        track: AudioTrackRef,
    },
    TrimAudio {
        id: Id,
        source_in_us: i64,
        source_out_us: i64,
    },
    SetVolume {
        id: Id,
        volume: f32,
    },
    SetFades {
        id: Id,
        fade_in_us: i64,
        fade_out_us: i64,
    },
    SetDucking {
        id: Id,
        ducking: Option<Ducking>,
    },
    SetClipAudio {
        id: Id,
        audio: ClipAudio,
    },
    // Tracks
    AddAudioTrack {
        track: AudioTrack,
    },
    RemoveAudioTrack {
        id: Id,
    },
    RenameAudioTrack {
        id: Id,
        name: String,
    },
    ReorderAudioTrack {
        id: Id,
        to: usize,
    },
    SetTrackFlags {
        track: TrackKind,
        flags: TrackFlags,
    },
    SetAudioTrackFlags {
        id: Id,
        flags: TrackFlags,
    },
    // Transcript
    SetTranscript {
        owner: TranscriptOwner,
        transcript: Option<Transcript>,
    },
    EditWord {
        owner: TranscriptOwner,
        segment: Id,
        word: usize,
        text: String,
    },
    HideSegment {
        owner: TranscriptOwner,
        segment: Id,
        hidden: bool,
    },
    DeleteWords {
        owner: TranscriptOwner,
        segment: Id,
        words: Vec<usize>,
        deleted: bool,
    },
    ToggleEmphasis {
        owner: TranscriptOwner,
        segment: Id,
        word: usize,
    },
    // Captions
    SetCaptionSettings {
        settings: Box<CaptionSettings>,
    },
    // Overlays
    AddOverlay {
        overlay: Box<OverlayItem>,
    },
    UpdateOverlayTransform {
        id: Id,
        base: Transform,
    },
    SetOverlayTiming {
        id: Id,
        start_us: i64,
        end_us: i64,
    },
    SetKeyframe {
        id: Id,
        property: Prop,
        key: Key,
    },
    RemoveKeyframe {
        id: Id,
        property: Prop,
        t_us: i64,
    },
    SetMotionPath {
        id: Id,
        path: Option<MotionPath>,
    },
    SetPresetAnim {
        id: Id,
        slot: AnimSlot,
        anim: Option<PresetAnim>,
    },
    ReorderOverlay {
        id: Id,
        to: usize,
    },
    DeleteOverlay {
        id: Id,
    },
    // Grouping
    Group {
        label: String,
        commands: Vec<Command>,
    },
    // Inverses: whole values put back. Also valid journal entries.
    ReplaceClip {
        clip: Box<ClipItem>,
    },
    ReplaceAudio {
        item: Box<AudioItem>,
    },
    ReplaceOverlay {
        overlay: Box<OverlayItem>,
    },
    RestoreClips {
        clips: Vec<ClipItem>,
        transitions: Vec<Transition>,
    },
    RestoreAudio {
        audio: Vec<AudioItem>,
        captions: Box<CaptionSettings>,
    },
    RestoreTracks {
        tracks: Vec<AudioTrack>,
        audio: Vec<AudioItem>,
        captions: Box<CaptionSettings>,
    },
    RestoreMedia {
        media: Vec<MediaRef>,
    },
    RestoreOverlays {
        overlays: Vec<OverlayItem>,
    },
}

fn clip_mut(project: &mut Project, id: Id) -> Result<&mut ClipItem, ModelError> {
    project
        .timeline
        .clips
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(ModelError::UnknownId(id))
}

fn audio_mut(project: &mut Project, id: Id) -> Result<&mut AudioItem, ModelError> {
    project
        .timeline
        .audio
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or(ModelError::UnknownId(id))
}

fn overlay_mut(project: &mut Project, id: Id) -> Result<&mut OverlayItem, ModelError> {
    project
        .timeline
        .overlays
        .iter_mut()
        .find(|o| o.id == id)
        .ok_or(ModelError::UnknownId(id))
}

/// Changes one clip; the inverse puts the previous clip back.
fn edit_clip(
    project: &mut Project,
    id: Id,
    change: impl FnOnce(&mut ClipItem),
) -> Result<Command, ModelError> {
    let clip = clip_mut(project, id)?;
    let inverse = Command::ReplaceClip {
        clip: Box::new(clip.clone()),
    };
    change(clip);
    Ok(inverse)
}

fn edit_audio(
    project: &mut Project,
    id: Id,
    change: impl FnOnce(&mut AudioItem),
) -> Result<Command, ModelError> {
    let item = audio_mut(project, id)?;
    let inverse = Command::ReplaceAudio {
        item: Box::new(item.clone()),
    };
    change(item);
    Ok(inverse)
}

fn edit_overlay(
    project: &mut Project,
    id: Id,
    change: impl FnOnce(&mut OverlayItem),
) -> Result<Command, ModelError> {
    let overlay = overlay_mut(project, id)?;
    let inverse = Command::ReplaceOverlay {
        overlay: Box::new(overlay.clone()),
    };
    change(overlay);
    Ok(inverse)
}

fn restore_clips(project: &Project) -> Command {
    Command::RestoreClips {
        clips: project.timeline.clips.clone(),
        transitions: project.timeline.transitions.clone(),
    }
}

fn restore_audio(project: &Project) -> Command {
    Command::RestoreAudio {
        audio: project.timeline.audio.clone(),
        captions: Box::new(project.captions.clone()),
    }
}

/// A transition only exists between neighbours (validated): drop those a reorder separated.
fn prune_transitions(timeline: &mut Timeline) {
    let clips = &timeline.clips;
    timeline.transitions.retain(|transition| {
        clips
            .windows(2)
            .any(|pair| pair[0].id == transition.left && pair[1].id == transition.right)
    });
}

fn reposition<T>(
    items: &mut Vec<T>,
    id: Id,
    to: usize,
    id_of: impl Fn(&T) -> Id,
) -> Result<(), ModelError> {
    let from = items
        .iter()
        .position(|item| id_of(item) == id)
        .ok_or(ModelError::UnknownId(id))?;
    if to >= items.len() {
        return Err(ModelError::OutOfRange { index: to });
    }
    let item = items.remove(from);
    items.insert(to, item);
    Ok(())
}

fn word_mut(
    transcript: Option<&mut Transcript>,
    segment: Id,
    index: usize,
) -> Result<&mut Word, ModelError> {
    segment_mut(transcript, segment)?
        .words
        .get_mut(index)
        .ok_or(ModelError::OutOfRange { index })
}

fn segment_mut(transcript: Option<&mut Transcript>, id: Id) -> Result<&mut Segment, ModelError> {
    transcript
        .and_then(|t| t.segments.iter_mut().find(|s| s.id == id))
        .ok_or(ModelError::UnknownId(id))
}

/// Runs `change` on the owner's transcript. Failing leaves the working copy discarded by `apply`.
fn edit_transcript(
    project: &mut Project,
    owner: TranscriptOwner,
    change: impl FnOnce(Option<&mut Transcript>) -> Result<(), ModelError>,
) -> Result<Command, ModelError> {
    match owner {
        TranscriptOwner::Clip(id) => {
            let clip = clip_mut(project, id)?;
            let inverse = Command::ReplaceClip {
                clip: Box::new(clip.clone()),
            };
            change(clip.transcript.as_mut())?;
            Ok(inverse)
        }
        TranscriptOwner::Audio(id) => {
            let item = audio_mut(project, id)?;
            let inverse = Command::ReplaceAudio {
                item: Box::new(item.clone()),
            };
            change(item.transcript.as_mut())?;
            Ok(inverse)
        }
    }
}

fn mutate(command: &Command, project: &mut Project) -> Result<Command, ModelError> {
    use Command::*;
    Ok(match command.clone() {
        Rename { title } => Rename {
            title: std::mem::replace(&mut project.title, title),
        },
        SetLanguage { language } => SetLanguage {
            language: std::mem::replace(&mut project.language, language),
        },
        SetCanvasFps { fps } => SetCanvasFps {
            fps: std::mem::replace(&mut project.canvas.fps_hint, fps),
        },

        ImportMedia { mut media } => {
            let inverse = RestoreMedia {
                media: project.media.clone(),
            };
            project.media.append(&mut media);
            inverse
        }
        RemoveMedia { id } => {
            let inverse = RestoreMedia {
                media: project.media.clone(),
            };
            let before = project.media.len();
            project.media.retain(|m| m.id != id);
            if project.media.len() == before {
                return Err(ModelError::UnknownId(id));
            }
            inverse
        }
        RelinkMedia { id, location } => {
            let inverse = RestoreMedia {
                media: project.media.clone(),
            };
            let media = project
                .media
                .iter_mut()
                .find(|m| m.id == id)
                .ok_or(ModelError::UnknownId(id))?;
            media.location = location;
            inverse
        }

        InsertClips { at, clips } => {
            let inverse = restore_clips(project);
            if at > project.timeline.clips.len() {
                return Err(ModelError::OutOfRange { index: at });
            }
            project.timeline.clips.splice(at..at, clips);
            prune_transitions(&mut project.timeline);
            inverse
        }
        MoveClip { id, to } => {
            let inverse = restore_clips(project);
            reposition(&mut project.timeline.clips, id, to, |c| c.id)?;
            prune_transitions(&mut project.timeline);
            inverse
        }
        TrimClip {
            id,
            source_in_us,
            source_out_us,
        } => edit_clip(project, id, |clip| {
            clip.source_in_us = source_in_us;
            clip.source_out_us = source_out_us;
        })?,
        SplitClip {
            id,
            at_source_us,
            new_id,
        } => {
            let inverse = restore_clips(project);
            let left = clip_mut(project, id)?;
            let mut right = left.clone();
            right.id = new_id;
            right.source_in_us = at_source_us;
            left.source_out_us = at_source_us;
            let index = project
                .timeline
                .clips
                .iter()
                .position(|c| c.id == id)
                .unwrap_or(0);
            project.timeline.clips.insert(index + 1, right);
            for transition in &mut project.timeline.transitions {
                if transition.left == id {
                    transition.left = new_id;
                }
            }
            inverse
        }
        DeleteClip { id } => {
            let inverse = restore_clips(project);
            clip_mut(project, id)?;
            project.timeline.clips.retain(|c| c.id != id);
            prune_transitions(&mut project.timeline);
            inverse
        }
        SetSpeed { id, speed } => edit_clip(project, id, |clip| clip.speed = speed)?,
        SetCrop { id, crop } => edit_clip(project, id, |clip| clip.crop = crop)?,
        SetColor { id, color } => edit_clip(project, id, |clip| clip.color = color)?,

        SetTransition {
            left,
            right,
            kind,
            duration_us,
        } => {
            let inverse = restore_clips(project);
            let transitions = &mut project.timeline.transitions;
            transitions.retain(|t| !(t.left == left && t.right == right));
            if kind != TransitionKind::Cut {
                transitions.push(Transition {
                    left,
                    right,
                    kind,
                    duration_us,
                });
            }
            inverse
        }
        SetTransitionForAll { kind, duration_us } => {
            let inverse = restore_clips(project);
            let timeline = &mut project.timeline;
            timeline.transitions = if kind == TransitionKind::Cut {
                Vec::new()
            } else {
                timeline
                    .clips
                    .windows(2)
                    .map(|pair| Transition {
                        left: pair[0].id,
                        right: pair[1].id,
                        kind,
                        duration_us,
                    })
                    .collect()
            };
            inverse
        }

        AddAudio { item } => {
            let inverse = restore_audio(project);
            project.timeline.audio.push(*item);
            inverse
        }
        DeleteAudio { id } => {
            let inverse = restore_audio(project);
            audio_mut(project, id)?;
            project.timeline.audio.retain(|a| a.id != id);
            project
                .captions
                .sources
                .retain(|s| *s != CaptionSource::AudioItem(id));
            inverse
        }
        MoveAudio {
            id,
            start_us,
            track,
        } => edit_audio(project, id, |item| {
            item.start_us = start_us;
            item.track = track;
        })?,
        TrimAudio {
            id,
            source_in_us,
            source_out_us,
        } => edit_audio(project, id, |item| {
            item.source_in_us = source_in_us;
            item.source_out_us = source_out_us;
        })?,
        SetVolume { id, volume } => edit_audio(project, id, |item| item.volume = volume)?,
        SetFades {
            id,
            fade_in_us,
            fade_out_us,
        } => edit_audio(project, id, |item| {
            item.fade_in_us = fade_in_us;
            item.fade_out_us = fade_out_us;
        })?,
        SetDucking { id, ducking } => edit_audio(project, id, |item| item.ducking = ducking)?,
        SetClipAudio { id, audio } => edit_clip(project, id, |clip| clip.original_audio = audio)?,

        AddAudioTrack { track } => {
            let inverse = restore_tracks(project);
            project.timeline.audio_tracks.push(track);
            inverse
        }
        RemoveAudioTrack { id } => {
            let inverse = restore_tracks(project);
            let timeline = &mut project.timeline;
            let before = timeline.audio_tracks.len();
            timeline.audio_tracks.retain(|t| t.id != id);
            if timeline.audio_tracks.len() == before {
                return Err(ModelError::UnknownId(id));
            }
            let track = AudioTrackRef::User(id);
            let removed: Vec<Id> = timeline
                .audio
                .iter()
                .filter(|a| a.track == track)
                .map(|a| a.id)
                .collect();
            timeline.audio.retain(|a| a.track != track);
            project.captions.sources.retain(|source| {
                !matches!(source, CaptionSource::AudioItem(item) if removed.contains(item))
            });
            inverse
        }
        RenameAudioTrack { id, name } => {
            let inverse = restore_tracks(project);
            track_mut(project, id)?.name = name;
            inverse
        }
        ReorderAudioTrack { id, to } => {
            let inverse = restore_tracks(project);
            reposition(&mut project.timeline.audio_tracks, id, to, |t| t.id)?;
            inverse
        }
        SetTrackFlags { track, flags } => {
            let set = &mut project.timeline.track_flags;
            let target = match track {
                TrackKind::Video => &mut set.video,
                TrackKind::Widgets => &mut set.widgets,
                TrackKind::Captions => &mut set.captions,
                TrackKind::VoiceOver => &mut set.voice_over,
            };
            SetTrackFlags {
                track,
                flags: std::mem::replace(target, flags),
            }
        }
        SetAudioTrackFlags { id, flags } => {
            let target = &mut track_mut(project, id)?.flags;
            SetAudioTrackFlags {
                id,
                flags: std::mem::replace(target, flags),
            }
        }

        SetTranscript { owner, transcript } => match owner {
            TranscriptOwner::Clip(id) => edit_clip(project, id, |c| c.transcript = transcript)?,
            TranscriptOwner::Audio(id) => edit_audio(project, id, |a| a.transcript = transcript)?,
        },
        EditWord {
            owner,
            segment,
            word,
            text,
        } => edit_transcript(project, owner, |t| {
            word_mut(t, segment, word)?.text = text;
            Ok(())
        })?,
        HideSegment {
            owner,
            segment,
            hidden,
        } => edit_transcript(project, owner, |t| {
            segment_mut(t, segment)?.hidden = hidden;
            Ok(())
        })?,
        DeleteWords {
            owner,
            segment,
            words,
            deleted,
        } => edit_transcript(project, owner, |t| {
            let segment = segment_mut(t, segment)?;
            for index in words {
                let word = segment
                    .words
                    .get_mut(index)
                    .ok_or(ModelError::OutOfRange { index })?;
                word.deleted = deleted;
            }
            Ok(())
        })?,
        ToggleEmphasis {
            owner,
            segment,
            word,
        } => edit_transcript(project, owner, |t| {
            let word = word_mut(t, segment, word)?;
            let (EmphasisState::Auto(on) | EmphasisState::User(on)) = word.emphasis;
            word.emphasis = EmphasisState::User(!on);
            Ok(())
        })?,

        SetCaptionSettings { settings } => SetCaptionSettings {
            settings: Box::new(std::mem::replace(&mut project.captions, *settings)),
        },

        AddOverlay { overlay } => {
            let inverse = RestoreOverlays {
                overlays: project.timeline.overlays.clone(),
            };
            project.timeline.overlays.push(*overlay);
            inverse
        }
        UpdateOverlayTransform { id, base } => edit_overlay(project, id, |o| o.base = base)?,
        SetOverlayTiming {
            id,
            start_us,
            end_us,
        } => edit_overlay(project, id, |o| {
            o.start_us = start_us;
            o.end_us = end_us;
        })?,
        SetKeyframe { id, property, key } => edit_overlay(project, id, |o| {
            set_key(o, property, key);
        })?,
        RemoveKeyframe { id, property, t_us } => edit_overlay(project, id, |o| {
            if let Some(track) = o.keyframes.iter_mut().find(|k| k.property == property) {
                track.keys.retain(|key| key.t_us != t_us);
            }
            o.keyframes.retain(|track| !track.keys.is_empty());
        })?,
        SetMotionPath { id, path } => edit_overlay(project, id, |o| o.path = path)?,
        SetPresetAnim { id, slot, anim } => edit_overlay(project, id, |o| match slot {
            AnimSlot::In => o.anim_in = anim,
            AnimSlot::Loop => o.anim_loop = anim,
            AnimSlot::Out => o.anim_out = anim,
        })?,
        ReorderOverlay { id, to } => {
            let inverse = RestoreOverlays {
                overlays: project.timeline.overlays.clone(),
            };
            reposition(&mut project.timeline.overlays, id, to, |o| o.id)?;
            inverse
        }
        DeleteOverlay { id } => {
            let inverse = RestoreOverlays {
                overlays: project.timeline.overlays.clone(),
            };
            overlay_mut(project, id)?;
            project.timeline.overlays.retain(|o| o.id != id);
            inverse
        }

        Group { label, commands } => {
            let mut inverses = Vec::with_capacity(commands.len());
            for command in &commands {
                inverses.push(mutate(command, project)?);
            }
            inverses.reverse();
            Group {
                label,
                commands: inverses,
            }
        }

        ReplaceClip { clip } => {
            let target = clip_mut(project, clip.id)?;
            ReplaceClip {
                clip: Box::new(std::mem::replace(target, *clip)),
            }
        }
        ReplaceAudio { item } => {
            let target = audio_mut(project, item.id)?;
            ReplaceAudio {
                item: Box::new(std::mem::replace(target, *item)),
            }
        }
        ReplaceOverlay { overlay } => {
            let target = overlay_mut(project, overlay.id)?;
            ReplaceOverlay {
                overlay: Box::new(std::mem::replace(target, *overlay)),
            }
        }
        RestoreClips { clips, transitions } => {
            let inverse = restore_clips(project);
            project.timeline.clips = clips;
            project.timeline.transitions = transitions;
            inverse
        }
        RestoreAudio { audio, captions } => {
            let inverse = restore_audio(project);
            project.timeline.audio = audio;
            project.captions = *captions;
            inverse
        }
        RestoreTracks {
            tracks,
            audio,
            captions,
        } => {
            let inverse = restore_tracks(project);
            project.timeline.audio_tracks = tracks;
            project.timeline.audio = audio;
            project.captions = *captions;
            inverse
        }
        RestoreMedia { media } => RestoreMedia {
            media: std::mem::replace(&mut project.media, media),
        },
        RestoreOverlays { overlays } => RestoreOverlays {
            overlays: std::mem::replace(&mut project.timeline.overlays, overlays),
        },
    })
}

fn track_mut(project: &mut Project, id: Id) -> Result<&mut AudioTrack, ModelError> {
    project
        .timeline
        .audio_tracks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or(ModelError::UnknownId(id))
}

fn restore_tracks(project: &Project) -> Command {
    Command::RestoreTracks {
        tracks: project.timeline.audio_tracks.clone(),
        audio: project.timeline.audio.clone(),
        captions: Box::new(project.captions.clone()),
    }
}

/// Inserts or replaces a key, keeping keys sorted by time.
fn set_key(overlay: &mut OverlayItem, property: Prop, key: Key) {
    let index = overlay
        .keyframes
        .iter()
        .position(|k| k.property == property);
    let track = match index {
        Some(index) => &mut overlay.keyframes[index],
        None => {
            overlay.keyframes.push(AnimatedProperty {
                property,
                keys: Vec::new(),
            });
            let last = overlay.keyframes.len() - 1;
            &mut overlay.keyframes[last]
        }
    };
    match track.keys.binary_search_by_key(&key.t_us, |k| k.t_us) {
        Ok(found) => track.keys[found] = key,
        Err(slot) => track.keys.insert(slot, key),
    }
}

impl Command {
    /// Applies to `project` and returns the inverse. On error `project` is unchanged: the edit
    /// runs on a copy, which replaces the project only once it validates.
    pub fn apply(&self, project: &mut Project) -> Result<Command, ModelError> {
        let mut next = project.clone();
        let inverse = mutate(self, &mut next)?;
        next.validate()?;
        *project = next;
        Ok(inverse)
    }
}
