//! Overlays on the Widgets track (T-0701): timing, layer order, lock, hide, duplicate, delete,
//! and where each one sits on screen so Flutter can draw the selection around it.

use shorted_core::command::{AnimSlot, Command};
use shorted_core::evaluate::{
    evaluate, keyframe_times, keys_for, path_position_at, preset_modifier, property_value,
    simplify_path,
};
use shorted_core::model::*;
use shorted_render::captions::parse_color;
use shorted_render::overlays::{shape_size, CANVAS_HEIGHT, CANVAS_WIDTH};
use shorted_render::text::{text_size, Decoration, TextLook};
use uuid::Uuid;

use crate::api::clips::{edit, parse};
use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};
use crate::transport::{publish_overlay, publish_overlay_keys, transport};

/// A new element lasts this long from the playhead.
const DEFAULT_LENGTH_US: i64 = 3_000_000;
/// A duplicate sits this far down and right, so both stay visible.
const DUPLICATE_OFFSET: f32 = 40.0;
/// The shortest an element can be.
const MIN_LENGTH_US: i64 = 100_000;
const DEFAULT_FILL: &str = "#9A8CFFFF";
const MIN_SCALE: f32 = 0.05;
const PLACEHOLDER_SIZE: (f32, f32) = (400.0, 400.0);
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationView {
    Plain,
    Chip,
    Outline,
    Shadow,
}

/// The words of a text element and how they look. Sizes are canvas units.
#[derive(Clone)]
pub struct TextElementView {
    pub content: String,
    pub font: String,
    pub size: f32,
    /// `#RRGGBBAA`.
    pub color: String,
    pub uppercase: bool,
    pub decoration: TextDecorationView,
}

pub(crate) fn text_look(style: &TextStyle) -> TextLook {
    TextLook {
        font: style.font.clone(),
        size: style.size,
        color: parse_color(&style.color),
        uppercase: style.uppercase,
        decoration: match style.decoration {
            TextDecoration::Plain => Decoration::Plain,
            TextDecoration::Chip => Decoration::Chip,
            TextDecoration::Outline => Decoration::Outline,
            TextDecoration::Shadow => Decoration::Shadow,
        },
    }
}

