//! The edit plan (docs/07 §5): the one contract between Shorted and any planner, local or AI.
//! A plan names clips and segments by short aliases, is validated as a whole, and becomes one
//! `Group` command, so it applies completely or not at all and Undo takes it back in one step.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use shorted_core::command::{Command, TranscriptOwner};
use shorted_core::model::{EmphasisState, Id, MediaKind, Project, TransitionKind};

/// The JSON Schema given to providers that support structured output.
pub const SCHEMA_V1: &str = include_str!("../schema/edit_plan.v1.json");
/// The longest short the app exports (NFR limits).
pub const MAX_SHORT_US: i64 = 120_000_000;
/// AI groups carry this prefix, so the store keeps a version before them (T-0107).
pub const AI_LABEL: &str = "ai:arrange";
pub const LOCAL_LABEL: &str = "arrange";
const MAX_TRANSITION_MS: i64 = 1_500;
const MAX_OPS: usize = 500;
const MAX_TEXT: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanSource {
    Ai,
    Local,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditPlan {
    pub version: u32,
    pub source: PlanSource,
    #[serde(default)]
    pub summary: String,
    pub ops: Vec<PlanOp>,
    #[serde(default)]
    pub uncertain: Vec<Uncertain>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOp {
    Order {
        clips: Vec<String>,
    },
    Trim {
        clip: String,
        in_ms: i64,
        out_ms: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Drop {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Split {
        clip: String,
        at_ms: i64,
    },
    /// Removes a stretch inside a clip, such as a long pause; the parts either side stay.
    Cut {
        clip: String,
        from_ms: i64,
        to_ms: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Transition {
        after: String,
        kind: String,
        duration_ms: i64,
    },
    Emphasis {
        words: Vec<WordRef>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordRef {
    pub segment: String,
    pub word_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uncertain {
    pub clip: String,
    pub reason: String,
}

/// Short names for clips (`c1`…, in timeline order) and transcript segments (`s1`…). A model
/// only ever sees these, never internal ids.
#[derive(Clone, Debug, Default)]
pub struct Aliases {
    clips: HashMap<String, Id>,
    clip_names: HashMap<Id, String>,
    segments: HashMap<String, (Id, Id)>,
    segment_names: HashMap<Id, String>,
}

impl Aliases {
    pub fn of(project: &Project) -> Self {
        let mut aliases = Self::default();
        let mut segment_number = 0;
        for (index, clip) in project.timeline.clips.iter().enumerate() {
            let name = format!("c{}", index + 1);
            aliases.clips.insert(name.clone(), clip.id);
            aliases.clip_names.insert(clip.id, name);
            for segment in clip.transcript.iter().flat_map(|t| &t.segments) {
                segment_number += 1;
                let name = format!("s{segment_number}");
                aliases.segments.insert(name.clone(), (clip.id, segment.id));
                aliases.segment_names.insert(segment.id, name);
            }
        }
        aliases
    }

    pub fn clip(&self, alias: &str) -> Option<Id> {
        self.clips.get(alias).copied()
    }

    pub fn clip_alias(&self, id: Id) -> Option<&str> {
        self.clip_names.get(&id).map(String::as_str)
    }

    /// The clip holding the segment, and the segment.
    pub fn segment(&self, alias: &str) -> Option<(Id, Id)> {
        self.segments.get(alias).copied()
    }

    pub fn segment_alias(&self, id: Id) -> Option<&str> {
        self.segment_names.get(&id).map(String::as_str)
    }
}

/// What the plan does to one clip, for the review screen (FR-ARR-04).
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeKind {
    Trimmed,
    Cut,
    Dropped,
    Split,
    Moved,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanChange {
    pub clip: Id,
    pub kind: ChangeKind,
    pub reason: Option<String>,
}

/// A plan checked against the project: ready to execute as one command.
#[derive(Clone, Debug)]
pub struct ValidPlan {
    pub command: Command,
    pub summary: String,
    pub changes: Vec<PlanChange>,
    /// Clips the planner was unsure about, shown in amber.
    pub uncertain: Vec<(Id, String)>,
    /// Length of the short once applied.
    pub duration_us: i64,
}

/// Reads a plan. Anything that is not exactly a v1 plan is refused with the reason, in words a
/// model can act on.
pub fn parse(json: &str) -> Result<EditPlan, Vec<String>> {
    serde_json::from_str(json).map_err(|error| vec![format!("not a valid edit plan: {error}")])
}

fn transition_kind(name: &str) -> Option<TransitionKind> {
    Some(match name {
        "cut" => TransitionKind::Cut,
        "crossfade" => TransitionKind::Crossfade,
        "dip_to_black" => TransitionKind::DipToBlack,
        "dip_to_white" => TransitionKind::DipToWhite,
        "flash" => TransitionKind::Flash,
        "slide" => TransitionKind::Slide,
        "push" => TransitionKind::Push,
        "whip_pan" => TransitionKind::WhipPan,
        "zoom_in" => TransitionKind::ZoomIn,
        "zoom_out" => TransitionKind::ZoomOut,
        _ => return None,
    })
}

/// Checks `plan` against `project` and turns it into one `Group`. Splits name their new parts
/// with `new_id`, as the core makes no ids. Every problem is reported, so a model can fix them
/// all in one round; on any problem nothing is returned to apply.
pub fn validate(
    plan: &EditPlan,
    project: &Project,
    aliases: &Aliases,
    new_id: &mut dyn FnMut() -> Id,
) -> Result<ValidPlan, Vec<String>> {
    let mut errors = Vec::new();
    if plan.version != 1 {
        errors.push(format!("version must be 1, not {}", plan.version));
    }
    if plan.ops.len() > MAX_OPS {
        errors.push(format!("at most {MAX_OPS} ops"));
    }
    if plan.summary.len() > MAX_TEXT {
        errors.push("the summary is too long".into());
    }
    let clips = &project.timeline.clips;
    let clip_of = |alias: &str, errors: &mut Vec<String>| {
        let found = aliases
            .clip(alias)
            .and_then(|id| clips.iter().find(|clip| clip.id == id));
        if found.is_none() {
            errors.push(format!("unknown clip \"{alias}\""));
        }
        found
    };

    let mut trims: HashMap<Id, (i64, i64)> = HashMap::new();
    let mut drops: HashSet<Id> = HashSet::new();
    let mut splits: HashMap<Id, Vec<i64>> = HashMap::new();
    let mut cuts: HashMap<Id, Vec<(i64, i64)>> = HashMap::new();
    let mut order: Option<Vec<Id>> = None;
    let mut changes = Vec::new();

    // Trims and drops first: splits and transitions are checked against their result.
    for op in &plan.ops {
        match op {
            PlanOp::Trim {
                clip,
                in_ms,
                out_ms,
                reason,
            } => {
                let Some(item) = clip_of(clip, &mut errors) else {
                    continue;
                };
                let (from, to) = (in_ms.saturating_mul(1000), out_ms.saturating_mul(1000));
                let media = project.media.iter().find(|m| m.id == item.media_id);
                let length = media
                    .filter(|m| matches!(m.kind, MediaKind::Video | MediaKind::Audio))
                    .map(|m| m.probe.duration_us);
                if from < 0 || from >= to {
                    errors.push(format!("trim of {clip}: in_ms must be below out_ms"));
                } else if length.is_some_and(|length| to > length) {
                    errors.push(format!(
                        "trim of {clip}: out_ms is past the end of its source ({} ms)",
                        length.unwrap_or_default() / 1000
                    ));
                } else if trims.insert(item.id, (from, to)).is_some() {
                    errors.push(format!("{clip} is trimmed twice"));
                } else {
                    changes.push(PlanChange {
                        clip: item.id,
                        kind: ChangeKind::Trimmed,
                        reason: reason.clone(),
                    });
                }
            }
            PlanOp::Drop { clip, reason } => {
                let Some(item) = clip_of(clip, &mut errors) else {
                    continue;
                };
                if !drops.insert(item.id) {
                    errors.push(format!("{clip} is dropped twice"));
                } else {
                    changes.push(PlanChange {
                        clip: item.id,
                        kind: ChangeKind::Dropped,
                        reason: reason.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    for op in &plan.ops {
        match op {
            PlanOp::Split { clip, at_ms } => {
                let Some(item) = clip_of(clip, &mut errors) else {
                    continue;
                };
                let (start, end) = trims
                    .get(&item.id)
                    .copied()
                    .unwrap_or((item.source_in_us, item.source_out_us));
                let at = at_ms.saturating_mul(1000);
                if drops.contains(&item.id) {
                    errors.push(format!("{clip} is both dropped and split"));
                } else if at <= start || at >= end {
                    errors.push(format!(
                        "split of {clip}: at_ms must be between {} and {} ms",
                        start / 1000,
                        end / 1000
                    ));
                } else {
                    let points = splits.entry(item.id).or_default();
                    if points.contains(&at) {
                        errors.push(format!("{clip} is split twice at {at_ms} ms"));
                    } else {
                        points.push(at);
                        changes.push(PlanChange {
                            clip: item.id,
                            kind: ChangeKind::Split,
                            reason: None,
                        });
                    }
                }
            }
            PlanOp::Cut {
                clip,
                from_ms,
                to_ms,
                reason,
            } => {
                let Some(item) = clip_of(clip, &mut errors) else {
                    continue;
                };
                let (start, end) = trims
                    .get(&item.id)
                    .copied()
                    .unwrap_or((item.source_in_us, item.source_out_us));
                let (from, to) = (from_ms.saturating_mul(1000), to_ms.saturating_mul(1000));
                let taken = cuts
                    .get(&item.id)
                    .is_some_and(|ranges| ranges.iter().any(|(a, b)| from < *b && *a < to));
                if drops.contains(&item.id) {
                    errors.push(format!("{clip} is both dropped and cut"));
                } else if from >= to || from < start || to > end {
                    errors.push(format!(
                        "cut of {clip}: from_ms and to_ms must lie between {} and {} ms",
                        start / 1000,
                        end / 1000
                    ));
                } else if from == start && to == end {
                    errors.push(format!("cut of {clip} takes all of it: drop it instead"));
                } else if taken {
                    errors.push(format!("cuts of {clip} overlap"));
                } else {
                    cuts.entry(item.id).or_default().push((from, to));
                    changes.push(PlanChange {
                        clip: item.id,
                        kind: ChangeKind::Cut,
                        reason: reason.clone(),
                    });
                }
            }
            PlanOp::Order { clips: listed } => {
                if order.is_some() {
                    errors.push("only one order op is allowed".into());
                    continue;
                }
                let mut seen = HashSet::new();
                let mut ids = Vec::new();
                for alias in listed {
                    let Some(item) = clip_of(alias, &mut errors) else {
                        continue;
                    };
                    if !seen.insert(item.id) {
                        errors.push(format!("{alias} appears twice in order"));
                    } else if !drops.contains(&item.id) {
                        ids.push(item.id);
                    }
                }
                order = Some(ids);
            }
            PlanOp::Trim { .. } | PlanOp::Drop { .. } => {}
            PlanOp::Transition { .. } | PlanOp::Emphasis { .. } => {}
        }
    }
    if drops.len() == clips.len() && !clips.is_empty() {
        errors.push("a plan cannot drop every clip".into());
    }

    // The arrangement the plan asks for: listed clips first, the others after, in their order.
    let mut arranged: Vec<Id> = order.clone().unwrap_or_default();
    for clip in clips {
        if !drops.contains(&clip.id) && !arranged.contains(&clip.id) {
            arranged.push(clip.id);
        }
    }

    let mut commands = Vec::new();
    for (id, (from, to)) in &trims {
        commands.push(Command::TrimClip {
            id: *id,
            source_in_us: *from,
            source_out_us: *to,
        });
    }
    // Each clip becomes its parts, left to right; the right part keeps splitting. Cuts are two
    // splits whose middle part goes.
    let mut parts: HashMap<Id, Vec<Id>> = HashMap::new();
    for clip in clips {
        let (start, end) = trims
            .get(&clip.id)
            .copied()
            .unwrap_or((clip.source_in_us, clip.source_out_us));
        let removed = cuts.get(&clip.id).cloned().unwrap_or_default();
        let mut points = splits.get(&clip.id).cloned().unwrap_or_default();
        points.extend(removed.iter().flat_map(|(from, to)| [*from, *to]));
        points.retain(|at| *at > start && *at < end);
        points.sort_unstable();
        points.dedup();
        let mut pieces = vec![(clip.id, start)];
        for at in points {
            let right = new_id();
            commands.push(Command::SplitClip {
                id: pieces.last().map_or(clip.id, |(id, _)| *id),
                at_source_us: at,
                new_id: right,
            });
            pieces.push((right, at));
        }
        let mut kept = Vec::new();
        for (index, (id, from)) in pieces.iter().enumerate() {
            let to = pieces.get(index + 1).map_or(end, |(_, at)| *at);
            if removed.iter().any(|(a, b)| *from >= *a && to <= *b) {
                commands.push(Command::DeleteClip { id: *id });
            } else {
                kept.push(*id);
            }
        }
        parts.insert(clip.id, kept);
    }
    for id in &drops {
        commands.push(Command::DeleteClip { id: *id });
    }
    let final_order: Vec<Id> = arranged
        .iter()
        .flat_map(|id| parts.get(id).cloned().unwrap_or_default())
        .collect();
    if order.is_some() {
        let before: Vec<Id> = clips.iter().map(|clip| clip.id).collect();
        for (position, id) in arranged.iter().enumerate() {
            if before.iter().filter(|c| arranged.contains(c)).nth(position) != Some(id) {
                changes.push(PlanChange {
                    clip: *id,
                    kind: ChangeKind::Moved,
                    reason: None,
                });
            }
        }
        for (to, id) in final_order.iter().enumerate() {
            commands.push(Command::MoveClip { id: *id, to });
        }
    }

    for op in &plan.ops {
        match op {
            PlanOp::Transition {
                after,
                kind,
                duration_ms,
            } => {
                let Some(item) = clip_of(after, &mut errors) else {
                    continue;
                };
                let Some(kind) = transition_kind(kind) else {
                    errors.push(format!("unknown transition \"{kind}\""));
                    continue;
                };
                if !(0..=MAX_TRANSITION_MS).contains(duration_ms) {
                    errors.push(format!(
                        "transition after {after}: duration_ms must be 0 to {MAX_TRANSITION_MS}"
                    ));
                    continue;
                }
                let left = parts.get(&item.id).and_then(|p| p.last()).copied();
                let next = left
                    .and_then(|left| final_order.iter().position(|id| *id == left))
                    .and_then(|index| final_order.get(index + 1));
                match (left, next) {
                    (Some(left), Some(right)) if !drops.contains(&item.id) => {
                        commands.push(Command::SetTransition {
                            left,
                            right: *right,
                            kind,
                            duration_us: duration_ms * 1000,
                        });
                    }
                    _ => errors.push(format!("no clip follows {after} for a transition")),
                }
            }
            PlanOp::Emphasis { words } => {
                for word in words {
                    let Some((clip, segment)) = aliases.segment(&word.segment) else {
                        errors.push(format!("unknown segment \"{}\"", word.segment));
                        continue;
                    };
                    let found = clips
                        .iter()
                        .find(|c| c.id == clip)
                        .and_then(|c| c.transcript.as_ref())
                        .and_then(|t| t.segments.iter().find(|s| s.id == segment))
                        .and_then(|s| s.words.get(word.word_index));
                    match found {
                        None => {
                            errors.push(format!("{} has no word {}", word.segment, word.word_index))
                        }
                        Some(found) => {
                            let (EmphasisState::Auto(on) | EmphasisState::User(on)) =
                                found.emphasis;
                            if !on && !drops.contains(&clip) {
                                commands.push(Command::ToggleEmphasis {
                                    owner: TranscriptOwner::Clip(clip),
                                    segment,
                                    word: word.word_index,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let uncertain = plan
        .uncertain
        .iter()
        .filter_map(|item| {
            let id = aliases.clip(&item.clip);
            if id.is_none() {
                errors.push(format!("unknown clip \"{}\" in uncertain", item.clip));
            }
            Some((id?, item.reason.clone()))
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }

    let command = Command::Group {
        label: match plan.source {
            PlanSource::Ai => AI_LABEL,
            PlanSource::Local => LOCAL_LABEL,
        }
        .into(),
        commands,
    };
    // The whole plan runs on a copy: a project the plan breaks is refused, never half changed.
    let mut result = project.clone();
    if let Err(error) = command.apply(&mut result) {
        return Err(vec![format!("the plan does not apply: {error}")]);
    }
    let duration_us = result.duration_us();
    if duration_us > MAX_SHORT_US {
        return Err(vec![format!(
            "the result lasts {} s, more than the {} s a short may last",
            duration_us / 1_000_000,
            MAX_SHORT_US / 1_000_000
        )]);
    }
    Ok(ValidPlan {
        command,
        summary: plan.summary.clone(),
        changes,
        uncertain,
        duration_us,
    })
}
