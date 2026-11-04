//! The Shorted library (T-0703): project-made SVG elements built into the engine, and SVG
//! drawing for them and for imported SVG files.

use resvg::{tiny_skia, usvg};

pub use crate::library_index::ASSETS;

/// One element of the library. `tags` are search words, separated by spaces.
#[derive(Debug)]
pub struct LibraryAsset {
    pub name: &'static str,
    pub category: &'static str,
    pub tags: &'static str,
    pub svg: &'static str,
}

/// Pictures larger than this are drawn smaller: an overlay never needs more than the frame.
const MAX_SIDE: u32 = 2048;

pub fn find(name: &str) -> Option<&'static LibraryAsset> {
    ASSETS.iter().find(|asset| asset.name == name)
}

/// Items whose name, category or tags contain every word of `query`, in library order.
pub fn search(query: &str, category: Option<&str>) -> Vec<&'static LibraryAsset> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    ASSETS
        .iter()
        .filter(|asset| category.is_none_or(|wanted| asset.category == wanted))
        .filter(|asset| {
            words.iter().all(|word| {
                asset.name.contains(word.as_str()) || asset.tags.contains(word.as_str())
            })
        })
        .collect()
}

fn tree(data: &[u8]) -> Option<usvg::Tree> {
    usvg::Tree::from_data(data, &usvg::Options::default()).ok()
}

/// The size an SVG declares, in its own units.
pub fn svg_size(data: &[u8]) -> Option<(u32, u32)> {
    let size = tree(data)?.size();
    Some((size.width().ceil() as u32, size.height().ceil() as u32))
}

/// Straight-alpha RGBA pixels of an SVG, at most `max_side` pixels on its longer side.
pub fn rasterize_svg(data: &[u8], max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = tree(data)?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    let scale = (max_side.min(MAX_SIDE) as f32 / longest).min(1.0);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Some((width, height, rgba))
}

/// Pixels of an SVG at its declared size.
pub fn rasterize_svg_full(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    rasterize_svg(data, MAX_SIDE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATEGORIES: [&str; 8] = [
        "badges",
        "arrows",
        "stickers",
        "shapes",
        "doodles",
        "bubbles",
        "highlights",
        "labels",
    ];

    #[test]
    fn the_library_has_at_least_120_items_in_eight_categories() {
        assert!(ASSETS.len() >= 120);
        for category in CATEGORIES {
            assert!(
                ASSETS.iter().any(|asset| asset.category == category),
                "{category}"
            );
        }
        assert!(
            ASSETS
                .iter()
                .all(|asset| CATEGORIES.contains(&asset.category))
        );
    }

    #[test]
    fn every_item_draws_something() {
        for asset in ASSETS {
            let (width, height, pixels) = rasterize_svg(asset.svg.as_bytes(), 64)
                .unwrap_or_else(|| panic!("{} does not parse", asset.name));
            assert!(width > 0 && height > 0);
            let opaque = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0)
                .count();
            assert!(opaque > 20, "{} is empty", asset.name);
        }
    }

    #[test]
    fn search_matches_every_word_and_the_category() {
        let found = search("speech violet", None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "bubbles-speech-violet");
        assert!(search("", Some("arrows")).len() >= 15);
        assert!(search("heart", Some("arrows")).is_empty());
        assert!(search("HEART", None).len() >= 3);
    }

    #[test]
    fn a_circle_keeps_its_size_and_clear_corners() {
        let asset = find("shapes-circle").unwrap();
        assert_eq!(svg_size(asset.svg.as_bytes()), Some((300, 300)));
        let (width, _, pixels) = rasterize_svg_full(asset.svg.as_bytes()).unwrap();
        assert_eq!(width, 300);
        assert_eq!(pixels[3], 0);
        let middle = ((150 * 300 + 150) * 4) as usize;
        assert_eq!(&pixels[middle..middle + 4], &[0xFF, 0xD2, 0x3F, 0xFF]);
    }
}