fn text_view(content: &str, style: &TextStyle) -> TextElementView {
    TextElementView {
        content: content.into(),
        font: style.font.clone(),
        size: style.size,
        color: style.color.clone(),
        uppercase: style.uppercase,
        decoration: match style.decoration {
            TextDecoration::Plain => TextDecorationView::Plain,
            TextDecoration::Chip => TextDecorationView::Chip,
            TextDecoration::Outline => TextDecorationView::Outline,
            TextDecoration::Shadow => TextDecorationView::Shadow,
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AnimSlotView {
    In,
    Loop,
    Out,
}

impl From<AnimSlotView> for AnimSlot {
    fn from(slot: AnimSlotView) -> Self {
        match slot {
            AnimSlotView::In => Self::In,
            AnimSlotView::Loop => Self::Loop,
            AnimSlotView::Out => Self::Out,
        }
    }
}

/// The presets of FR-ENH-20.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PresetKindView {
    Pop,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Fade,
    Zoom,
    Stamp,
    Wipe,
    Shrink,
    Float,
    Pulse,
    Wiggle,
    Rotate,
    Bounce,
}

impl From<PresetKindView> for PresetKind {
    fn from(kind: PresetKindView) -> Self {
        match kind {
            PresetKindView::Pop => Self::Pop,
            PresetKindView::SlideLeft => Self::SlideLeft,
            PresetKindView::SlideRight => Self::SlideRight,
            PresetKindView::SlideUp => Self::SlideUp,
            PresetKindView::SlideDown => Self::SlideDown,
            PresetKindView::Fade => Self::Fade,
            PresetKindView::Zoom => Self::Zoom,
            PresetKindView::Stamp => Self::Stamp,
            PresetKindView::Wipe => Self::Wipe,
            PresetKindView::Shrink => Self::Shrink,
            PresetKindView::Float => Self::Float,
            PresetKindView::Pulse => Self::Pulse,
            PresetKindView::Wiggle => Self::Wiggle,
            PresetKindView::Rotate => Self::Rotate,
            PresetKindView::Bounce => Self::Bounce,
        }
    }
}

impl From<PresetKind> for PresetKindView {
    fn from(kind: PresetKind) -> Self {
        match kind {
            PresetKind::Pop => Self::Pop,
            PresetKind::SlideLeft => Self::SlideLeft,
            PresetKind::SlideRight => Self::SlideRight,
            PresetKind::SlideUp => Self::SlideUp,
            PresetKind::SlideDown => Self::SlideDown,
            PresetKind::Fade => Self::Fade,
            PresetKind::Zoom => Self::Zoom,
            PresetKind::Stamp => Self::Stamp,
            PresetKind::Wipe => Self::Wipe,
            PresetKind::Shrink => Self::Shrink,
            PresetKind::Float => Self::Float,
            PresetKind::Pulse => Self::Pulse,
            PresetKind::Wiggle => Self::Wiggle,
            PresetKind::Rotate => Self::Rotate,
            PresetKind::Bounce => Self::Bounce,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PresetView {
    pub kind: PresetKindView,
    pub duration_us: i64,
}

fn preset_view(anim: Option<PresetAnim>) -> Option<PresetView> {
    anim.map(|anim| PresetView {
        kind: anim.kind.into(),
        duration_us: anim.duration_us,
    })
}

/// What a preset does at one instant, relative to the element: for the animated thumbnails of
/// the Animate sheet, so they move exactly as the video will.
pub struct PresetSampleView {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub reveal: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EasingView {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Spring,
}

impl From<EasingView> for Easing {
    fn from(easing: EasingView) -> Self {
        match easing {
            EasingView::Linear => Self::Linear,
            EasingView::EaseIn => Self::EaseIn,
            EasingView::EaseOut => Self::EaseOut,
            EasingView::EaseInOut => Self::EaseInOut,
            EasingView::Spring => Self::Spring,
        }
    }
}

impl From<Easing> for EasingView {
    fn from(easing: Easing) -> Self {
        match easing {
            Easing::Linear => Self::Linear,
            Easing::EaseIn => Self::EaseIn,
            Easing::EaseOut => Self::EaseOut,
            Easing::EaseInOut => Self::EaseInOut,
            Easing::Spring => Self::Spring,
        }
    }
}

/// A time holding keys, on the timeline: absolute microseconds.
#[derive(Clone, Copy)]
pub struct KeyframeView {
    pub time_us: i64,
    pub easing: EasingView,
}

/// A point of a recorded path: timeline time, canvas units.
#[derive(Clone, Copy)]
pub struct PathPointView {
    pub time_us: i64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
pub struct PathView {
    pub points: Vec<PathPointView>,
    pub smoothing: f32,
    pub orient_to_path: bool,
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
    /// Set for text elements.
    pub text: Option<TextElementView>,
    pub anim_in: Option<PresetView>,
    pub anim_loop: Option<PresetView>,
    pub anim_out: Option<PresetView>,
    pub keyframes: Vec<KeyframeView>,
    pub path: Option<PathView>,
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

/// The size of an element at scale 1, in canvas units: pictures show one pixel per unit.
pub(crate) fn natural_size(kind: &OverlayKind, project: &Project) -> (f32, f32) {
    let picture = |id: &Id| {
        let media = project.media.iter().find(|media| media.id == *id)?;
        Some((media.probe.width as f32, media.probe.height as f32))
    };
    match kind {
        OverlayKind::Shape { shape, .. } => shape_size(*shape),
        OverlayKind::Image { asset } | OverlayKind::AnimatedImage { asset } => {
            picture(asset).unwrap_or(PLACEHOLDER_SIZE)
        }
        OverlayKind::Video { media, .. } => picture(media).unwrap_or(PLACEHOLDER_SIZE),
        OverlayKind::Text { content, style } => {
            let (width, height) = text_size(content, &text_look(style));
            (width as f32, height as f32)
        }
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
            text: match &item.kind {
                OverlayKind::Text { content, style } => Some(text_view(content, style)),
                _ => None,
            },
            anim_in: preset_view(item.anim_in),
            anim_loop: preset_view(item.anim_loop),
            anim_out: preset_view(item.anim_out),
            keyframes: keyframe_times(item)
                .into_iter()
                .map(|(local, easing)| KeyframeView {
                    time_us: item.start_us + local,
                    easing: easing.into(),
                })
                .collect(),
            path: item.path.as_ref().map(|path| PathView {
                points: path
                    .samples
                    .iter()
                    .map(|sample| PathPointView {
                        time_us: item.start_us + sample.t_us,
                        x: sample.x,
                        y: sample.y,
                    })
                    .collect(),
                smoothing: path.smoothing,
                orient_to_path: path.orient_to_path,
            }),
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
            let (width, height) = natural_size(&item.kind, project);
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

const MAX_TEXT_SIZE: f32 = 600.0;
const MIN_TEXT_SIZE: f32 = 12.0;

/// A text element in the caption style, from the playhead.
pub async fn add_text(content: String, at_us: i64) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let kind = OverlayKind::Text {
            content,
            style: TextStyle::default(),
        };
        Ok(Command::AddOverlay {
            overlay: Box::new(new_overlay(kind, at_us, project)),
        })
    })
}

/// New words or look for a text element.
pub async fn set_text(id: String, text: TextElementView) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let mut item = find(project, parse(&id)?)?.clone();
        if !matches!(item.kind, OverlayKind::Text { .. }) {
            return Err(EngineError::Internal("not a text element".into()));
        }
        item.kind = OverlayKind::Text {
            content: text.content,
            style: TextStyle {
                font: if text.font == "Inter" {
                    "Inter".into()
                } else {
                    "Space Grotesk".into()
                },
                size: text.size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE),
                color: text.color,
                uppercase: text.uppercase,
                decoration: match text.decoration {
                    TextDecorationView::Plain => TextDecoration::Plain,
                    TextDecorationView::Chip => TextDecoration::Chip,
                    TextDecorationView::Outline => TextDecoration::Outline,
                    TextDecorationView::Shadow => TextDecoration::Shadow,
                },
            },
        };
        Ok(Command::ReplaceOverlay {
            overlay: Box::new(item),
        })
    })
}

const MIN_PRESET_US: i64 = 100_000;
const MAX_PRESET_US: i64 = 10_000_000;

/// Sets or clears the In, Loop or Out animation. In and Out last at most half the element each,
/// so both fit.
pub async fn set_preset_anim(
    id: String,
    slot: AnimSlotView,
    kind: Option<PresetKindView>,
    duration_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let length = item.end_us - item.start_us;
        let most = if slot == AnimSlotView::Loop {
            MAX_PRESET_US
        } else {
            (length / 2).max(MIN_PRESET_US)
        };
        Ok(Command::SetPresetAnim {
            id: item.id,
            slot: slot.into(),
            anim: kind.map(|kind| PresetAnim {
                kind: kind.into(),
                duration_us: duration_us.clamp(MIN_PRESET_US, most.min(MAX_PRESET_US)),
            }),
        })
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn preset_sample(kind: PresetKindView, slot: AnimSlotView, progress: f32) -> PresetSampleView {
    let modifier = preset_modifier(kind.into(), slot.into(), progress);
    PresetSampleView {
        dx: modifier.offset.x,
        dy: modifier.offset.y,
        scale: modifier.scale,
        rotation: modifier.rotation,
        opacity: modifier.opacity,
        reveal: modifier.reveal,
    }
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

/// The element's own placement at `time_us`: its keyframes and path, without presets. Where a
/// gesture in keyframe mode starts from.
#[flutter_rust_bridge::frb(sync)]
pub fn overlay_transform_at(id: String, time_us: i64) -> Option<TransformView> {
    let id = parse(&id).ok()?;
    let project = transport().project.clone()?;
    let item = project
        .timeline
        .overlays
        .iter()
        .find(|item| item.id == id)?;
    Some(own_transform(item, time_us - item.start_us).into())
}

/// Base, then the path, then keyframes on their own axis: the order evaluation uses.
fn own_transform(item: &OverlayItem, local: i64) -> Transform {
    let keyed = |property| {
        item.keyframes
            .iter()
            .any(|track| track.property == property && !track.keys.is_empty())
    };
    let path = item.path.as_ref().and_then(|p| path_position_at(p, local));
    let mut position = path.unwrap_or(item.base.position);
    if keyed(Prop::X) {
        position.x = property_value(item, Prop::X, local);
    }
    if keyed(Prop::Y) {
        position.y = property_value(item, Prop::Y, local);
    }
    Transform {
        position,
        scale: property_value(item, Prop::Scale, local),
        rotation: property_value(item, Prop::Rotation, local),
        opacity: property_value(item, Prop::Opacity, local),
        flip: item.base.flip,
    }
}

/// Shows keys for `transform` at `time_us` while a finger moves the element; nothing recorded.
#[flutter_rust_bridge::frb(sync)]
pub fn preview_overlay_key(id: String, time_us: i64, transform: TransformView) {
    let Ok(id) = parse(&id) else {
        return;
    };
    let target: Transform = transform.into();
    publish_overlay_keys(id, |item| {
        let local = time_us - item.start_us;
        keys_for(item, local, &target)
    });
}

/// Keys every property that differs from `transform` at `time_us`, as one undo step.
pub async fn key_overlay_transform(
    id: String,
    time_us: i64,
    transform: TransformView,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let target: Transform = transform.into();
        let local = (time_us - item.start_us).clamp(0, item.end_us - item.start_us);
        let commands = keys_for(item, local, &target)
            .into_iter()
            .map(|(property, key)| Command::SetKeyframe {
                id: item.id,
                property,
                key,
            })
            .collect();
        Ok(Command::Group {
            label: "keyframe".into(),
            commands,
        })
    })
}

/// The keys of every property at `time_us`, by timeline time.
fn keys_at(item: &OverlayItem, time_us: i64) -> Vec<(Prop, Key)> {
    let local = time_us - item.start_us;
    item.keyframes
        .iter()
        .flat_map(|track| {
            track
                .keys
                .iter()
                .filter(|key| key.t_us == local)
                .map(|key| (track.property, *key))
        })
        .collect()
}

pub async fn remove_keyframes_at(id: String, time_us: i64) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let commands = keys_at(item, time_us)
            .into_iter()
            .map(|(property, key)| Command::RemoveKeyframe {
                id: item.id,
                property,
                t_us: key.t_us,
            })
            .collect();
        Ok(Command::Group {
            label: "remove keyframe".into(),
            commands,
        })
    })
}

