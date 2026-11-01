//! `evaluate(project, t)`: what is on screen at time `t`. Pure and deterministic: preview and
//! export both go through this function (ADR-0003).

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
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionRun {
    pub text: String,
    pub emphasis: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FrameDescription {
    pub time_us: i64,
    pub clips: Vec<ClipLayer>,
    pub transition: Option<TransitionState>,
    /// Bottom to top.
    pub overlays: Vec<OverlayLayer>,
    pub captions: Vec<CaptionRun>,
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

/// Position along a recorded path. Smoothing is applied by T-0708.
pub(crate) fn path_position(path: &MotionPath, t: i64) -> Option<(Vec2, f32)> {
    let samples = &path.samples;
    let first = samples.first()?;
    let next = samples.partition_point(|s| s.t_us <= t);
    let point = |s: &PathSample| Vec2 { x: s.x, y: s.y };
    let (from, to) = match next {
        0 => (first, samples.get(1).unwrap_or(first)),
        n if n == samples.len() => (
            samples.get(n.saturating_sub(2)).unwrap_or(first),
            &samples[n - 1],
        ),
        n => (&samples[n - 1], &samples[n]),
    };
    let position = lerp_vec(
        point(from),
        point(to),
        fraction(t, from.t_us, to.t_us).clamp(0.0, 1.0),
    );
    let heading = (to.y - from.y).atan2(to.x - from.x).to_degrees();
    Some((position, heading))
}

/// Relative changes a preset makes; they compose with paths and keyframes (docs/06 §3).
#[derive(Clone, Copy)]
struct Modifier {
    offset: Vec2,
    scale: f32,
    rotation: f32,
    opacity: f32,
}

const NEUTRAL: Modifier = Modifier {
    offset: Vec2 { x: 0.0, y: 0.0 },
    scale: 1.0,
    rotation: 0.0,
    opacity: 1.0,
};
const SLIDE_DISTANCE: f32 = CANVAS_WIDTH as f32 * 0.5;
const LOOP_AMOUNT: f32 = 24.0;

/// `visible` is 0 when fully gone and 1 when fully present.
fn enter_exit(kind: PresetKind, visible: f32) -> Modifier {
    let hidden = 1.0 - visible;
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
    }
}

fn add_words(transcript: &Transcript, source_time: i64, captions: &mut Vec<CaptionRun>) {
    let words = transcript
        .segments
        .iter()
        .filter(|segment| !segment.hidden)
        .flat_map(|segment| &segment.words)
        .filter(|word| !word.deleted && (word.start_us..word.end_us).contains(&source_time));
    for word in words {
        let (EmphasisState::Auto(emphasis) | EmphasisState::User(emphasis)) = word.emphasis;
        captions.push(CaptionRun {
            text: word.text.clone(),
            emphasis,
        });
    }
}

/// Spoken words at `t`. Grouping and layout come with the caption engine (T-0603).
fn caption_runs(project: &Project, layers: &[ClipLayer], t: i64) -> Vec<CaptionRun> {
    let settings = &project.captions;
    let mut captions = Vec::new();
    if !settings.enabled || project.timeline.track_flags.captions.hidden {
        return captions;
    }
    for source in &settings.sources {
        match source {
            CaptionSource::OriginalSound => {
                // The incoming clip wins: it is the one being heard at the end of a transition.
                let layer = layers.last();
                let clip =
                    layer.and_then(|l| project.timeline.clips.iter().find(|c| c.id == l.clip_id));
                if let (Some(layer), Some(clip)) = (layer, clip)
                    && clip.original_audio.transcribe
                    && let Some(transcript) = &clip.transcript
                {
                    add_words(transcript, layer.source_time_us, &mut captions);
                }
            }
            CaptionSource::AudioItem(id) => {
                let item = project.timeline.audio.iter().find(|a| a.id == *id);
                if let Some(item) = item
                    && (item.start_us..item.end_us()).contains(&t)
                    && let Some(transcript) = &item.transcript
                {
                    let source_time = item.source_in_us + t - item.start_us;
                    add_words(transcript, source_time, &mut captions);
                }
            }
        }
    }
    captions
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
    let captions = caption_runs(project, &clips, t);
    FrameDescription {
        time_us: t,
        clips,
        transition,
        overlays,
        captions,
    }
}
