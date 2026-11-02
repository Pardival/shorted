//! Media of the open project: import, placing on the timeline, offline state and relink.

use std::path::{Path, PathBuf};
use std::thread;

use shorted_core::command::Command;
use shorted_core::model::*;
use shorted_media::probe_image;
use shorted_store::media::{classify, content_hash, copy_into_project};
use shorted_store::session::ProjectSession;
use uuid::Uuid;

use crate::api::projects::{active_folder, on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure, ErrorCode};
use crate::frb_generated::StreamSink;
use crate::transport::transport;

/// FR-CLIP-03.
const IMAGE_DURATION_US: i64 = 3_000_000;
const THUMBNAIL_HEIGHT: u32 = 96;
const THUMBNAIL_STEP_US: i64 = 1_000_000;
const MAX_THUMBNAILS: i64 = 120;
const MAX_THUMBNAIL_WIDTH: u32 = 256;
const STRIP_HEADER_BYTES: usize = 8;
const FRAME_HEADER_BYTES: usize = 8;

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

#[cfg(target_vendor = "apple")]
fn audio_probe(path: &Path) -> Result<MediaProbe, EngineError> {
    let duration_us =
        shorted_media::apple::audio_duration_us(path).map_err(|_| EngineError::UnsupportedMedia)?;
    Ok(MediaProbe {
        duration_us,
        width: 0,
        height: 0,
        rotation: 0,
        fps: 0.0,
        has_audio: true,
        codec: String::new(),
        color: ColorRange::Sdr,
    })
}

/// No decoder on this platform yet (T-0401).
#[cfg(not(target_vendor = "apple"))]
fn audio_probe(_path: &Path) -> Result<MediaProbe, EngineError> {
    Err(EngineError::UnsupportedMedia)
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
        MediaKind::Audio => audio_probe(path)?,
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

pub struct ThumbnailView {
    pub time_us: i64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn thumbnail_size(probe: &MediaProbe) -> (u32, u32) {
    let (width, height) = if probe.rotation.rem_euclid(180) == 90 {
        (probe.height, probe.width)
    } else {
        (probe.width, probe.height)
    };
    let scaled = u64::from(width) * u64::from(THUMBNAIL_HEIGHT) / u64::from(height.max(1));
    (
        (scaled as u32).clamp(1, MAX_THUMBNAIL_WIDTH),
        THUMBNAIL_HEIGHT,
    )
}

fn thumbnail_times(duration_us: i64) -> Vec<i64> {
    let step = THUMBNAIL_STEP_US.max(duration_us / MAX_THUMBNAILS);
    (0..MAX_THUMBNAILS)
        .map(|index| index * step)
        .take_while(|time| *time < duration_us.max(1))
        .collect()
}

fn encode_strip(frames: &[ThumbnailView]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let (width, height) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    for frame in frames {
        bytes.extend(frame.time_us.to_le_bytes());
        bytes.extend(&frame.rgba);
    }
    bytes
}

/// Reads a cached strip. The file sits in a folder anyone can edit, so sizes are checked, and a
/// file that does not add up counts as no cache.
fn decode_strip(bytes: &[u8], width: u32, height: u32) -> Option<Vec<ThumbnailView>> {
    let header = bytes.get(..STRIP_HEADER_BYTES)?;
    if header[..4] != width.to_le_bytes() || header[4..] != height.to_le_bytes() {
        return None;
    }
    let pixels = width as usize * height as usize * 4;
    let body = &bytes[STRIP_HEADER_BYTES..];
    let record = FRAME_HEADER_BYTES + pixels;
    if body.is_empty() || !body.len().is_multiple_of(record) {
        return None;
    }
    let frames = body.chunks_exact(record).map(|chunk| ThumbnailView {
        time_us: i64::from_le_bytes(chunk[..FRAME_HEADER_BYTES].try_into().unwrap_or_default()),
        width,
        height,
        rgba: chunk[FRAME_HEADER_BYTES..].to_vec(),
    });
    Some(frames.collect())
}

#[cfg(target_vendor = "apple")]
fn make_strip(
    path: &Path,
    probe: &MediaProbe,
    (width, height): (u32, u32),
    sink: &StreamSink<ThumbnailView>,
) -> Result<Vec<ThumbnailView>, EngineError> {
    let mut frames = Vec::new();
    let times = thumbnail_times(probe.duration_us);
    let size = (width as usize, height as usize);
    shorted_media::apple::filmstrip(path, times, probe.rotation, size, |time_us, rgba| {
        let sent = sink.add(ThumbnailView {
            time_us,
            width,
            height,
            rgba: rgba.clone(),
        });
        frames.push(ThumbnailView {
            time_us,
            width,
            height,
            rgba,
        });
        sent.is_ok()
    })
    .map_err(|_| EngineError::UnsupportedMedia)?;
    Ok(frames)
}

/// No decoder on this platform yet (T-0401).
#[cfg(not(target_vendor = "apple"))]
fn make_strip(
    _path: &Path,
    _probe: &MediaProbe,
    _size: (u32, u32),
    _sink: &StreamSink<ThumbnailView>,
) -> Result<Vec<ThumbnailView>, EngineError> {
    Ok(Vec::new())
}

/// The hash comes from the project file, so it must not be able to name a path.
fn strip_cache(media: &MediaRef, folder: &Path) -> Option<PathBuf> {
    let hash = &media.content_hash;
    (!hash.is_empty() && hash.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| folder.join("cache").join(format!("filmstrip-{hash}.bin")))
}

/// A still image is one thumbnail, repeated along its clip.
fn send_still(path: &Path, (width, height): (u32, u32), sink: &StreamSink<ThumbnailView>) {
    let Some((from_width, from_height, mut pixels)) = shorted_render::compositor::load_image(path)
    else {
        return;
    };
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let rgba = shorted_media::thumbnail_rgba(
        &pixels,
        from_width as usize * 4,
        (from_width as usize, from_height as usize),
        0,
        (width as usize, height as usize),
    );
    let _ = sink.add(ThumbnailView {
        time_us: 0,
        width,
        height,
        rgba,
    });
}

fn send_strip(media: &MediaRef, folder: &Path, sink: &StreamSink<ThumbnailView>) {
    let Some(path) = resolve(media, folder) else {
        return;
    };
    let (width, height) = thumbnail_size(&media.probe);
    if media.kind == MediaKind::Image {
        send_still(&path, (width, height), sink);
        return;
    }
    let cache = strip_cache(media, folder);
    let cached = cache.as_ref().and_then(|file| std::fs::read(file).ok());
    if let Some(frames) = cached.and_then(|bytes| decode_strip(&bytes, width, height)) {
        for frame in frames {
            if sink.add(frame).is_err() {
                return;
            }
        }
        return;
    }
    let frames = match make_strip(&path, &media.probe, (width, height), sink) {
        Ok(frames) if frames.len() == thumbnail_times(media.probe.duration_us).len() => frames,
        Ok(_) => return,
        Err(error) => {
            tracing::warn!(%error, "no filmstrip for this clip");
            return;
        }
    };
    let Some(cache) = cache else {
        return;
    };
    // Losing the cache only costs decoding again.
    let written = cache
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&cache, encode_strip(&frames)));
    if let Err(error) = written {
        tracing::warn!(%error, "filmstrip not cached");
    }
}

