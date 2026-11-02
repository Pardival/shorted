//! Media of the open project: import, placing on the timeline, offline state and relink.

use std::path::{Path, PathBuf};

use shorted_core::command::Command;
use shorted_core::model::*;
use shorted_media::probe_image;
use shorted_store::media::{classify, content_hash, copy_into_project};
use shorted_store::session::ProjectSession;
use uuid::Uuid;

use crate::api::projects::{active_folder, on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure, ErrorCode};

/// FR-CLIP-03.
const IMAGE_DURATION_US: i64 = 3_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MediaKindView {
    Video,
    Image,
    AnimatedImage,
    Audio,
}

pub struct MediaItemView {
    pub id: String,
    pub name: String,
    pub kind: MediaKindView,
    pub duration_us: i64,
    pub offline: bool,
}

/// One file of an import: the others go on when one fails.
pub struct ImportOutcome {
    pub media_id: Option<String>,
    pub error: Option<ErrorCode>,
}

fn display_name(location: &MediaLocation) -> String {
    let path = match location {
        MediaLocation::ProjectCopy { rel_path } => rel_path,
        MediaLocation::External { abs_path, .. } => abs_path,
    };
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Where the file is now, if it can be reached.
pub(crate) fn resolve(media: &MediaRef, folder: &Path) -> Option<PathBuf> {
    match &media.location {
        MediaLocation::ProjectCopy { rel_path } => {
            Some(folder.join(rel_path)).filter(|path| path.exists())
        }
        MediaLocation::External { abs_path, bookmark } => {
            #[cfg(target_vendor = "apple")]
            {
                let found = bookmark
                    .as_deref()
                    .and_then(shorted_media::apple::resolve_bookmark)
                    .filter(|path| path.exists());
                if found.is_some() {
                    return found;
                }
            }
            #[cfg(not(target_vendor = "apple"))]
            let _ = bookmark;
            Some(PathBuf::from(abs_path)).filter(|path| path.exists())
        }
    }
}

pub(crate) fn media_views(session: &ProjectSession) -> Vec<MediaItemView> {
    let project = session.project();
    let folder = active_folder(project.id).unwrap_or_default();
    project
        .media
        .iter()
        .map(|media| MediaItemView {
            id: media.id.to_string(),
            name: display_name(&media.location),
            kind: match media.kind {
                MediaKind::Video => MediaKindView::Video,
                MediaKind::Image => MediaKindView::Image,
                MediaKind::AnimatedImage => MediaKindView::AnimatedImage,
                MediaKind::Audio => MediaKindView::Audio,
            },
            duration_us: media.probe.duration_us,
            offline: resolve(media, &folder).is_none(),
        })
        .collect()
}

fn image_probe(path: &Path) -> Result<MediaProbe, EngineError> {
    let (width, height) = probe_image(path).map_err(|_| EngineError::UnsupportedMedia)?;
    Ok(MediaProbe {
        duration_us: IMAGE_DURATION_US,
        width,
        height,
        rotation: 0,
        fps: 0.0,
        has_audio: false,
        codec: String::new(),
        color: ColorRange::Sdr,
    })
}

#[cfg(target_vendor = "apple")]
fn video_probe(path: &Path) -> Result<MediaProbe, EngineError> {
    use shorted_media::MediaBackend;
    let info = shorted_media::apple::AppleBackend
        .probe(path)
        .map_err(|_| EngineError::UnsupportedMedia)?;
    Ok(MediaProbe {
        duration_us: info.duration_us,
        width: info.width,
        height: info.height,
        rotation: info.rotation_degrees,
        fps: info.nominal_fps,
        has_audio: shorted_media::apple::has_audio(path),
        // Colour details come with the full probe (T-0301).
        codec: String::new(),
        color: ColorRange::Sdr,
    })
}

/// No decoder on this platform yet (T-0401).
#[cfg(not(target_vendor = "apple"))]
fn video_probe(_path: &Path) -> Result<MediaProbe, EngineError> {
    Err(EngineError::UnsupportedMedia)
}

fn location_for(path: &Path, folder: &Path, id: Id) -> Result<MediaLocation, EngineError> {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        // FR-PRJ-05: on phones the picked file is only reachable for a moment.
        let rel_path = copy_into_project(folder, path, id)
            .map_err(shorted_store::session::StoreError::from)?;
        return Ok(MediaLocation::ProjectCopy { rel_path });
    }
    #[cfg(target_vendor = "apple")]
    let bookmark = shorted_media::apple::create_bookmark(path);
    #[cfg(not(target_vendor = "apple"))]
    let bookmark = None;
    Ok(MediaLocation::External {
        abs_path: path.to_string_lossy().into_owned(),
        bookmark,
    })
}

