//! My assets, the Brand kit (T-0704): pictures, animations and short videos kept once for every
//! project, in `<root>/brand-kit`. Placing one copies it into the project, which stays complete.

use std::fs;
use std::path::{Path, PathBuf};

use shorted_core::command::Command;
use shorted_core::model::*;
use shorted_render::pictures::{checked_size, is_animated, preview, PictureError};
use shorted_store::media::{content_hash, copy_into_project};
use uuid::Uuid;

use crate::api::library::PictureView;
use crate::api::media::{video_probe, ImportOutcome};
use crate::api::overlays::new_overlay;
use crate::api::projects::{active_folder, on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};

const FOLDER: &str = "brand-kit";
/// Between the id and the original name in a stored file's name.
const SEPARATOR: char = '~';
const MAX_NAME: usize = 60;
const MAX_FILE_BYTES: u64 = 500 * 1024 * 1024;
/// Video overlays are cut-aways: no longer than a whole short (NFR limits).
const MAX_VIDEO_US: i64 = 120_000_000;
const MAX_VIDEO_SIDE: u32 = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrandKindView {
    Picture,
    Animation,
    Video,
}

pub struct BrandAssetView {
    /// The stored file's name: what the other calls take.
    pub id: String,
    pub name: String,
    pub kind: BrandKindView,
}

fn folder(root: &str) -> PathBuf {
    Path::new(root).join(FOLDER)
}

fn kind_of(path: &Path) -> Option<BrandKindView> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "mov" | "mp4" | "m4v" => Some(BrandKindView::Video),
        "gif" | "webp" | "png" | "apng" if is_animated(path) => Some(BrandKindView::Animation),
        "png" | "jpg" | "jpeg" | "webp" | "svg" => Some(BrandKindView::Picture),
        _ => None,
    }
}

/// Only names this module wrote: no separators, no way out of the folder.
fn stored(root: &str, id: &str) -> Result<PathBuf, EngineError> {
    let plain = !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ' | SEPARATOR));
    let path = folder(root).join(id);
    if plain && path.is_file() {
        Ok(path)
    } else {
        Err(EngineError::UnsupportedMedia)
    }
}

fn display_name(id: &str) -> String {
    let stem = Path::new(id)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(id);
    stem.split_once(SEPARATOR)
        .map_or(stem, |(_, name)| name)
        .to_string()
}

/// The newest first.
pub async fn brand_kit(root: String) -> Result<Vec<BrandAssetView>, EngineFailure> {
    on_command_thread(move || {
        let Ok(entries) = fs::read_dir(folder(&root)) else {
            return Ok(Vec::new());
        };
        let mut assets: Vec<BrandAssetView> = entries
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().to_str()?.to_string();
                let kind = kind_of(&entry.path())?;
                Some(BrandAssetView {
                    name: display_name(&id),
                    id,
                    kind,
                })
            })
            .collect();
        assets.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(assets)
    })
}

/// Refuses what the app cannot use, and anything beyond the limits of docs/08.
fn check(path: &Path) -> Result<BrandKindView, EngineError> {
    let kind = kind_of(path).ok_or(EngineError::UnsupportedMedia)?;
    if fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(EngineError::TooLarge);
    }
    match kind {
        BrandKindView::Video => {
            let probe = video_probe(path)?;
            if probe.duration_us > MAX_VIDEO_US || probe.width.max(probe.height) > MAX_VIDEO_SIDE {
                return Err(EngineError::TooLarge);
            }
        }
        BrandKindView::Animation => {
            shorted_render::pictures::decode_animation(path).map_err(picture_error)?;
        }
        BrandKindView::Picture => {
            checked_size(path).map_err(picture_error)?;
        }
    }
    Ok(kind)
}

fn picture_error(error: PictureError) -> EngineError {
    match error {
        PictureError::TooLarge => EngineError::TooLarge,
        PictureError::Unsupported => EngineError::UnsupportedMedia,
    }
}

