//! Overlays on the Widgets track (T-0701): timing, layer order, lock, hide, duplicate, delete,
//! and where each one sits on screen so Flutter can draw the selection around it.

use shorted_core::command::Command;
use shorted_core::evaluate::evaluate;
use shorted_core::model::*;
use shorted_render::overlays::{shape_size, CANVAS_HEIGHT, CANVAS_WIDTH};
use uuid::Uuid;

use crate::api::clips::{edit, parse};
use crate::api::projects::ProjectView;
use crate::error::{EngineError, EngineFailure};
use crate::transport::transport;

/// A new element lasts this long from the playhead.
const DEFAULT_LENGTH_US: i64 = 3_000_000;
/// A duplicate sits this far down and right, so both stay visible.
const DUPLICATE_OFFSET: f32 = 40.0;
/// The shortest an element can be.
const MIN_LENGTH_US: i64 = 100_000;
const DEFAULT_FILL: &str = "#9A8CFFFF";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OverlayKindView {
    Image,
    AnimatedImage,
    Video,
    Text,
    Shape,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShapeKindView {
    Rectangle,
    Ellipse,
    Arrow,
}

impl From<ShapeKindView> for ShapeKind {
    fn from(shape: ShapeKindView) -> Self {
        match shape {
            ShapeKindView::Rectangle => Self::Rectangle,
            ShapeKindView::Ellipse => Self::Ellipse,
            ShapeKindView::Arrow => Self::Arrow,
        }
    }
}

/// One element, bottom first in `ProjectView::overlays`.
pub struct OverlayView {
    pub id: String,
    pub kind: OverlayKindView,
    pub start_us: i64,
    pub end_us: i64,
    pub locked: bool,
    pub hidden: bool,
    pub has_animation: bool,
}

/// Where an element is drawn at one instant, in canvas units (1080 × 1920, origin top left).
pub struct OverlayBoxView {
    pub id: String,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    /// Degrees, clockwise.
    pub rotation: f32,
}

/// The size of an element at scale 1, in canvas units.
pub(crate) fn natural_size(kind: &OverlayKind) -> (f32, f32) {
    match kind {
        OverlayKind::Shape { shape, .. } => shape_size(*shape),
        // Measured from their content by the tickets that draw them.
        _ => (400.0, 400.0),
    }
}

pub(crate) fn overlay_views(project: &Project) -> Vec<OverlayView> {
    project
        .timeline
        .overlays
        .iter()
        .map(|item| OverlayView {
            id: item.id.to_string(),
            kind: match item.kind {
                OverlayKind::Image { .. } => OverlayKindView::Image,
                OverlayKind::AnimatedImage { .. } => OverlayKindView::AnimatedImage,
                OverlayKind::Video { .. } => OverlayKindView::Video,
                OverlayKind::Text { .. } => OverlayKindView::Text,
                OverlayKind::Shape { .. } => OverlayKindView::Shape,
            },
            start_us: item.start_us,
            end_us: item.end_us,
            locked: item.locked,
            hidden: item.hidden,
            has_animation: item.anim_in.is_some()
                || item.anim_out.is_some()
                || item.anim_loop.is_some()
                || !item.keyframes.is_empty()
                || item.path.is_some(),
        })
        .collect()
}

/// The elements on screen at `time_us`, as the renderer places them, top first.
#[flutter_rust_bridge::frb(sync)]
pub fn overlay_boxes(time_us: i64) -> Vec<OverlayBoxView> {
    let Some(project) = transport().project.clone() else {
        return Vec::new();
    };
    boxes(&project, time_us)
}

fn boxes(project: &Project, time_us: i64) -> Vec<OverlayBoxView> {
    let frame = evaluate(project, time_us);
    frame
        .overlays
        .iter()
        .rev()
        .filter_map(|layer| {
            let item = project
                .timeline
                .overlays
                .iter()
                .find(|item| item.id == layer.overlay_id)?;
            let (width, height) = natural_size(&item.kind);
            Some(OverlayBoxView {
                id: item.id.to_string(),
                center_x: layer.position.x,
                center_y: layer.position.y,
                width: width * layer.scale,
                height: height * layer.scale,
                rotation: layer.rotation,
            })
        })
        .collect()
}

fn find(project: &Project, id: Id) -> Result<&OverlayItem, EngineError> {
    project
        .timeline
        .overlays
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| EngineError::Internal("unknown overlay".into()))
}