/// Moves every key at `from_us` to `to_us`, kept inside the element.
pub async fn move_keyframes(
    id: String,
    from_us: i64,
    to_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let to_local = (to_us - item.start_us).clamp(0, item.end_us - item.start_us);
        let mut commands = Vec::new();
        for (property, key) in keys_at(item, from_us) {
            commands.push(Command::RemoveKeyframe {
                id: item.id,
                property,
                t_us: key.t_us,
            });
            commands.push(Command::SetKeyframe {
                id: item.id,
                property,
                key: Key {
                    t_us: to_local,
                    ..key
                },
            });
        }
        Ok(Command::Group {
            label: "move keyframe".into(),
            commands,
        })
    })
}

/// The easing from the keys at `time_us` to the next ones.
pub async fn set_keyframe_easing(
    id: String,
    time_us: i64,
    easing: EasingView,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let commands = keys_at(item, time_us)
            .into_iter()
            .map(|(property, key)| Command::SetKeyframe {
                id: item.id,
                property,
                key: Key {
                    easing: easing.into(),
                    ..key
                },
            })
            .collect();
        Ok(Command::Group {
            label: "keyframe easing".into(),
            commands,
        })
    })
}

/// How far a recorded sample may stray from the kept line before it is kept, in canvas units.
const PATH_TOLERANCE: f32 = 3.0;
const DEFAULT_SMOOTHING: f32 = 0.6;
/// Steps per second when the curve is drawn on the preview.
const CURVE_STEPS_PER_SECOND: i64 = 60;
const MAX_CURVE_POINTS: i64 = 2_000;

