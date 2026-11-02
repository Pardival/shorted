//! Overlays on the Widgets track (T-0701): timing, layer order, lock, hide, duplicate, delete,
//! and where each one sits on screen so Flutter can draw the selection around it.

use shorted_core::command::Command;
use shorted_core::evaluate::evaluate;
use shorted_core::model::*;
use shorted_render::overlays::{shape_size, CANVAS_HEIGHT, CANVAS_WIDTH};
use uuid::Uuid;

use crate::api::clips::{edit, parse};
use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};
use crate::transport::{publish_overlay, transport};

/// A new element lasts this long from the playhead.
const DEFAULT_LENGTH_US: i64 = 3_000_000;
/// A duplicate sits this far down and right, so both stay visible.
const DUPLICATE_OFFSET: f32 = 40.0;
/// The shortest an element can be.
const MIN_LENGTH_US: i64 = 100_000;
const DEFAULT_FILL: &str = "#9A8CFFFF";
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 20.0;

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
    pub transform: TransformView,
}

/// An element's own placement, before animation: centre in canvas units, degrees clockwise.
#[derive(Clone, Copy)]
pub struct TransformView {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub flip: bool,
}

impl From<Transform> for TransformView {
    fn from(base: Transform) -> Self {
        Self {
            x: base.position.x,
            y: base.position.y,
            scale: base.scale,
            rotation: base.rotation,
            opacity: base.opacity,
            flip: base.flip,
        }
    }
}

impl From<TransformView> for Transform {
    fn from(view: TransformView) -> Self {
        Self {
            position: Vec2 {
                x: view.x,
                y: view.y,
            },
            scale: view.scale.clamp(MIN_SCALE, MAX_SCALE),
            rotation: view.rotation.rem_euclid(360.0),
            opacity: view.opacity.clamp(0.0, 1.0),
            flip: view.flip,
        }
    }
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
            transform: item.base.into(),
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

/// Shows `transform` while a finger moves the element; nothing is recorded yet.
#[flutter_rust_bridge::frb(sync)]
pub fn preview_overlay_transform(id: String, transform: TransformView) {
    if let Ok(id) = parse(&id) {
        publish_overlay(id, transform.into());
    }
}

/// Records where a gesture left the element, as one undo step.
pub async fn set_overlay_transform(
    id: String,
    transform: TransformView,
) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let id = parse(&id)?;
        let base: Transform = transform.into();
        with_active(|session| {
            let saved = find(session.project(), id).ok().map(|item| item.base);
            if saved.is_some_and(|saved| saved != base) {
                session.execute(&Command::UpdateOverlayTransform { id, base })?;
            }
            Ok(view_of(session))
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

    #[test]
    fn a_transform_from_a_gesture_stays_valid() {
        let base: Transform = TransformView {
            x: 10.0,
            y: 20.0,
            scale: 0.0,
            rotation: -90.0,
            opacity: 1.5,
            flip: true,
        }
        .into();
        assert_eq!(base.scale, MIN_SCALE);
        assert_eq!(base.rotation, 270.0);
        assert_eq!(base.opacity, 1.0);
        assert_eq!(TransformView::from(base).x, 10.0);
    }
}
