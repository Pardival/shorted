//! `evaluate(project, t)`: what is on screen at time `t`. Pure and deterministic: preview and
//! export both go through this function (ADR-0003).

use std::ops::Range;

use crate::command::{AnimSlot, TranscriptOwner};
use crate::model::*;

/// One video layer. During a transition there are two: outgoing first.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipLayer {
    pub clip_id: Id,
    pub media_id: Id,
    pub source_time_us: i64,
    pub crop: Crop,
    pub color: ColorAdjust,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionState {
    pub kind: TransitionKind,
    /// 0 at its start, 1 at its end.
    pub progress: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayLayer {
    pub overlay_id: Id,
    /// Source time for video elements.
    pub source_time_us: Option<i64>,
    pub position: Vec2,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub flip: bool,
    /// How much of the element shows, from its left edge: 1 is all of it (Wipe).
    pub reveal: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordPlace {
    pub owner: TranscriptOwner,
    pub segment: Id,
    /// Index in the segment, deleted words counted.
    pub word: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionRun {
    pub text: String,
    /// Where the word is in its transcript, so a tap on it can change it (T-1202).
    pub place: Option<WordPlace>,
    pub emphasis: bool,
    /// The word being spoken now, for karaoke.
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FrameDescription {
    pub time_us: i64,
    pub clips: Vec<ClipLayer>,
    pub transition: Option<TransitionState>,
    /// Bottom to top.
    pub overlays: Vec<OverlayLayer>,
    pub captions: Vec<CaptionRun>,
    /// How long the caption block has been on screen, for its entrance.
    pub caption_age_us: i64,
}

/// Where each clip sits on the timeline. Entering a transition overlaps the next clip with the
/// previous one, so `Project::duration_us` equals the last end.
pub(crate) struct Span<'a> {
    pub clip: &'a ClipItem,
    pub start_us: i64,
    pub end_us: i64,
    pub incoming: Option<&'a Transition>,
}

pub(crate) fn spans(timeline: &Timeline) -> Vec<Span<'_>> {
    let mut result = Vec::with_capacity(timeline.clips.len());
    let mut cursor = 0;
    let mut previous: Option<Id> = None;
    for clip in &timeline.clips {
        let incoming = previous.and_then(|left| {
            timeline
                .transitions
                .iter()
                .find(|t| t.left == left && t.right == clip.id)
        });
        let start_us = cursor - incoming.map_or(0, |t| t.duration_us);
        let end_us = start_us + clip.duration_us();
        result.push(Span {
            clip,
            start_us,
            end_us,
            incoming,
        });
        cursor = end_us;
        previous = Some(clip.id);
    }
    result
}

/// Timeline position of each clip: id, start, end.
pub fn clip_positions(project: &Project) -> Vec<(Id, i64, i64)> {
    spans(&project.timeline)
        .iter()
        .map(|span| (span.clip.id, span.start_us, span.end_us))
        .collect()
}

pub fn ease(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => t * (2.0 - t),
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
        // Damped overshoot, settling exactly on 1.
        Easing::Spring => 1.0 - (-6.0 * t).exp() * (1.0 - t) * (9.0 * t).cos(),
    }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn lerp_vec(from: Vec2, to: Vec2, t: f32) -> Vec2 {
    Vec2 {
        x: lerp(from.x, to.x, t),
        y: lerp(from.y, to.y, t),
    }
}

fn fraction(value: i64, from: i64, to: i64) -> f32 {
    if to <= from {
        1.0
    } else {
        ((value - from) as f64 / (to - from) as f64) as f32
    }
}

fn clip_layer(span: &Span<'_>, t: i64) -> ClipLayer {
    let clip = span.clip;
    let local = (t - span.start_us).clamp(0, span.end_us - span.start_us - 1);
    let source = clip.source_in_us + (local as f64 * f64::from(clip.speed)).round() as i64;
    let mut crop = clip.crop;
    if let Some(motion) = &clip.image_motion {
        let progress = fraction(t, span.start_us, span.end_us);
        crop.scale *= lerp(motion.start_scale, motion.end_scale, progress);
        crop.center = lerp_vec(motion.start_center, motion.end_center, progress);
    }
    ClipLayer {
        clip_id: clip.id,
        media_id: clip.media_id,
        source_time_us: source.min(clip.source_out_us - 1),
        crop,
        color: clip.color,
    }
}

/// The value of a key track at `t`, held before the first key and after the last.
pub(crate) fn key_value(keys: &[Key], t: i64) -> Option<f32> {
    let next = keys.partition_point(|key| key.t_us <= t);
    match (
        next.checked_sub(1).map(|i| keys[i]),
        keys.get(next).copied(),
    ) {
        (None, None) => None,
        (None, Some(key)) | (Some(key), None) => Some(key.value),
        (Some(from), Some(to)) => {
            let progress = ease(from.easing, fraction(t, from.t_us, to.t_us));
            Some(lerp(from.value, to.value, progress))
        }
    }
}

/// A property's own value at `local` microseconds into the element: its keyframes, else its base.
/// Paths and presets are left out: they are not what a keyframe records.
pub fn property_value(item: &OverlayItem, property: Prop, local: i64) -> f32 {
    let keyed = item
        .keyframes
        .iter()
        .find(|track| track.property == property)
        .and_then(|track| key_value(&track.keys, local));
    keyed.unwrap_or(match property {
        Prop::X => item.base.position.x,
        Prop::Y => item.base.position.y,
        Prop::Scale => item.base.scale,
        Prop::Rotation => item.base.rotation,
        Prop::Opacity => item.base.opacity,
    })
}

/// The keys that make the element look like `target` at `local`: one per property that differs.
/// A key already there keeps its easing; a new one eases in and out.
pub fn keys_for(item: &OverlayItem, local: i64, target: &Transform) -> Vec<(Prop, Key)> {
    const SAME: f32 = 1e-4;
    [
        (Prop::X, target.position.x),
        (Prop::Y, target.position.y),
        (Prop::Scale, target.scale),
        (Prop::Rotation, target.rotation),
        (Prop::Opacity, target.opacity),
    ]
    .into_iter()
    .filter(|(property, value)| (property_value(item, *property, local) - value).abs() > SAME)
    .map(|(property, value)| {
        let easing = item
            .keyframes
            .iter()
            .find(|track| track.property == property)
            .and_then(|track| track.keys.iter().find(|key| key.t_us == local))
            .map_or(Easing::EaseInOut, |key| key.easing);
        (
            property,
            Key {
                t_us: local,
                value,
                easing,
            },
        )
    })
    .collect()
}

/// Every time that holds a key on any property, in order, with the easing of the first property
/// keyed there.
pub fn keyframe_times(item: &OverlayItem) -> Vec<(i64, Easing)> {
    let mut times: Vec<(i64, Easing)> = item
        .keyframes
        .iter()
        .flat_map(|track| track.keys.iter().map(|key| (key.t_us, key.easing)))
        .collect();
    times.sort_by_key(|(time, _)| *time);
    times.dedup_by_key(|(time, _)| *time);
    times
}

/// Where a recorded path puts the element at `t`, for edits that start from it.
pub fn path_position_at(path: &MotionPath, t: i64) -> Option<Vec2> {
    path_position(path, t).map(|(point, _)| point)
}

/// Position along a recorded path, and its heading in degrees. Between two samples the way is
/// straight at smoothing 0 and a Catmull-Rom curve through the neighbours at 1 (T-0708).
pub(crate) fn path_position(path: &MotionPath, t: i64) -> Option<(Vec2, f32)> {
    const HEADING_STEP_US: i64 = 1_000;
    let point = path_point(path, t)?;
    let (before, after) = (
        path_point(path, t - HEADING_STEP_US)?,
        path_point(path, t + HEADING_STEP_US)?,
    );
    let heading = if before == after {
        segment_heading(path, t)
    } else {
        (after.y - before.y).atan2(after.x - before.x).to_degrees()
    };
    Some((point, heading))
}

/// The direction of the straight segment at `t`, for the ends where the curve stops moving.
fn segment_heading(path: &MotionPath, t: i64) -> f32 {
    let samples = &path.samples;
    let Some((from, to)) = segment(samples, t) else {
        return 0.0;
    };
    let (from, to) = (&samples[from], &samples[to]);
    (to.y - from.y).atan2(to.x - from.x).to_degrees()
}

/// The two samples around `t`: the first two before the path, the last two after it.
fn segment(samples: &[PathSample], t: i64) -> Option<(usize, usize)> {
    let count = samples.len();
    if count == 0 {
        return None;
    }
    if count == 1 {
        return Some((0, 0));
    }
    let next = samples.partition_point(|s| s.t_us <= t);
    Some(match next {
        0 => (0, 1),
        n if n == count => (n - 2, n - 1),
        n => (n - 1, n),
    })
}

fn path_point(path: &MotionPath, t: i64) -> Option<Vec2> {
    let samples = &path.samples;
    let (from, to) = segment(samples, t)?;
    let at = |i: usize| Vec2 {
        x: samples[i].x,
        y: samples[i].y,
    };
    let progress = fraction(t, samples[from].t_us, samples[to].t_us).clamp(0.0, 1.0);
    let straight = lerp_vec(at(from), at(to), progress);
    let smoothing = path.smoothing.clamp(0.0, 1.0);
    if smoothing == 0.0 || from == to {
        return Some(straight);
    }
    let before = at(from.saturating_sub(1));
    let after = at((to + 1).min(samples.len() - 1));
    let curve = catmull_rom(before, at(from), at(to), after, progress);
    Some(lerp_vec(straight, curve, smoothing))
}

fn catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let (t2, t3) = (t * t, t * t * t);
    let axis = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * (2.0 * b
            + (c - a) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (3.0 * b - a - 3.0 * c + d) * t3)
    };
    Vec2 {
        x: axis(p0.x, p1.x, p2.x, p3.x),
        y: axis(p0.y, p1.y, p2.y, p3.y),
    }
}

