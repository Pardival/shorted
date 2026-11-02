//! Clip edits from the timeline. Times arrive in timeline microseconds and are turned into source
//! times here, on a 30 fps grid (T-0304), so Dart holds no editing logic.

use shorted_core::command::Command;
use shorted_core::evaluate::clip_positions;
use shorted_core::model::{ClipItem, Crop, Id, KenBurns, MediaProbe, Project, Vec2};
use uuid::Uuid;

use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};
use crate::transport::{publish_crop, transport};

const FRAME_US: f64 = 1_000_000.0 / 30.0;
const FRAME_ASPECT: f32 = 9.0 / 16.0;
const MAX_CROP_ZOOM: f32 = 4.0;
const SLOW_ZOOM_END: f32 = 1.15;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipEdge {
    Start,
    End,
}

fn parse(id: &str) -> Result<Id, EngineError> {
    id.parse()
        .map_err(|_| EngineError::Internal("bad clip id".into()))
}

fn snap_to_frame(time_us: i64) -> i64 {
    ((time_us as f64 / FRAME_US).round() * FRAME_US).round() as i64
}

fn locate(project: &Project, id: Id) -> Result<(usize, &ClipItem, i64), EngineError> {
    let index = project
        .timeline
        .clips
        .iter()
        .position(|clip| clip.id == id)
        .ok_or_else(|| EngineError::Internal("unknown clip".into()))?;
    let start = clip_positions(project)[index].1;
    Ok((index, &project.timeline.clips[index], start))
}

/// Builds a command from the project, runs it, and returns the new view.
fn edit(
    build: impl FnOnce(&Project) -> Result<Command, EngineError> + Send + 'static,
) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let command = {
            let mut built = None;
            with_active(|session| {
                built = Some(build(session.project()));
                Ok(())
            })?;
            built.ok_or_else(|| EngineError::Internal("no project is open".into()))??
        };
        with_active(|session| {
            session.execute(&command)?;
            Ok(view_of(session))
        })
    })
}

/// How much of the picture the frame shows, from 0 to 1 on each axis.
fn visible_fraction(picture_aspect: f32, zoom: f32) -> (f32, f32) {
    if picture_aspect > FRAME_ASPECT {
        (FRAME_ASPECT / picture_aspect / zoom, 1.0 / zoom)
    } else {
        (1.0 / zoom, picture_aspect / FRAME_ASPECT / zoom)
    }
}

/// A drag of (`dx`, `dy`) fractions of the frame, and a pinch by `zoom_by`. The picture follows
/// the finger, so its centre moves the other way, and it always covers the frame.
fn moved_crop(crop: Crop, picture_aspect: f32, dx: f32, dy: f32, zoom_by: f32) -> Crop {
    let scale = (crop.scale * zoom_by).clamp(1.0, MAX_CROP_ZOOM);
    let (x, y) = visible_fraction(picture_aspect, scale);
    Crop {
        center: Vec2 {
            x: (crop.center.x - dx * x).clamp(x / 2.0, 1.0 - x / 2.0),
            y: (crop.center.y - dy * y).clamp(y / 2.0, 1.0 - y / 2.0),
        },
        scale,
        rotation: crop.rotation,
    }
}

fn picture_aspect(probe: &MediaProbe) -> f32 {
    let (width, height) = if probe.rotation.rem_euclid(180) == 90 {
        (probe.height, probe.width)
    } else {
        (probe.width, probe.height)
    };
    width as f32 / height.max(1) as f32
}

/// Follows a gesture on the preview. Nothing is recorded until `commit_crop`, so that one gesture
/// is one undo step.
#[flutter_rust_bridge::frb(sync)]
pub fn move_crop(id: String, dx: f32, dy: f32, zoom_by: f32) {
    let Ok(id) = parse(&id) else {
        return;
    };
    let moved = {
        let state = transport();
        state.project.as_ref().and_then(|project| {
            let clip = project.timeline.clips.iter().find(|clip| clip.id == id)?;
            let media = project.media.iter().find(|m| m.id == clip.media_id)?;
            Some(moved_crop(
                clip.crop,
                picture_aspect(&media.probe),
                dx,
                dy,
                zoom_by,
            ))
        })
    };
    if let Some(crop) = moved {
        publish_crop(id, crop);
    }
}

/// Records the crop the gesture left on the preview, as one undo step.
pub async fn commit_crop(id: String) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let id = parse(&id)?;
        let shown = {
            let state = transport();
            state.project.as_ref().and_then(|project| {
                let clip = project.timeline.clips.iter().find(|clip| clip.id == id);
                clip.map(|clip| clip.crop)
            })
        };
        with_active(|session| {
            let saved = session
                .project()
                .timeline
                .clips
                .iter()
                .find(|clip| clip.id == id)
                .map(|clip| clip.crop);
            if let Some(crop) = shown.filter(|crop| Some(*crop) != saved) {
                session.execute(&Command::SetCrop { id, crop })?;
            }
            Ok(view_of(session))
        })
    })
}

/// A slow zoom on the picture as it is framed.
fn slow_zoom(crop: &Crop) -> KenBurns {
    KenBurns {
        start_scale: 1.0,
        end_scale: SLOW_ZOOM_END,
        start_center: crop.center,
        end_center: crop.center,
    }
}

