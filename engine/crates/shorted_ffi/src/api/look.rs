//! The Enhance section: speed, colour and transitions.

use shorted_core::command::Command;
use shorted_core::model::*;

use crate::api::clips::{edit, parse};
use crate::api::projects::ProjectView;
use crate::error::{EngineError, EngineFailure};

const DEFAULT_TRANSITION_US: i64 = 500_000;

/// The transitions the renderer draws.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransitionKindView {
    Cut,
    Crossfade,
    DipToBlack,
    DipToWhite,
    Flash,
    Slide,
    Push,
    WhipPan,
    ZoomIn,
    ZoomOut,
}

impl From<TransitionKindView> for TransitionKind {
    fn from(kind: TransitionKindView) -> Self {
        match kind {
            TransitionKindView::Cut => Self::Cut,
            TransitionKindView::Crossfade => Self::Crossfade,
            TransitionKindView::DipToBlack => Self::DipToBlack,
            TransitionKindView::DipToWhite => Self::DipToWhite,
            TransitionKindView::Flash => Self::Flash,
            TransitionKindView::Slide => Self::Slide,
            TransitionKindView::Push => Self::Push,
            TransitionKindView::WhipPan => Self::WhipPan,
            TransitionKindView::ZoomIn => Self::ZoomIn,
            TransitionKindView::ZoomOut => Self::ZoomOut,
        }
    }
}

fn kind_view(kind: TransitionKind) -> Option<TransitionKindView> {
    Some(match kind {
        TransitionKind::Cut => TransitionKindView::Cut,
        TransitionKind::Crossfade => TransitionKindView::Crossfade,
        TransitionKind::DipToBlack => TransitionKindView::DipToBlack,
        TransitionKind::DipToWhite => TransitionKindView::DipToWhite,
        TransitionKind::Flash => TransitionKindView::Flash,
        TransitionKind::Slide => TransitionKindView::Slide,
        TransitionKind::Push => TransitionKindView::Push,
        TransitionKind::WhipPan => TransitionKindView::WhipPan,
        TransitionKind::ZoomIn => TransitionKindView::ZoomIn,
        TransitionKind::ZoomOut => TransitionKindView::ZoomOut,
        _ => return None,
    })
}

pub struct ClipLookView {
    pub clip_id: String,
    pub speed: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
}

pub struct LookView {
    pub clips: Vec<ClipLookView>,
    /// The kind used on every cut, if they all share one.
    pub transition: Option<TransitionKindView>,
}

fn shared_transition(timeline: &Timeline) -> Option<TransitionKindView> {
    let cuts = timeline.clips.len().saturating_sub(1);
    let Some(first) = timeline.transitions.first() else {
        return Some(TransitionKindView::Cut);
    };
    let shared = timeline.transitions.len() == cuts
        && timeline.transitions.iter().all(|t| t.kind == first.kind);
    if shared {
        kind_view(first.kind)
    } else {
        None
    }
}

pub(crate) fn look_view(project: &Project) -> LookView {
    let timeline = &project.timeline;
    LookView {
        clips: timeline
            .clips
            .iter()
            .map(|clip| ClipLookView {
                clip_id: clip.id.to_string(),
                speed: clip.speed,
                brightness: clip.color.brightness,
                contrast: clip.color.contrast,
                saturation: clip.color.saturation,
            })
            .collect(),
        transition: shared_transition(timeline),
    }
}

pub async fn set_clip_speed(id: String, speed: f32) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::SetSpeed {
            id: parse(&id)?,
            speed,
        })
    })
}

pub async fn set_clip_color(
    id: String,
    brightness: f32,
    contrast: f32,
    saturation: f32,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::SetColor {
            id: parse(&id)?,
            color: ColorAdjust {
                brightness,
                contrast,
                saturation,
            },
        })
    })
}

/// Puts one transition on every cut.
pub async fn set_transition_for_all(
    kind: TransitionKindView,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        if project.timeline.clips.is_empty() {
            return Err(EngineError::Internal("no clips".into()));
        }
        Ok(Command::SetTransitionForAll {
            kind: kind.into(),
            duration_us: DEFAULT_TRANSITION_US,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        Project::from_json(json).unwrap()
    }

    #[test]
    fn no_transition_reads_as_cuts() {
        let mut project = project();
        project.timeline.transitions.clear();
        assert!(shared_transition(&project.timeline) == Some(TransitionKindView::Cut));
    }

    #[test]
    fn a_shared_kind_is_reported_and_a_mix_is_not() {
        let mut project = project();
        let timeline = &mut project.timeline;
        timeline.transitions = timeline
            .clips
            .windows(2)
            .map(|pair| Transition {
                left: pair[0].id,
                right: pair[1].id,
                kind: TransitionKind::Push,
                duration_us: 500_000,
            })
            .collect();
        assert!(shared_transition(timeline) == Some(TransitionKindView::Push));
        if timeline.transitions.len() > 1 {
            timeline.transitions[0].kind = TransitionKind::Crossfade;
            assert!(shared_transition(timeline).is_none());
        }
    }
}