fn import_one(root: &str, source: &Path) -> Result<String, EngineError> {
    check(source)?;
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name: String = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .take(MAX_NAME)
        .collect();
    let id = format!("{}{SEPARATOR}{name}.{extension}", Uuid::now_v7());
    fs::create_dir_all(folder(root))?;
    fs::copy(source, folder(root).join(&id))?;
    Ok(id)
}

/// Copies files into the Brand kit, one by one: a refused file fails alone.
pub async fn import_brand_assets(
    root: String,
    paths: Vec<String>,
) -> Result<Vec<ImportOutcome>, EngineFailure> {
    on_command_thread(move || {
        Ok(paths
            .iter()
            .map(|path| match import_one(&root, Path::new(path)) {
                Ok(id) => ImportOutcome {
                    media_id: Some(id),
                    error: None,
                },
                Err(error) => {
                    tracing::warn!(%error, "file not added to the brand kit");
                    ImportOutcome {
                        media_id: None,
                        error: Some(error.code()),
                    }
                }
            })
            .collect())
    })
}

/// Removes it from the Brand kit. Projects keep their own copy.
pub async fn delete_brand_asset(root: String, id: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        fs::remove_file(stored(&root, &id)?)?;
        Ok(())
    })
}

/// A small picture of an asset; videos show their first frame where a decoder exists.
#[flutter_rust_bridge::frb(sync)]
pub fn brand_thumbnail(root: String, id: String, size: u32) -> Option<PictureView> {
    let path = stored(&root, &id).ok()?;
    let (width, height, rgba) = match kind_of(&path)? {
        BrandKindView::Video => video_still(&path, size)?,
        _ => preview(&path, size)?,
    };
    Some(PictureView {
        width,
        height,
        rgba,
    })
}

#[cfg(target_vendor = "apple")]
fn video_still(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let probe = video_probe(path).ok()?;
    let (width, height) = if probe.rotation % 180 == 0 {
        (probe.width, probe.height)
    } else {
        (probe.height, probe.width)
    };
    let scale = size as f32 / width.max(height).max(1) as f32;
    let target = (
        ((width as f32 * scale) as usize).max(1),
        ((height as f32 * scale) as usize).max(1),
    );
    let mut still = None;
    shorted_media::apple::filmstrip(path, vec![0], probe.rotation, target, |_, rgba| {
        still = Some(rgba);
        false
    })
    .ok()?;
    Some((target.0 as u32, target.1 as u32, still?))
}

#[cfg(not(target_vendor = "apple"))]
fn video_still(_path: &Path, _size: u32) -> Option<(u32, u32, Vec<u8>)> {
    None
}

fn media_for(path: &Path, kind: BrandKindView, folder: &Path) -> Result<MediaRef, EngineError> {
    let id = Uuid::now_v7();
    let probe = match kind {
        BrandKindView::Video => video_probe(path)?,
        _ => {
            let (width, height) = checked_size(path).map_err(picture_error)?;
            MediaProbe {
                duration_us: 0,
                width,
                height,
                rotation: 0,
                fps: 0.0,
                has_audio: false,
                codec: String::new(),
                color: ColorRange::Sdr,
            }
        }
    };
    Ok(MediaRef {
        id,
        kind: match kind {
            BrandKindView::Video => MediaKind::Video,
            BrandKindView::Animation => MediaKind::AnimatedImage,
            BrandKindView::Picture => MediaKind::Image,
        },
        location: MediaLocation::ProjectCopy {
            rel_path: copy_into_project(folder, path, id)?,
        },
        content_hash: content_hash(path)?,
        probe,
        captured_at: None,
        name: None,
    })
}