fn edit_path(
    id: String,
    change: impl FnOnce(&OverlayItem, &mut MotionPath) -> Result<(), EngineError> + Send + 'static,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let mut path = item
            .path
            .clone()
            .ok_or_else(|| EngineError::Internal("no path".into()))?;
        change(item, &mut path)?;
        Ok(Command::SetMotionPath {
            id: item.id,
            path: Some(path),
        })
    })
}

/// Stores what a finger drew during playback: timeline times, canvas units. The samples are
/// simplified, then smoothed when drawn. One undo step.
pub async fn set_recorded_path(
    id: String,
    samples: Vec<PathPointView>,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        let length = item.end_us - item.start_us;
        let mut recorded: Vec<PathSample> = samples
            .iter()
            .map(|point| PathSample {
                t_us: (point.time_us - item.start_us).clamp(0, length),
                x: point.x,
                y: point.y,
            })
            .collect();
        recorded.sort_by_key(|sample| sample.t_us);
        recorded.dedup_by_key(|sample| sample.t_us);
        if recorded.len() < 2 {
            return Err(EngineError::Internal("a path needs two points".into()));
        }
        Ok(Command::SetMotionPath {
            id: item.id,
            path: Some(MotionPath {
                samples: simplify_path(&recorded, PATH_TOLERANCE),
                smoothing: DEFAULT_SMOOTHING,
                orient_to_path: item.path.as_ref().is_some_and(|p| p.orient_to_path),
            }),
        })
    })
}