fn make_media(path: &Path, folder: &Path) -> Result<MediaRef, EngineError> {
    let kind = classify(path).ok_or(EngineError::UnsupportedMedia)?;
    let probe = match kind {
        MediaKind::Video => video_probe(path)?,
        _ => image_probe(path)?,
    };
    let content_hash = content_hash(path).map_err(shorted_store::session::StoreError::from)?;
    let id = Uuid::now_v7();
    Ok(MediaRef {
        id,
        kind,
        location: location_for(path, folder, id)?,
        content_hash,
        probe,
        captured_at: None,
    })
}

fn import_one(path: &Path, session: &mut ProjectSession) -> Result<Id, EngineError> {
    let folder = active_folder(session.project().id)
        .ok_or_else(|| EngineError::Internal("no project is open".into()))?;
    let hash = content_hash(path).map_err(shorted_store::session::StoreError::from)?;
    if let Some(existing) = session
        .project()
        .media
        .iter()
        .find(|m| m.content_hash == hash)
    {
        return Ok(existing.id);
    }
    let media = make_media(path, &folder)?;
    let id = media.id;
    session.execute(&Command::ImportMedia { media: vec![media] })?;
    Ok(id)
}

/// Imports files one by one: a bad file fails alone.
pub async fn import_files(paths: Vec<String>) -> Result<Vec<ImportOutcome>, EngineFailure> {
    on_command_thread(move || {
        with_active(|session| {
            Ok(paths
                .iter()
                .map(|path| match import_one(Path::new(path), session) {
                    Ok(id) => ImportOutcome {
                        media_id: Some(id.to_string()),
                        error: None,
                    },
                    Err(error) => {
                        tracing::warn!(%error, "file not imported");
                        ImportOutcome {
                            media_id: None,
                            error: Some(error.code()),
                        }
                    }
                })
                .collect())
        })
    })
}

fn clip_for(media: &MediaRef) -> ClipItem {
    ClipItem {
        id: Uuid::now_v7(),
        media_id: media.id,
        source_in_us: 0,
        source_out_us: media.probe.duration_us,
        speed: 1.0,
        crop: Crop {
            center: Vec2 { x: 0.5, y: 0.5 },
            scale: 1.0,
            rotation: 0.0,
        },
        color: ColorAdjust {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        },
        image_motion: None,
        original_audio: ClipAudio {
            volume: 1.0,
            muted: false,
            fade_in_us: 0,
            fade_out_us: 0,
            transcribe: true,
        },
        transcript: None,
    }
}

/// Appends clips in this order, as one undo step (FR-CLIP-05).
pub async fn place_clips(media_ids: Vec<String>) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let mut clips = Vec::with_capacity(media_ids.len());
        let at = with_active(|session| {
            let project = session.project();
            for id in &media_ids {
                let media = project.media.iter().find(|m| m.id.to_string() == *id);
                clips.extend(media.map(clip_for));
            }
            Ok(project.timeline.clips.len())
        })?;
        with_active(|session| {
            session.execute(&Command::InsertClips { at, clips })?;
            Ok(view_of(session))
        })
    })
}

/// Points a missing media at a file with the same content.
pub async fn relink_media(id: String, path: String) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let path = PathBuf::from(path);
        let hash = content_hash(&path).map_err(shorted_store::session::StoreError::from)?;
        let mut active = None;
        with_active(|session| {
            let project = session.project();
            active = project
                .media
                .iter()
                .find(|m| m.id.to_string() == id)
                .map(|m| (m.id, m.content_hash.clone(), project.id));
            Ok(())
        })?;
        let (media_id, expected, project_id) =
            active.ok_or_else(|| EngineError::Internal("unknown media".into()))?;
        if hash != expected {
            return Err(EngineError::DifferentMedia);
        }
        let folder = active_folder(project_id).unwrap_or_default();
        let location = location_for(&path, &folder, media_id)?;
        with_active(|session| {
            session.execute(&Command::RelinkMedia {
                id: media_id,
                location,
            })?;
            Ok(view_of(session))
        })
    })
}