/// Turns the slow zoom of an image clip on or off (FR-CLIP-03).
pub async fn toggle_slow_zoom(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let (_, clip, _) = locate(project, id)?;
        Ok(Command::SetImageMotion {
            id,
            motion: match clip.image_motion {
                Some(_) => None,
                None => Some(slow_zoom(&clip.crop)),
            },
        })
    })
}

/// Back to the centred picture.
pub async fn reset_crop(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let (_, clip, _) = locate(project, id)?;
        Ok(Command::SetCrop {
            id,
            crop: Crop {
                center: Vec2 { x: 0.5, y: 0.5 },
                scale: 1.0,
                rotation: clip.crop.rotation,
            },
        })
    })
}

/// Splits the clip under `time_us` at the nearest frame.
pub async fn split_clip(id: String, time_us: i64) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let (_, clip, start) = locate(project, id)?;
        let local = ((time_us - start) as f64 * f64::from(clip.speed)).round() as i64;
        Ok(Command::SplitClip {
            id,
            at_source_us: snap_to_frame(clip.source_in_us + local),
            new_id: Uuid::now_v7(),
        })
    })
}

pub async fn delete_clip(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |_| Ok(Command::DeleteClip { id: parse(&id)? }))
}

pub async fn duplicate_clip(id: String) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let (index, clip, _) = locate(project, parse(&id)?)?;
        let mut copy = clip.clone();
        copy.id = Uuid::now_v7();
        Ok(Command::InsertClips {
            at: index + 1,
            clips: vec![copy],
        })
    })
}

pub async fn move_clip(id: String, to: u32) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::MoveClip {
            id: parse(&id)?,
            to: to as usize,
        })
    })
}

/// Moves one edge by `delta_us` of timeline time, within the source.
pub async fn trim_clip(
    id: String,
    edge: ClipEdge,
    delta_us: i64,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        let id = parse(&id)?;
        let (_, clip, _) = locate(project, id)?;
        let media = project.media.iter().find(|m| m.id == clip.media_id);
        let available = media.map_or(clip.source_out_us, |m| m.probe.duration_us);
        let delta = (delta_us as f64 * f64::from(clip.speed)).round() as i64;
        let frame = FRAME_US.ceil() as i64;
        let (source_in_us, source_out_us) = match edge {
            ClipEdge::Start => (
                snap_to_frame(clip.source_in_us + delta).clamp(0, clip.source_out_us - frame),
                clip.source_out_us,
            ),
            ClipEdge::End => (
                clip.source_in_us,
                snap_to_frame(clip.source_out_us + delta)
                    .clamp(clip.source_in_us + frame, available),
            ),
        };
        Ok(Command::TrimClip {
            id,
            source_in_us,
            source_out_us,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centred() -> Crop {
        Crop {
            center: Vec2 { x: 0.5, y: 0.5 },
            scale: 1.0,
            rotation: 0.0,
        }
    }

    #[test]
    fn a_slow_zoom_keeps_the_framing() {
        let mut crop = centred();
        crop.center.x = 0.3;
        let motion = slow_zoom(&crop);
        assert_eq!(motion.start_center, crop.center);
        assert_eq!(motion.end_center, crop.center);
        assert!(motion.end_scale > motion.start_scale);
    }

    #[test]
    fn a_landscape_picture_slides_sideways_only() {
        let crop = moved_crop(centred(), 16.0 / 9.0, 0.5, 0.5, 1.0);
        assert!(crop.center.x < 0.5);
        assert_eq!(crop.center.y, 0.5);
    }

    #[test]
    fn a_crop_always_covers_the_frame() {
        let crop = moved_crop(centred(), 16.0 / 9.0, 100.0, 0.0, 1.0);
        let (x, _) = visible_fraction(16.0 / 9.0, 1.0);
        assert_eq!(crop.center.x, x / 2.0);
        let crop = moved_crop(centred(), 16.0 / 9.0, -100.0, 0.0, 1.0);
        assert_eq!(crop.center.x, 1.0 - x / 2.0);
    }

    #[test]
    fn zoom_stays_in_range_and_widens_the_room_to_move() {
        assert_eq!(moved_crop(centred(), 1.0, 0.0, 0.0, 0.1).scale, 1.0);
        assert_eq!(moved_crop(centred(), 1.0, 0.0, 0.0, 99.0).scale, 4.0);
        let zoomed = moved_crop(centred(), 9.0 / 16.0, 0.0, 0.0, 2.0);
        assert_eq!(zoomed.center.y, 0.5);
        assert!(moved_crop(zoomed, 9.0 / 16.0, 0.0, 0.2, 1.0).center.y < 0.5);
    }

    #[test]
    fn rotated_pictures_use_their_shown_shape() {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut probe = Project::from_json(json).unwrap().media.remove(0).probe;
        (probe.width, probe.height, probe.rotation) = (1920, 1080, 90);
        assert_eq!(picture_aspect(&probe), 1080.0 / 1920.0);
    }

    #[test]
    fn times_snap_to_the_30_fps_grid() {
        assert_eq!(snap_to_frame(0), 0);
        assert_eq!(snap_to_frame(33_000), 33_333);
        assert_eq!(snap_to_frame(1_000_010), 1_000_000);
        assert_eq!(snap_to_frame(1_016_000), 1_000_000);
        assert_eq!(snap_to_frame(1_017_000), 1_033_333);
    }
}
