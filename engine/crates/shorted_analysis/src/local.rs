//! The local plan (docs/07 §2): trims and cuts of dead time and unusable picture, never through
//! speech, ordered by capture time. Its reasons are codes the review screen words.

use shorted_ai::plan::{Aliases, EditPlan, PlanOp, PlanSource};
use shorted_core::model::{ClipItem, Id, Project};

use crate::frames::{Defect, FrameConfig, FrameMetrics, bad_ranges};
use crate::speech::{Range, SpeechConfig, dead_ranges, merge, speech_ranges, with_words};

/// Interior stretches shorter than this are left: a cut that small looks like a glitch.
const MIN_CUT_US: i64 = 300_000;

pub const REASON_SILENCE: &str = "silence";
pub const REASON_DUPLICATE: &str = "duplicate";

/// Adds every earlier take of a sentence said again later to what its clip can lose.
pub fn add_duplicate_takes(project: &Project, analyses: &mut [ClipAnalysis]) {
    for (clip, range) in crate::takes::duplicate_takes(project) {
        let Some(analysis) = analyses.iter_mut().find(|a| a.clip == clip) else {
            continue;
        };
        let Some(item) = project.timeline.clips.iter().find(|c| c.id == clip) else {
            continue;
        };
        let clamped = (
            range.0.max(item.source_in_us),
            range.1.min(item.source_out_us),
        );
        if clamped.1 > clamped.0 {
            analysis.removable.push((clamped, REASON_DUPLICATE));
            analysis.removable.sort_by_key(|(range, _)| *range);
        }
    }
}

pub fn defect_reason(defect: Defect) -> &'static str {
    match defect {
        Defect::Blur => "blur",
        Defect::Shake => "shake",
        Defect::Dark => "dark",
        Defect::Bright => "bright",
    }
}

/// What the analysis found in one clip, in source microseconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipAnalysis {
    pub clip: Id,
    pub speech: Vec<Range>,
    /// With the reason each stretch can go.
    pub removable: Vec<(Range, &'static str)>,
}

/// Sound of a clip: mono samples from the start of its source, and their rate.
pub struct Sound<'a> {
    pub mono: &'a [f32],
    pub rate: u32,
}

impl Sound<'_> {
    /// Where its speech is: what the cache keeps instead of the samples.
    pub fn speech(&self, config: &SpeechConfig) -> Vec<Range> {
        speech_ranges(self.mono, self.rate, config)
    }
}

