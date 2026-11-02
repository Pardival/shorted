//! Clip edits from the timeline. Times arrive in timeline microseconds and are turned into source
//! times here, on a 30 fps grid (T-0304), so Dart holds no editing logic.

use shorted_core::command::Command;
use shorted_core::evaluate::clip_positions;
use shorted_core::model::{ClipItem, Id, Project};
use uuid::Uuid;

use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};

const FRAME_US: f64 = 1_000_000.0 / 30.0;

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

    #[test]
    fn times_snap_to_the_30_fps_grid() {
        assert_eq!(snap_to_frame(0), 0);
        assert_eq!(snap_to_frame(33_000), 33_333);
        assert_eq!(snap_to_frame(1_000_010), 1_000_000);
        assert_eq!(snap_to_frame(1_016_000), 1_000_000);
        assert_eq!(snap_to_frame(1_017_000), 1_033_333);
    }
}
