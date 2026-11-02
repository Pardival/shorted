//! Pictures of overlays drawn on the CPU, once per look: the compositor places them as sprites.

use shorted_core::model::ShapeKind;

/// Width of the canvas every position is given in (docs/06).
pub const CANVAS_WIDTH: f32 = 1080.0;
pub const CANVAS_HEIGHT: f32 = 1920.0;

/// The size of a shape at scale 1, in canvas units.
pub fn shape_size(shape: ShapeKind) -> (f32, f32) {
    match shape {
        ShapeKind::Rectangle | ShapeKind::Ellipse => (360.0, 360.0),
        ShapeKind::Arrow => (400.0, 200.0),
    }
}

/// How much of the pixel at (`x`, `y`) of a `width` × `height` picture the shape covers, from a
/// signed distance: one pixel of soft edge.
fn coverage(shape: ShapeKind, x: f32, y: f32, width: f32, height: f32) -> f32 {
    let distance = match shape {
        ShapeKind::Rectangle => {
            let dx = (x - width / 2.0).abs() - width / 2.0;
            let dy = (y - height / 2.0).abs() - height / 2.0;
            dx.max(dy)
        }
        ShapeKind::Ellipse => {
            let (rx, ry) = (width / 2.0, height / 2.0);
            let (nx, ny) = ((x - rx) / rx, (y - ry) / ry);
            // Distance to the unit circle, brought back to pixels by the smaller radius.
            ((nx * nx + ny * ny).sqrt() - 1.0) * rx.min(ry)
        }
        ShapeKind::Arrow => arrow_distance(x, y, width, height),
    };
    (0.5 - distance).clamp(0.0, 1.0)
}

/// An arrow pointing right: a shaft over the left 60 %, a head over the rest.
fn arrow_distance(x: f32, y: f32, width: f32, height: f32) -> f32 {
    let middle = height / 2.0;
    let neck = width * 0.6;
    let shaft = {
        let dx = (x - neck / 2.0).abs() - neck / 2.0;
        let dy = (y - middle).abs() - height * 0.15;
        dx.max(dy)
    };
    // The head is a triangle: inside when left of both slanted edges and right of the neck.
    let reach = width - neck;
    let along = (width - x) / reach;
    let edge = (y - middle).abs() - along * middle;
    let slant = edge * reach / (reach * reach + middle * middle).sqrt();
    let head = slant.max(neck - x).max(x - width);
    shaft.min(head)
}

/// RGBA pixels of a shape filled with `fill`, at one pixel per canvas unit.
pub fn shape_pixels(shape: ShapeKind, fill: [u8; 4]) -> (u32, u32, Vec<u8>) {
    let (width, height) = shape_size(shape);
    let (columns, rows) = (width as u32, height as u32);
    let mut pixels = Vec::with_capacity((columns * rows * 4) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let covered = coverage(shape, column as f32 + 0.5, row as f32 + 0.5, width, height);
            let alpha = (f32::from(fill[3]) * covered).round() as u8;
            pixels.extend_from_slice(&[fill[0], fill[1], fill[2], alpha]);
        }
    }
    (columns, rows, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(shape: ShapeKind, x: u32, y: u32) -> u8 {
        let (width, _, pixels) = shape_pixels(shape, [255, 0, 0, 255]);
        pixels[((y * width + x) * 4 + 3) as usize]
    }

    #[test]
    fn a_rectangle_fills_its_box() {
        assert_eq!(alpha_at(ShapeKind::Rectangle, 0, 0), 255);
        assert_eq!(alpha_at(ShapeKind::Rectangle, 359, 180), 255);
    }

    #[test]
    fn an_ellipse_leaves_its_corners_clear() {
        assert_eq!(alpha_at(ShapeKind::Ellipse, 2, 2), 0);
        assert_eq!(alpha_at(ShapeKind::Ellipse, 180, 180), 255);
        assert_eq!(alpha_at(ShapeKind::Ellipse, 180, 1), 255);
    }

    #[test]
    fn an_arrow_has_a_shaft_and_a_head() {
        assert_eq!(alpha_at(ShapeKind::Arrow, 20, 100), 255);
        assert_eq!(alpha_at(ShapeKind::Arrow, 20, 20), 0);
        assert_eq!(alpha_at(ShapeKind::Arrow, 250, 20), 255);
        assert_eq!(alpha_at(ShapeKind::Arrow, 395, 100), 255);
        assert_eq!(alpha_at(ShapeKind::Arrow, 380, 20), 0);
    }
}