/// A new element in the middle of the frame, from `at_us` for a few seconds.
pub(crate) fn new_overlay(kind: OverlayKind, at_us: i64, project: &Project) -> OverlayItem {
    let start_us = at_us.clamp(0, (project.duration_us() - MIN_LENGTH_US).max(0));
    OverlayItem {
        id: Uuid::now_v7(),
        kind,
        start_us,
        end_us: start_us + DEFAULT_LENGTH_US,
        base: Transform {
            position: Vec2 {
                x: CANVAS_WIDTH / 2.0,
                y: CANVAS_HEIGHT / 2.0,
            },
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            flip: false,
        },
        keyframes: Vec::new(),
        path: None,
        anim_in: None,
        anim_loop: None,
        anim_out: None,
        locked: false,
        hidden: false,
    }
}

pub async fn add_shape(shape: ShapeKindView, at_us: i64) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let kind = OverlayKind::Shape {
            shape: shape.into(),
            fill: DEFAULT_FILL.into(),
        };
        Ok(Command::AddOverlay {
            overlay: Box::new(new_overlay(kind, at_us, project)),
        })
    })
}

/// Moves an element in time, or one of its edges. Times are timeline microseconds.
pub async fn set_overlay_timing(
    id: String,
    start_us: i64,
    end_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let start_us = start_us.max(0);
        let end_us = end_us.max(start_us + MIN_LENGTH_US);
        Ok(Command::SetOverlayTiming {
            id: item.id,
            start_us,
            end_us,
        })
    })
}

/// Up (towards the viewer) when `forward`, one step.
pub async fn shift_overlay_layer(id: String, forward: bool) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let overlays = &project.timeline.overlays;
        let index = overlays
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| EngineError::Internal("unknown overlay".into()))?;
        let to = if forward {
            (index + 1).min(overlays.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        Ok(Command::ReorderOverlay { id, to })
    })
}

pub async fn set_overlay_flags(
    id: String,
    locked: bool,
    hidden: bool,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let mut item = find(project, parse(&id)?)?.clone();
        item.locked = locked;
        item.hidden = hidden;
        Ok(Command::ReplaceOverlay {
            overlay: Box::new(item),
        })
    })
}

/// A copy just above the original, a little down and to the right.
pub async fn duplicate_overlay(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let mut copy = find(project, id)?.clone();
        copy.id = Uuid::now_v7();
        copy.locked = false;
        copy.base.position.x += DUPLICATE_OFFSET;
        copy.base.position.y += DUPLICATE_OFFSET;
        let index = project
            .timeline
            .overlays
            .iter()
            .position(|item| item.id == id)
            .unwrap_or_default();
        let copy_id = copy.id;
        Ok(Command::Group {
            label: "duplicate overlay".into(),
            commands: vec![
                Command::AddOverlay {
                    overlay: Box::new(copy),
                },
                Command::ReorderOverlay {
                    id: copy_id,
                    to: index + 1,
                },
            ],
        })
    })
}

pub async fn delete_overlay(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        Ok(Command::DeleteOverlay { id: item.id })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(kind: OverlayKind) -> (Project, Id) {
        let mut project = Project::new(Uuid::nil(), "t".into(), DateTime::default());
        let mut item = new_overlay(kind, 0, &project);
        item.base.scale = 2.0;
        item.base.rotation = 30.0;
        let id = item.id;
        project.timeline.overlays.push(item);
        (project, id)
    }

    #[test]
    fn a_box_is_the_shape_at_its_scale_and_turn() {
        let (project, id) = project_with(OverlayKind::Shape {
            shape: ShapeKind::Arrow,
            fill: DEFAULT_FILL.into(),
        });
        let found = boxes(&project, 1_000_000);
        assert_eq!(found.len(), 1);
        let shown = &found[0];
        assert_eq!(shown.id, id.to_string());
        assert_eq!((shown.width, shown.height), (800.0, 400.0));
        assert_eq!((shown.center_x, shown.center_y), (540.0, 960.0));
        assert_eq!(shown.rotation, 30.0);
    }

    #[test]
    fn hidden_elements_have_no_box() {
        let (mut project, _) = project_with(OverlayKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: DEFAULT_FILL.into(),
        });
        project.timeline.overlays[0].hidden = true;
        assert!(boxes(&project, 0).is_empty());
    }

    #[test]
    fn a_new_element_starts_at_the_playhead_in_the_middle() {
        let project = Project::new(Uuid::nil(), "t".into(), DateTime::default());
        let item = new_overlay(
            OverlayKind::Shape {
                shape: ShapeKind::Ellipse,
                fill: DEFAULT_FILL.into(),
            },
            0,
            &project,
        );
        assert_eq!(item.end_us - item.start_us, DEFAULT_LENGTH_US);
        assert_eq!(item.base.position.x, 540.0);
    }
}
