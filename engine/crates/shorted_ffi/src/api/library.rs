//! The Shorted library in the Enhance section (T-0703): browsing, thumbnails, and placing an item
//! on the preview. A placed item is copied into the project, so the project stays complete.

use std::fs;

use shorted_core::command::Command;
use shorted_core::model::*;
use shorted_render::library::{find, rasterize_svg, search, svg_size};
use shorted_store::media::content_hash;
use uuid::Uuid;

use crate::api::overlays::new_overlay;
use crate::api::projects::{active_folder, on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};

/// Where placed library items are kept inside a project folder.
const LIBRARY_FOLDER: &str = "library";

pub struct LibraryItemView {
    pub name: String,
    pub category: String,
}

pub struct PictureView {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Items matching every word of `query`, in one category or all.
#[flutter_rust_bridge::frb(sync)]
pub fn library_items(query: String, category: Option<String>) -> Vec<LibraryItemView> {
    search(&query, category.as_deref())
        .into_iter()
        .map(|asset| LibraryItemView {
            name: asset.name.into(),
            category: asset.category.into(),
        })
        .collect()
}

/// A small picture of an item, at most `size` pixels on its longer side.
#[flutter_rust_bridge::frb(sync)]
pub fn library_thumbnail(name: String, size: u32) -> Option<PictureView> {
    let asset = find(&name)?;
    let (width, height, rgba) = rasterize_svg(asset.svg.as_bytes(), size.max(1))?;
    Some(PictureView {
        width,
        height,
        rgba,
    })
}

/// Whether a media is a placed library item, which the media bin leaves out.
pub(crate) fn is_library_item(media: &MediaRef) -> bool {
    matches!(&media.location, MediaLocation::ProjectCopy { rel_path }
        if rel_path.starts_with(&format!("{LIBRARY_FOLDER}/")))
}

/// Places `name` from the playhead, centred on (`x`, `y`) in canvas units, or in the middle of
/// the frame. One undo step, even when the item is copied into the project for the first time.
pub async fn add_library_item(
    name: String,
    at_us: i64,
    x: Option<f32>,
    y: Option<f32>,
) -> Result<ProjectView, EngineFailure> {
    on_command_thread(move || {
        let asset = find(&name).ok_or_else(|| EngineError::Internal("unknown item".into()))?;
        let (width, height) = svg_size(asset.svg.as_bytes())
            .ok_or_else(|| EngineError::Internal("unreadable item".into()))?;
        with_active(|session| {
            let project = session.project();
            let folder = active_folder(project.id).unwrap_or_default();
            let rel_path = format!("{LIBRARY_FOLDER}/{}.svg", asset.name);
            let path = folder.join(&rel_path);
            if !path.exists() {
                fs::create_dir_all(folder.join(LIBRARY_FOLDER))?;
                fs::write(&path, asset.svg)?;
            }
            let hash = content_hash(&path)?;
            let existing = project
                .media
                .iter()
                .find(|media| media.content_hash == hash && is_library_item(media))
                .map(|media| media.id);
            let mut commands = Vec::with_capacity(2);
            let media_id = existing.unwrap_or_else(|| {
                let id = Uuid::now_v7();
                commands.push(Command::ImportMedia {
                    media: vec![MediaRef {
                        id,
                        kind: MediaKind::Image,
                        location: MediaLocation::ProjectCopy { rel_path },
                        content_hash: hash,
                        probe: MediaProbe {
                            duration_us: 0,
                            width,
                            height,
                            rotation: 0,
                            fps: 0.0,
                            has_audio: false,
                            codec: "svg".into(),
                            color: ColorRange::Sdr,
                        },
                        captured_at: None,
                        name: None,
                    }],
                });
                id
            });
            let mut overlay = new_overlay(OverlayKind::Image { asset: media_id }, at_us, project);
            if let (Some(x), Some(y)) = (x, y) {
                overlay.base.position = Vec2 { x, y };
            }
            commands.push(Command::AddOverlay {
                overlay: Box::new(overlay),
            });
            session.execute(&Command::Group {
                label: "add library item".into(),
                commands,
            })?;
            Ok(view_of(session))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_found_by_words_and_drawn_small() {
        let arrows = library_items(String::new(), Some("arrows".into()));
        assert!(arrows.len() >= 15);
        assert!(arrows.iter().all(|item| item.category == "arrows"));
        let picture = library_thumbnail(arrows[0].name.clone(), 48).unwrap();
        assert!(picture.width <= 48 && picture.height <= 48);
        assert_eq!(
            picture.rgba.len(),
            (picture.width * picture.height * 4) as usize
        );
        assert!(library_thumbnail("nothing".into(), 48).is_none());
    }

    #[test]
    fn placed_items_are_recognised() {
        let media = |rel_path: &str| MediaRef {
            id: Uuid::nil(),
            kind: MediaKind::Image,
            location: MediaLocation::ProjectCopy {
                rel_path: rel_path.into(),
            },
            content_hash: String::new(),
            probe: MediaProbe {
                duration_us: 0,
                width: 1,
                height: 1,
                rotation: 0,
                fps: 0.0,
                has_audio: false,
                codec: String::new(),
                color: ColorRange::Sdr,
            },
            captured_at: None,
            name: None,
        };
        assert!(is_library_item(&media("library/shapes-star.svg")));
        assert!(!is_library_item(&media("media/1.png")));
    }
}