/// `heard` is the speech found in the clip's sound from the start of its source, or `None`
/// when it has no sound.
pub fn analyse_clip(
    clip: &ClipItem,
    heard: Option<&[Range]>,
    frames: &[FrameMetrics],
    frame_step_us: i64,
    speech_config: &SpeechConfig,
    frame_config: &FrameConfig,
) -> ClipAnalysis {
    let (start, end) = (clip.source_in_us, clip.source_out_us);
    let words: Vec<Range> = clip
        .transcript
        .iter()
        .flat_map(|t| &t.segments)
        .flat_map(|s| &s.words)
        .map(|w| (w.start_us, w.end_us))
        .collect();
    let speech = with_words(heard.unwrap_or_default(), &words, speech_config);
    let mut removable: Vec<(Range, &'static str)> = Vec::new();
    // Without a sound track, silence says nothing about the clip.
    if heard.is_some() {
        removable.extend(
            dead_ranges(&speech, start, end, speech_config)
                .into_iter()
                .map(|range| (range, REASON_SILENCE)),
        );
    }
    let padded: Vec<Range> = speech
        .iter()
        .map(|(a, b)| (a - speech_config.padding_us, b + speech_config.padding_us))
        .collect();
    for (range, defect) in bad_ranges(frames, frame_step_us, frame_config) {
        for piece in subtract(range, &padded) {
            removable.push((piece, defect_reason(defect)));
        }
    }
    let mut clamped: Vec<(Range, &'static str)> = removable
        .into_iter()
        .map(|((a, b), why)| ((a.max(start), b.min(end)), why))
        .filter(|((a, b), _)| b > a)
        .collect();
    clamped.sort_by_key(|(range, _)| *range);
    ClipAnalysis {
        clip: clip.id,
        speech,
        removable: clamped,
    }
}

/// `range` without the parts inside `holes`.
fn subtract(range: Range, holes: &[Range]) -> Vec<Range> {
    let mut pieces = vec![range];
    for (a, b) in holes {
        pieces = pieces
            .into_iter()
            .flat_map(|(from, to)| {
                if *b <= from || *a >= to {
                    vec![(from, to)]
                } else {
                    [(from, *a), (*b, to)]
                        .into_iter()
                        .filter(|(x, y)| y > x)
                        .collect()
                }
            })
            .collect();
    }
    pieces
}

/// The reason of the first stretch inside `range`; a retake says more than its silences.
fn reason_for(range: Range, removable: &[(Range, &'static str)]) -> &'static str {
    let inside = || {
        removable
            .iter()
            .filter(move |((a, b), _)| *a < range.1 && *b > range.0)
    };
    inside()
        .find(|(_, why)| *why == REASON_DUPLICATE)
        .or_else(|| inside().next())
        .map_or(REASON_SILENCE, |(_, why)| why)
}

/// One plan for the whole project: what each clip loses, and the order of capture.
pub fn local_plan(project: &Project, analyses: &[ClipAnalysis], aliases: &Aliases) -> EditPlan {
    let mut ops = Vec::new();
    for clip in &project.timeline.clips {
        let (Some(alias), Some(analysis)) = (
            aliases.clip_alias(clip.id),
            analyses.iter().find(|a| a.clip == clip.id),
        ) else {
            continue;
        };
        let (start, end) = (clip.source_in_us, clip.source_out_us);
        let gone = merge(
            analysis.removable.iter().map(|(range, _)| *range).collect(),
            0,
        );
        let reason = |range| Some(reason_for(range, &analysis.removable).to_string());
        // A clip whose every word goes has nothing left worth keeping, only its silences.
        let speechless = !analysis.speech.is_empty()
            && analysis.speech.iter().all(|(a, b)| {
                let (a, b) = ((*a).max(start), (*b).min(end));
                b <= a || gone.iter().any(|(x, y)| *x <= a && *y >= b)
            });
        if gone.first() == Some(&(start, end)) || speechless {
            ops.push(PlanOp::Drop {
                clip: alias.into(),
                reason: reason((start, end)),
            });
            continue;
        }
        let mut kept_start = start;
        let mut kept_end = end;
        let mut trim_reason = None;
        let mut inner = Vec::new();
        for range in gone {
            if range.0 <= start {
                kept_start = range.1;
                trim_reason = reason(range);
            } else if range.1 >= end {
                kept_end = range.0;
                trim_reason = trim_reason.or_else(|| reason(range));
            } else if range.1 - range.0 >= MIN_CUT_US {
                inner.push(range);
            }
        }
        if (kept_start, kept_end) != (start, end) {
            ops.push(PlanOp::Trim {
                clip: alias.into(),
                in_ms: kept_start / 1000,
                out_ms: kept_end / 1000,
                reason: trim_reason,
            });
        }
        for range in inner {
            ops.push(PlanOp::Cut {
                clip: alias.into(),
                from_ms: (range.0 + 999) / 1000,
                to_ms: range.1 / 1000,
                reason: reason(range),
            });
        }
    }
    if let Some(order) = capture_order(project, aliases) {
        ops.push(PlanOp::Order { clips: order });
    }
    EditPlan {
        version: 1,
        source: PlanSource::Local,
        summary: String::new(),
        ops,
        uncertain: Vec::new(),
    }
}

/// Clips by capture time when every clip has one and they are not in that order already.
fn capture_order(project: &Project, aliases: &Aliases) -> Option<Vec<String>> {
    let clips = &project.timeline.clips;
    let mut timed: Vec<(&str, &str)> = clips
        .iter()
        .map(|clip| {
            let media = project.media.iter().find(|m| m.id == clip.media_id)?;
            Some((media.captured_at.as_deref()?, aliases.clip_alias(clip.id)?))
        })
        .collect::<Option<_>>()?;
    let before: Vec<&str> = timed.iter().map(|(_, alias)| *alias).collect();
    timed.sort_by(|a, b| a.0.cmp(b.0));
    let after: Vec<&str> = timed.iter().map(|(_, alias)| *alias).collect();
    (after != before).then(|| after.into_iter().map(String::from).collect())
}