pub async fn set_path_options(
    id: String,
    smoothing: f32,
    orient_to_path: bool,
) -> Result<ProjectView, EngineFailure> {
    edit_path(id, move |_, path| {
        path.smoothing = smoothing.clamp(0.0, 1.0);
        path.orient_to_path = orient_to_path;
        Ok(())
    })
}

pub async fn move_path_point(
    id: String,
    index: u32,
    x: f32,
    y: f32,
) -> Result<ProjectView, EngineFailure> {
    edit_path(id, move |_, path| {
        let point = path
            .samples
            .get_mut(index as usize)
            .ok_or_else(|| EngineError::Internal("no such point".into()))?;
        point.x = x;
        point.y = y;
        Ok(())
    })
}

/// Removes a point; a path keeps at least two.
pub async fn delete_path_point(id: String, index: u32) -> Result<ProjectView, EngineFailure> {
    edit_path(id, move |_, path| {
        let index = index as usize;
        if path.samples.len() <= 2 || index >= path.samples.len() {
            return Err(EngineError::Internal("a path keeps two points".into()));
        }
        path.samples.remove(index);
        Ok(())
    })
}

/// Re-times the path by stretching it to last `length_us` from its first point, kept inside
/// the element.
pub async fn stretch_path(id: String, length_us: i64) -> Result<ProjectView, EngineFailure> {
    edit_path(id, move |item, path| {
        let (Some(first), Some(last)) = (path.samples.first(), path.samples.last()) else {
            return Ok(());
        };
        let (start, span) = (first.t_us, last.t_us - first.t_us);
        if span <= 0 {
            return Ok(());
        }
        let room = item.end_us - item.start_us - start;
        let target = length_us.clamp(MIN_LENGTH_US, room.max(MIN_LENGTH_US));
        for sample in &mut path.samples {
            sample.t_us =
                start + ((sample.t_us - start) as f64 * target as f64 / span as f64).round() as i64;
        }
        Ok(())
    })
}

pub async fn clear_path(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let item = find(project, parse(&id)?)?;
        Ok(Command::SetMotionPath {
            id: item.id,
            path: None,
        })
    })
}