/// Drops samples that lie within `tolerance` canvas units of the line between their neighbours
/// (Ramer-Douglas-Peucker), keeping the first and last. Timing stays with the kept samples.
pub fn simplify_path(samples: &[PathSample], tolerance: f32) -> Vec<PathSample> {
    if samples.len() < 3 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;
    let mut spans = vec![(0, samples.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        let (a, b) = (&samples[first], &samples[last]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        let mut farthest = (0.0, first);
        for (index, sample) in samples.iter().enumerate().take(last).skip(first + 1) {
            let distance = if length == 0.0 {
                (sample.x - a.x).hypot(sample.y - a.y)
            } else {
                ((sample.x - a.x) * dy - (sample.y - a.y) * dx).abs() / length
            };
            if distance > farthest.0 {
                farthest = (distance, index);
            }
        }
        if farthest.0 > tolerance {
            keep[farthest.1] = true;
            spans.push((first, farthest.1));
            spans.push((farthest.1, last));
        }
    }
    samples
        .iter()
        .zip(keep)
        .filter_map(|(sample, kept)| kept.then_some(*sample))
        .collect()
}

/// Relative changes a preset makes; they compose with paths and keyframes (docs/06 §3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifier {
    pub offset: Vec2,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub reveal: f32,
}

pub const NEUTRAL: Modifier = Modifier {
    offset: Vec2 { x: 0.0, y: 0.0 },
    scale: 1.0,
    rotation: 0.0,
    opacity: 1.0,
    reveal: 1.0,
};

/// What a preset does `progress` (0 to 1) through its slot: an In arrives, an Out leaves, a
/// Loop goes round once. For previews of the presets, drawn by the same code as the frames.
pub fn preset_modifier(kind: PresetKind, slot: AnimSlot, progress: f32) -> Modifier {
    let progress = progress.clamp(0.0, 1.0);
    match slot {
        AnimSlot::In => enter_exit(kind, progress),
        AnimSlot::Out => enter_exit(kind, 1.0 - progress),
        AnimSlot::Loop => looping(kind, progress),
    }
}
const SLIDE_DISTANCE: f32 = CANVAS_WIDTH as f32 * 0.5;
const LOOP_AMOUNT: f32 = 24.0;

/// `visible` is 0 when fully gone and 1 when fully present.
fn enter_exit(kind: PresetKind, visible: f32) -> Modifier {
    let hidden = 1.0 - ease(Easing::EaseOut, visible);
    let slide = |x: f32, y: f32| Modifier {
        offset: Vec2 {
            x: x * SLIDE_DISTANCE * hidden,
            y: y * SLIDE_DISTANCE * hidden,
        },
        opacity: visible,
        ..NEUTRAL
    };
    match kind {
        PresetKind::Pop => Modifier {
            scale: ease(Easing::Spring, visible),
            ..NEUTRAL
        },
        PresetKind::SlideLeft => slide(1.0, 0.0),
        PresetKind::SlideRight => slide(-1.0, 0.0),
        PresetKind::SlideUp => slide(0.0, 1.0),
        PresetKind::SlideDown => slide(0.0, -1.0),
        PresetKind::Zoom | PresetKind::Shrink => Modifier {
            scale: visible,
            opacity: visible,
            ..NEUTRAL
        },
        PresetKind::Stamp => Modifier {
            scale: 1.0 + 2.0 * hidden,
            opacity: visible,
            ..NEUTRAL
        },
        PresetKind::Wipe => Modifier {
            reveal: ease(Easing::EaseInOut, visible),
            ..NEUTRAL
        },
        _ => Modifier {
            opacity: visible,
            ..NEUTRAL
        },
    }
}

fn looping(kind: PresetKind, phase: f32) -> Modifier {
    let wave = (phase * std::f32::consts::TAU).sin();
    match kind {
        PresetKind::Float => Modifier {
            offset: Vec2 {
                x: 0.0,
                y: wave * LOOP_AMOUNT,
            },
            ..NEUTRAL
        },
        PresetKind::Pulse => Modifier {
            scale: 1.0 + 0.08 * wave,
            ..NEUTRAL
        },
        PresetKind::Wiggle => Modifier {
            rotation: 6.0 * wave,
            ..NEUTRAL
        },
        PresetKind::Rotate => Modifier {
            rotation: 360.0 * phase,
            ..NEUTRAL
        },
        PresetKind::Bounce => Modifier {
            offset: Vec2 {
                x: 0.0,
                y: -wave.abs() * LOOP_AMOUNT,
            },
            ..NEUTRAL
        },
        _ => NEUTRAL,
    }
}

fn overlay_layer(item: &OverlayItem, t: i64) -> OverlayLayer {
    let local = t - item.start_us;
    let mut position = item.base.position;
    let mut scale = item.base.scale;
    let mut rotation = item.base.rotation;
    let mut opacity = item.base.opacity;
    let mut reveal: f32 = 1.0;

    if let Some((point, heading)) = item.path.as_ref().and_then(|p| path_position(p, local)) {
        position = point;
        if item.path.as_ref().is_some_and(|p| p.orient_to_path) {
            rotation += heading;
        }
    }
    for track in &item.keyframes {
        let Some(value) = key_value(&track.keys, local) else {
            continue;
        };
        match track.property {
            Prop::X => position.x = value,
            Prop::Y => position.y = value,
            Prop::Scale => scale = value,
            Prop::Rotation => rotation = value,
            Prop::Opacity => opacity = value,
        }
    }

    let length = item.end_us - item.start_us;
    let mut modifiers = Vec::with_capacity(3);
    if let Some(anim) = item
        .anim_in
        .filter(|a| a.duration_us > 0 && local < a.duration_us)
    {
        modifiers.push(enter_exit(anim.kind, fraction(local, 0, anim.duration_us)));
    }
    if let Some(anim) = item.anim_loop.filter(|a| a.duration_us > 0) {
        modifiers.push(looping(
            anim.kind,
            (local % anim.duration_us) as f32 / anim.duration_us as f32,
        ));
    }
    if let Some(anim) = item
        .anim_out
        .filter(|a| a.duration_us > 0 && length - local <= a.duration_us)
    {
        modifiers.push(enter_exit(
            anim.kind,
            fraction(length - local, 0, anim.duration_us),
        ));
    }
    for modifier in modifiers {
        position.x += modifier.offset.x;
        position.y += modifier.offset.y;
        scale *= modifier.scale;
        rotation += modifier.rotation;
        opacity *= modifier.opacity;
        reveal = reveal.min(modifier.reveal);
    }

    OverlayLayer {
        overlay_id: item.id,
        source_time_us: match item.kind {
            OverlayKind::Video { source_in_us, .. } => Some(source_in_us + local),
            _ => None,
        },
        position,
        scale,
        rotation,
        opacity: opacity.clamp(0.0, 1.0),
        flip: item.base.flip,
        reveal: reveal.clamp(0.0, 1.0),
    }
}

/// The block of words on screen at `source_time`, in the grouping of the settings, and how long
/// it has been showing.
fn word_block(
    owner: TranscriptOwner,
    transcript: &Transcript,
    source_time: i64,
    grouping: Grouping,
    used: Range<i64>,
) -> Option<(Vec<CaptionRun>, i64)> {
    for segment in transcript.segments.iter().filter(|s| !s.hidden) {
        // Words trimmed out of the edit do not take a place in a group.
        let words: Vec<(usize, &Word)> = segment
            .words
            .iter()
            .enumerate()
            .filter(|(_, w)| !w.deleted && used.contains(&w.start_us))
            .collect();
        let size = match grouping {
            Grouping::OneWord => 1,
            Grouping::UpTo(count) => usize::from(count).max(1),
            Grouping::Sentence => words.len().max(1),
        };
        for group in words.chunks(size) {
            let (Some((_, first)), Some((_, last))) = (group.first(), group.last()) else {
                continue;
            };
            if !(first.start_us..last.end_us).contains(&source_time) {
                continue;
            }
            let runs = group
                .iter()
                .map(|(index, word)| {
                    let (EmphasisState::Auto(emphasis) | EmphasisState::User(emphasis)) =
                        word.emphasis;
                    CaptionRun {
                        text: word.text.clone(),
                        place: Some(WordPlace {
                            owner,
                            segment: segment.id,
                            word: *index,
                        }),
                        emphasis,
                        active: (word.start_us..word.end_us).contains(&source_time),
                    }
                })
                .collect();
            return Some((runs, source_time - first.start_us));
        }
    }
    None
}

/// The caption block at `t`: the first source with words there wins.
fn caption_block(project: &Project, layers: &[ClipLayer], t: i64) -> (Vec<CaptionRun>, i64) {
    let settings = &project.captions;
    if !settings.enabled || project.timeline.track_flags.captions.hidden {
        return (Vec::new(), 0);
    }
    for source in &settings.sources {
        let block = match source {
            CaptionSource::OriginalSound => {
                // The incoming clip wins: it is the one being heard at the end of a transition.
                let layer = layers.last();
                let clip =
                    layer.and_then(|l| project.timeline.clips.iter().find(|c| c.id == l.clip_id));
                match (layer, clip) {
                    (Some(layer), Some(clip)) if clip.original_audio.transcribe => {
                        clip.transcript.as_ref().and_then(|t| {
                            let used = clip.source_in_us..clip.source_out_us;
                            word_block(
                                TranscriptOwner::Clip(clip.id),
                                t,
                                layer.source_time_us,
                                settings.grouping,
                                used,
                            )
                        })
                    }
                    _ => None,
                }
            }
            CaptionSource::AudioItem(id) => {
                let item = project.timeline.audio.iter().find(|a| a.id == *id);
                item.filter(|item| (item.start_us..item.end_us()).contains(&t))
                    .and_then(|item| {
                        let source_time = item.source_in_us + t - item.start_us;
                        let transcript = item.transcript.as_ref()?;
                        let used = item.source_in_us..item.source_out_us;
                        word_block(
                            TranscriptOwner::Audio(item.id),
                            transcript,
                            source_time,
                            settings.grouping,
                            used,
                        )
                    })
            }
        };
        if let Some(block) = block {
            return block;
        }
    }
    (Vec::new(), 0)
}

/// Describes the frame at `time_us`, clamped to the project: scrubbing past the end shows the
/// last frame. An empty project gives an empty frame.
pub fn evaluate(project: &Project, time_us: i64) -> FrameDescription {
    let timeline = &project.timeline;
    let spans = spans(timeline);
    let end = spans.iter().map(|s| s.end_us).max().unwrap_or(0);
    let t = time_us.clamp(0, (end - 1).max(0));

    let mut clips = Vec::with_capacity(2);
    let mut transition = None;
    if !timeline.track_flags.video.hidden {
        for (index, span) in spans.iter().enumerate() {
            if !(span.start_us..span.end_us).contains(&t) {
                continue;
            }
            if let Some(incoming) = span.incoming
                && let Some(left) = index.checked_sub(1).map(|i| &spans[i])
                && (left.start_us..left.end_us).contains(&t)
            {
                transition = Some(TransitionState {
                    kind: incoming.kind,
                    progress: fraction(t, span.start_us, left.end_us).clamp(0.0, 1.0),
                });
            }
            clips.push(clip_layer(span, t));
        }
    }

    let overlays = if timeline.track_flags.widgets.hidden {
        Vec::new()
    } else {
        timeline
            .overlays
            .iter()
            .filter(|item| !item.hidden && (item.start_us..item.end_us).contains(&t))
            .map(|item| overlay_layer(item, t))
            .collect()
    };
    let (captions, caption_age_us) = caption_block(project, &clips, t);
    FrameDescription {
        time_us: t,
        clips,
        transition,
        overlays,
        captions,
        caption_age_us,
    }
}
