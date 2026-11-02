//! Range edits across tracks (docs/06 §4). Locked tracks and locked items are left alone.

use std::ops::Range;

use uuid::Uuid;

use crate::error::ModelError;
use crate::evaluate::{key_value, path_position, spans};
use crate::model::*;

/// Hands out the ids of new parts, deterministically so that journal replay gives equal projects.
struct Ids {
    next: u128,
}

impl Ids {
    fn new(seed: Id) -> Self {
        Self {
            next: seed.as_u128(),
        }
    }

    fn take(&mut self) -> Id {
        let id = Uuid::from_u128(self.next);
        self.next = self.next.wrapping_add(1);
        id
    }
}

fn check_range(start_us: i64, end_us: i64) -> Result<(), ModelError> {
    if start_us >= 0 && start_us < end_us {
        Ok(())
    } else {
        Err(ModelError::Invalid { what: "range" })
    }
}

fn track_flags(timeline: &Timeline, track: AudioTrackRef) -> TrackFlags {
    match track {
        AudioTrackRef::VoiceOver => timeline.track_flags.voice_over,
        AudioTrackRef::User(id) => timeline
            .audio_tracks
            .iter()
            .find(|t| t.id == id)
            .map_or_else(TrackFlags::default, |t| t.flags),
    }
}

/// The part of a clip between two timeline times, given where the clip starts.
fn clip_part(clip: &ClipItem, clip_start: i64, from: i64, to: i64, id: Id) -> ClipItem {
    let source = |t: i64| {
        clip.source_in_us + ((t - clip_start) as f64 * f64::from(clip.speed)).round() as i64
    };
    let mut part = clip.clone();
    part.id = id;
    part.source_in_us = source(from);
    part.source_out_us = source(to).min(clip.source_out_us);
    part
}

fn audio_part(item: &AudioItem, from: i64, to: i64, id: Id) -> AudioItem {
    let mut part = item.clone();
    part.id = id;
    part.start_us = from;
    part.source_in_us = item.source_in_us + from - item.start_us;
    part.source_out_us = item.source_in_us + to - item.start_us;
    part
}

/// Keys inside `window`, moved to its start. A key is added at each edge the animation crosses,
/// so that the value at the cut does not jump.
fn cut_keys(keys: &[Key], window: &Range<i64>) -> Vec<Key> {
    let inside = keys.iter().filter(|key| window.contains(&key.t_us));
    let mut result: Vec<Key> = Vec::with_capacity(keys.len() + 2);
    let easing = keys
        .iter()
        .rev()
        .find(|key| key.t_us < window.start)
        .map(|key| key.easing);
    let on_edge = keys.iter().any(|key| key.t_us == window.start);
    if let (false, Some(easing), Some(value)) = (on_edge, easing, key_value(keys, window.start)) {
        result.push(Key {
            t_us: window.start,
            value,
            easing,
        });
    }
    result.extend(inside);
    let crossed = keys.iter().any(|key| key.t_us >= window.end);
    if crossed && let Some(value) = key_value(keys, window.end) {
        let easing = result.last().map_or(Easing::Linear, |key| key.easing);
        result.push(Key {
            t_us: window.end,
            value,
            easing,
        });
    }
    for key in &mut result {
        key.t_us -= window.start;
    }
    result
}

fn cut_samples(path: &MotionPath, window: &Range<i64>) -> Vec<PathSample> {
    let samples = &path.samples;
    let mut result: Vec<PathSample> = Vec::with_capacity(samples.len() + 2);
    let at = |t_us: i64| {
        path_position(path, t_us).map(|(point, _)| PathSample {
            t_us,
            x: point.x,
            y: point.y,
        })
    };
    let starts_before = samples.first().is_some_and(|s| s.t_us < window.start);
    if starts_before && !samples.iter().any(|s| s.t_us == window.start) {
        result.extend(at(window.start));
    }
    result.extend(samples.iter().filter(|s| window.contains(&s.t_us)));
    if samples.last().is_some_and(|s| s.t_us >= window.end) {
        result.extend(at(window.end));
    }
    for sample in &mut result {
        sample.t_us -= window.start;
    }
    result
}

fn overlay_part(item: &OverlayItem, from: i64, to: i64, id: Id) -> OverlayItem {
    let cut = from - item.start_us;
    let length = to - from;
    let mut part = item.clone();
    part.id = id;
    part.start_us = from;
    part.end_us = to;
    if let OverlayKind::Video { source_in_us, .. } = &mut part.kind {
        *source_in_us += cut;
    }
    let window = cut..cut + length;
    for track in &mut part.keyframes {
        track.keys = cut_keys(&track.keys, &window);
    }
    part.keyframes.retain(|track| !track.keys.is_empty());
    if let Some(path) = &mut part.path {
        path.samples = cut_samples(path, &window);
    }
    part
}