/// The smoothed curve the element follows, for drawing on the preview: canvas units.
#[flutter_rust_bridge::frb(sync)]
pub fn path_curve(id: String) -> Vec<PathPointView> {
    let Ok(id) = parse(&id) else {
        return Vec::new();
    };
    let Some(project) = transport().project.clone() else {
        return Vec::new();
    };
    let Some(item) = project.timeline.overlays.iter().find(|item| item.id == id) else {
        return Vec::new();
    };
    curve(item)
}

fn curve(item: &OverlayItem) -> Vec<PathPointView> {
    let Some(path) = &item.path else {
        return Vec::new();
    };
    let (Some(first), Some(last)) = (path.samples.first(), path.samples.last()) else {
        return Vec::new();
    };
    let span = last.t_us - first.t_us;
    let steps = (span * CURVE_STEPS_PER_SECOND / 1_000_000).clamp(1, MAX_CURVE_POINTS);
    (0..=steps)
        .filter_map(|step| {
            let local = first.t_us + span * step / steps;
            let point = path_position_at(path, local)?;
            Some(PathPointView {
                time_us: item.start_us + local,
                x: point.x,
                y: point.y,
            })
        })
        .collect()
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
    fn a_text_box_is_the_size_of_its_words() {
        let short = (
            OverlayKind::Text {
                content: "Hi".into(),
                style: TextStyle::default(),
            },
            OverlayKind::Text {
                content: "Hi\nthere friend".into(),
                style: TextStyle::default(),
            },
        );
        let project = Project::new(Uuid::nil(), "t".into(), DateTime::default());
        let (small_width, small_height) = natural_size(&short.0, &project);
        let (wide_width, tall_height) = natural_size(&short.1, &project);
        assert!(wide_width > small_width && tall_height > small_height * 1.7);
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

    #[test]
    fn keyframes_win_over_a_path_on_their_axis_when_editing() {
        let (mut project, _) = project_with(OverlayKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: DEFAULT_FILL.into(),
        });
        let item = &mut project.timeline.overlays[0];
        item.path = Some(MotionPath {
            samples: vec![
                PathSample {
                    t_us: 0,
                    x: 10.0,
                    y: 20.0,
                },
                PathSample {
                    t_us: 1_000_000,
                    x: 110.0,
                    y: 220.0,
                },
            ],
            smoothing: 0.0,
            orient_to_path: false,
        });
        item.keyframes = vec![AnimatedProperty {
            property: Prop::Y,
            keys: vec![Key {
                t_us: 0,
                value: 7.0,
                easing: Easing::Linear,
            }],
        }];
        let own = own_transform(item, 500_000);
        assert_eq!((own.position.x, own.position.y), (60.0, 7.0));
        // Without clips the project lasts no time, so evaluation only shows its start.
        let start = own_transform(&project.timeline.overlays[0], 0).position;
        let shown = evaluate(&project, 0).overlays[0].position;
        assert_eq!((shown.x, shown.y), (start.x, start.y));
    }

    #[test]
    fn the_drawn_curve_starts_and_ends_on_the_path() {
        let (mut project, _) = project_with(OverlayKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: DEFAULT_FILL.into(),
        });
        let item = &mut project.timeline.overlays[0];
        item.start_us = 1_000_000;
        item.path = Some(MotionPath {
            samples: vec![
                PathSample {
                    t_us: 0,
                    x: 0.0,
                    y: 0.0,
                },
                PathSample {
                    t_us: 500_000,
                    x: 100.0,
                    y: 0.0,
                },
                PathSample {
                    t_us: 1_000_000,
                    x: 100.0,
                    y: 100.0,
                },
            ],
            smoothing: 1.0,
            orient_to_path: false,
        });
        let drawn = curve(item);
        assert_eq!(drawn.len(), 61);
        assert_eq!(drawn[0].time_us, 1_000_000);
        let end = drawn.last().unwrap();
        assert_eq!((end.x, end.y, end.time_us), (100.0, 100.0, 2_000_000));
    }
}