/// Places an asset from the playhead, centred on (`x`, `y`) in canvas units or in the middle.
/// A video keeps its own length and plays silent; pictures last a few seconds. One undo step.
pub async fn add_brand_asset(
    root: String,
    id: String,
    at_us: i64,
    x: Option<f32>,
    y: Option<f32>,
) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let path = stored(&root, &id)?;
        let kind = check(&path)?;
        let hash = content_hash(&path)?;
        let mut built = None;
        with_active(|session| {
            let project = session.project();
            let project_folder = active_folder(project.id).unwrap_or_default();
            let existing = project
                .media
                .iter()
                .find(|media| media.content_hash == hash)
                .cloned();
            built = Some((existing, project_folder));
            Ok(())
        })?;
        let (existing, project_folder) =
            built.ok_or_else(|| EngineError::Internal("no project is open".into()))?;
        let mut commands = Vec::with_capacity(2);
        let media = match existing {
            Some(media) => media,
            None => {
                let media = media_for(&path, kind, &project_folder)?;
                commands.push(Command::ImportMedia {
                    media: vec![media.clone()],
                });
                media
            }
        };
        let overlay_kind = match kind {
            BrandKindView::Video => OverlayKind::Video {
                media: media.id,
                source_in_us: 0,
            },
            BrandKindView::Animation => OverlayKind::AnimatedImage { asset: media.id },
            BrandKindView::Picture => OverlayKind::Image { asset: media.id },
        };
        with_active(|session| {
            let mut overlay = new_overlay(overlay_kind, at_us, session.project());
            if kind == BrandKindView::Video && media.probe.duration_us > 0 {
                overlay.end_us = overlay.start_us + media.probe.duration_us;
            }
            if let (Some(x), Some(y)) = (x, y) {
                overlay.base.position = Vec2 { x, y };
            }
            commands.push(Command::AddOverlay {
                overlay: Box::new(overlay),
            });
            session.execute(&Command::Group {
                label: "add brand asset".into(),
                commands,
            })?;
            Ok(view_of(session))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn picture(dir: &Path, name: &str, side: u32) -> PathBuf {
        let path = dir.join(name);
        RgbaImage::from_pixel(side, side, Rgba([1, 2, 3, 255]))
            .save(&path)
            .unwrap();
        path
    }

    use flutter_rust_bridge::for_generated::futures::executor::block_on as block;

    #[test]
    fn imported_pictures_are_listed_newest_first_with_their_names() {
        let root = tempfile::tempdir().unwrap();
        let sources = tempfile::tempdir().unwrap();
        let root_text = root.path().to_string_lossy().into_owned();
        let first = import_one(&root_text, &picture(sources.path(), "Logo v1.png", 8)).unwrap();
        let second = import_one(&root_text, &picture(sources.path(), "badge.png", 8)).unwrap();
        let listed = block(brand_kit(root_text.clone())).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second);
        assert_eq!(listed[1].name, "Logo v1");
        assert_eq!(listed[1].kind, BrandKindView::Picture);
        assert!(brand_thumbnail(root_text.clone(), first.clone(), 4).is_some());
        block(delete_brand_asset(root_text.clone(), first)).unwrap();
        assert_eq!(block(brand_kit(root_text)).unwrap().len(), 1);
    }

    #[test]
    fn unknown_types_and_paths_out_of_the_folder_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let root_text = root.path().to_string_lossy().into_owned();
        let text = root.path().join("notes.txt");
        fs::write(&text, "hello").unwrap();
        assert!(matches!(
            import_one(&root_text, &text),
            Err(EngineError::UnsupportedMedia)
        ));
        assert!(stored(&root_text, "../notes.txt").is_err());
        assert!(stored(&root_text, "").is_err());
    }

    #[test]
    fn an_animated_gif_is_an_animation() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moving.gif");
        let mut encoder = GifEncoder::new(fs::File::create(&path).unwrap());
        for shade in [0, 200] {
            let frame = RgbaImage::from_pixel(4, 4, Rgba([shade, 0, 0, 255]));
            encoder
                .encode_frame(Frame::from_parts(
                    frame,
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
        drop(encoder);
        assert_eq!(check(&path).unwrap(), BrandKindView::Animation);
    }
}