type Piece = (i64, i64);

/// The kept pieces of `[start, end)` once `[cut_start, cut_end)` is taken out.
fn kept(start: i64, end: i64, cut_start: i64, cut_end: i64) -> (Option<Piece>, Option<Piece>) {
    let left = (start < cut_start).then_some((start, end.min(cut_start)));
    let right = (end > cut_end).then_some((start.max(cut_end), end));
    (left, right)
}

fn delete_from_clips(timeline: &mut Timeline, start_us: i64, end_us: i64, ids: &mut Ids) {
    struct Part {
        clip: ClipItem,
        origin: Id,
        offset: i64,
        start: i64,
        end: i64,
    }
    let spans = spans(timeline);
    // A transition survives only if its whole overlap lies outside the range.
    let surviving: Vec<&Transition> = spans
        .iter()
        .zip(spans.iter().skip(1))
        .filter_map(|(left, right)| {
            let transition = right.incoming?;
            (left.end_us <= start_us || right.start_us >= end_us).then_some(transition)
        })
        .collect();
    let mut parts: Vec<Part> = Vec::new();
    for span in &spans {
        let (left, right) = kept(span.start_us, span.end_us, start_us, end_us);
        for (index, piece) in [left, right].into_iter().flatten().enumerate() {
            parts.push(Part {
                clip: span.clip.clone(),
                origin: span.clip.id,
                offset: span.start_us,
                start: piece.0,
                end: piece.1,
                // Replaced just below for a second part.
            });
            if index > 0
                && let Some(part) = parts.last_mut()
            {
                part.clip.id = ids.take();
            }
        }
    }
    // Without its transition, an overlap would lengthen the project: the later part gives way.
    let mut reached = i64::MIN;
    for index in 0..parts.len() {
        let joined = index > 0
            && surviving
                .iter()
                .any(|t| t.left == parts[index - 1].origin && t.right == parts[index].origin);
        if !joined {
            parts[index].start = parts[index].start.max(reached);
        }
        reached = reached.max(parts[index].end);
    }
    parts.retain(|part| part.start < part.end);

    let renamed = |origin: Id, last: bool, parts: &[Part]| {
        let mut matching = parts.iter().filter(|part| part.origin == origin);
        if last {
            matching.next_back()
        } else {
            matching.next()
        }
        .map(|part| part.clip.id)
    };
    timeline.transitions = surviving
        .iter()
        .filter_map(|transition| {
            Some(Transition {
                left: renamed(transition.left, true, &parts)?,
                right: renamed(transition.right, false, &parts)?,
                ..**transition
            })
        })
        .collect();
    timeline.clips = parts
        .iter()
        .map(|part| clip_part(&part.clip, part.offset, part.start, part.end, part.clip.id))
        .collect();
}

fn delete_from_audio(project: &mut Project, start_us: i64, end_us: i64, ids: &mut Ids) {
    let length = end_us - start_us;
    let mut result = Vec::with_capacity(project.timeline.audio.len());
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for item in &project.timeline.audio {
        if track_flags(&project.timeline, item.track).locked {
            result.push(item.clone());
            continue;
        }
        let (left, right) = kept(item.start_us, item.end_us(), start_us, end_us);
        if let Some((from, to)) = left {
            result.push(audio_part(item, from, to, item.id));
        }
        if let Some((from, to)) = right {
            let id = if left.is_some() {
                let id = ids.take();
                added.push((item.id, id));
                id
            } else {
                item.id
            };
            let mut part = audio_part(item, from, to, id);
            part.start_us -= length;
            result.push(part);
        }
        if left.is_none() && right.is_none() {
            removed.push(item.id);
        }
    }
    project.timeline.audio = result;
    let sources = &mut project.captions.sources;
    sources
        .retain(|source| !matches!(source, CaptionSource::AudioItem(id) if removed.contains(id)));
    for (original, id) in added {
        if sources.contains(&CaptionSource::AudioItem(original)) {
            sources.push(CaptionSource::AudioItem(id));
        }
    }
}