/// Streams the thumbnails of a video or image, in time order, from a thread of its own.
pub fn media_filmstrip(media_id: String, sink: StreamSink<ThumbnailView>) {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    let media = project.and_then(|project| {
        project
            .media
            .iter()
            .find(|media| {
                media.id.to_string() == media_id
                    && matches!(media.kind, MediaKind::Video | MediaKind::Image)
            })
            .cloned()
    });
    let Some(media) = media else {
        return;
    };
    let started = thread::Builder::new()
        .name("shorted-filmstrip".into())
        .spawn(move || send_strip(&media, &folder, &sink));
    if let Err(error) = started {
        tracing::error!(%error, "filmstrip not started");
    }
}

#[cfg(test)]
mod filmstrip_tests {
    use super::*;

    fn frame(time_us: i64) -> ThumbnailView {
        ThumbnailView {
            time_us,
            width: 2,
            height: 1,
            rgba: vec![7; 8],
        }
    }

    #[test]
    fn a_hash_cannot_leave_the_cache_folder() {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut media = Project::from_json(json).unwrap().media.remove(0);
        media.content_hash = "abc123".into();
        let file = strip_cache(&media, Path::new("/p")).unwrap();
        assert_eq!(file, Path::new("/p/cache/filmstrip-abc123.bin"));
        media.content_hash = "../../evil".into();
        assert!(strip_cache(&media, Path::new("/p")).is_none());
    }

    #[test]
    fn strips_survive_the_cache() {
        let bytes = encode_strip(&[frame(0), frame(1_000_000)]);
        let strip = decode_strip(&bytes, 2, 1).unwrap();
        assert_eq!(strip.len(), 2);
        assert_eq!(strip[1].time_us, 1_000_000);
        assert_eq!(strip[1].rgba, vec![7; 8]);
    }

    #[test]
    fn damaged_or_foreign_caches_are_ignored() {
        let bytes = encode_strip(&[frame(0)]);
        assert!(decode_strip(&bytes[..bytes.len() - 1], 2, 1).is_none());
        assert!(decode_strip(&bytes, 4, 1).is_none());
        assert!(decode_strip(&[], 2, 1).is_none());
        assert!(decode_strip(&bytes[..STRIP_HEADER_BYTES], 2, 1).is_none());
    }

    #[test]
    fn times_stay_inside_the_clip_and_their_number_is_bounded() {
        assert_eq!(thumbnail_times(2_500_000), [0, 1_000_000, 2_000_000]);
        assert_eq!(thumbnail_times(0), [0]);
        assert_eq!(thumbnail_times(3_600_000_000).len(), 120);
    }
}