fn delete_from_overlays(timeline: &mut Timeline, start_us: i64, end_us: i64, ids: &mut Ids) {
    let length = end_us - start_us;
    let mut result = Vec::with_capacity(timeline.overlays.len());
    for item in &timeline.overlays {
        if timeline.track_flags.widgets.locked || item.locked {
            result.push(item.clone());
            continue;
        }
        let (left, right) = kept(item.start_us, item.end_us, start_us, end_us);
        if let Some((from, to)) = left {
            result.push(overlay_part(item, from, to, item.id));
        }
        if let Some((from, to)) = right {
            let id = if left.is_some() { ids.take() } else { item.id };
            let mut part = overlay_part(item, from, to, id);
            part.start_us -= length;
            part.end_us -= length;
            result.push(part);
        }
    }
    timeline.overlays = result;
}

pub(crate) fn delete_range(
    project: &mut Project,
    start_us: i64,
    end_us: i64,
    seed: Id,
) -> Result<(), ModelError> {
    check_range(start_us, end_us)?;
    let mut ids = Ids::new(seed);
    if !project.timeline.track_flags.video.locked {
        delete_from_clips(&mut project.timeline, start_us, end_us, &mut ids);
    }
    delete_from_audio(project, start_us, end_us, &mut ids);
    delete_from_overlays(&mut project.timeline, start_us, end_us, &mut ids);
    Ok(())
}

pub(crate) fn split_all_at(project: &mut Project, t_us: i64, seed: Id) {
    let mut ids = Ids::new(seed);
    let timeline = &mut project.timeline;
    if !timeline.track_flags.video.locked {
        let positions: Vec<(i64, i64, bool)> = spans(timeline)
            .iter()
            .map(|s| (s.start_us, s.end_us, s.incoming.is_some()))
            .collect();
        let mut clips = Vec::with_capacity(timeline.clips.len() + 1);
        for (clip, (start, end, _)) in timeline.clips.iter().zip(positions) {
            if (start + 1..end).contains(&t_us) {
                let right = ids.take();
                clips.push(clip_part(clip, start, start, t_us, clip.id));
                clips.push(clip_part(clip, start, t_us, end, right));
                for transition in &mut timeline.transitions {
                    if transition.left == clip.id {
                        transition.left = right;
                    }
                }
            } else {
                clips.push(clip.clone());
            }
        }
        timeline.clips = clips;
    }

    let flags: Vec<bool> = timeline
        .audio
        .iter()
        .map(|item| track_flags(timeline, item.track).locked)
        .collect();
    let mut audio = Vec::with_capacity(timeline.audio.len() + 1);
    for (item, locked) in timeline.audio.iter().zip(flags) {
        if !locked && (item.start_us + 1..item.end_us()).contains(&t_us) {
            let end = item.end_us();
            audio.push(audio_part(item, item.start_us, t_us, item.id));
            audio.push(audio_part(item, t_us, end, ids.take()));
        } else {
            audio.push(item.clone());
        }
    }
    timeline.audio = audio;

    let locked = timeline.track_flags.widgets.locked;
    let mut overlays = Vec::with_capacity(timeline.overlays.len() + 1);
    for item in &timeline.overlays {
        if !locked && !item.locked && (item.start_us + 1..item.end_us).contains(&t_us) {
            overlays.push(overlay_part(item, item.start_us, t_us, item.id));
            overlays.push(overlay_part(item, t_us, item.end_us, ids.take()));
        } else {
            overlays.push(item.clone());
        }
    }
    timeline.overlays = overlays;
}

/// Silences audio items inside the range, splitting those crossing an edge.
pub(crate) fn mute_range(
    project: &mut Project,
    start_us: i64,
    end_us: i64,
    seed: Id,
) -> Result<(), ModelError> {
    check_range(start_us, end_us)?;
    let mut ids = Ids::new(seed);
    let timeline = &project.timeline;
    let mut audio = Vec::with_capacity(timeline.audio.len() + 2);
    for item in &timeline.audio {
        let end = item.end_us();
        let overlaps = item.start_us < end_us && start_us < end;
        if track_flags(timeline, item.track).locked || !overlaps {
            audio.push(item.clone());
            continue;
        }
        let (from, to) = (item.start_us.max(start_us), end.min(end_us));
        let mut first = true;
        let mut next_id = |ids: &mut Ids| {
            let id = if first { item.id } else { ids.take() };
            first = false;
            id
        };
        if item.start_us < from {
            let id = next_id(&mut ids);
            audio.push(audio_part(item, item.start_us, from, id));
        }
        let id = next_id(&mut ids);
        let mut muted = audio_part(item, from, to, id);
        muted.volume = 0.0;
        audio.push(muted);
        if to < end {
            let id = next_id(&mut ids);
            audio.push(audio_part(item, to, end, id));
        }
    }
    project.timeline.audio = audio;
    Ok(())
}
