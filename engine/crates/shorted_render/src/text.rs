//! Text elements (T-0705): words on their own sprite, in the caption fonts, with the four
//! styles of FR-ENH-03. Drawn at one pixel per canvas unit, like every overlay.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Wrap};

use crate::captions::{CaptionPainter, LINE_HEIGHT, blend};

/// Room around the words, in font sizes, so outlines and shadows are not cut.
const PLAIN_PADDING: f32 = 0.12;
const CHIP_PADDING_X: f32 = 0.45;
const CHIP_PADDING_Y: f32 = 0.22;
const CHIP_RADIUS: f32 = 0.4;
const OUTLINE_REACH: f32 = 0.06;
const SHADOW_OFFSET: f32 = 0.06;
const SHADOW: [u8; 4] = [0, 0, 0, 150];
const INK: [u8; 4] = [17, 17, 20, 255];
const PAPER: [u8; 4] = [255, 255, 255, 255];
/// Measured sizes kept for the selection box, which asks every frame.
const MEASURES_KEPT: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decoration {
    Plain,
    Chip,
    Outline,
    Shadow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLook {
    pub font: String,
    pub size: f32,
    pub color: [u8; 4],
    pub uppercase: bool,
    pub decoration: Decoration,
}

impl TextLook {
    fn shown(&self, content: &str) -> String {
        if self.uppercase {
            content.to_uppercase()
        } else {
            content.to_string()
        }
    }

    fn padding(&self) -> (f32, f32) {
        match self.decoration {
            Decoration::Chip => (self.size * CHIP_PADDING_X, self.size * CHIP_PADDING_Y),
            _ => (self.size * PLAIN_PADDING, self.size * PLAIN_PADDING),
        }
    }

    fn key(&self, content: &str) -> String {
        format!("{content}\u{0}{self:?}")
    }
}

/// Black or white, whichever reads better on `background`.
fn readable_on(background: [u8; 4]) -> [u8; 4] {
    let luma = 0.2126 * f32::from(background[0])
        + 0.7152 * f32::from(background[1])
        + 0.0722 * f32::from(background[2]);
    if luma > 150.0 { INK } else { PAPER }
}

fn family(font: &str) -> &'static str {
    if font == "Inter" {
        "Inter"
    } else {
        "Space Grotesk"
    }
}

/// Lays the words out, centred, one paragraph per line of `content`.
fn layout(fonts: &mut FontSystem, content: &str, look: &TextLook) -> (Buffer, f32, f32) {
    let size = look.size.max(1.0);
    let mut buffer = Buffer::new(fonts, Metrics::new(size, size * LINE_HEIGHT));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, None);
    let attrs = Attrs::new().family(Family::Name(family(&look.font)));
    let text = look.shown(content);
    buffer.set_text(&text, &attrs, Shaping::Advanced, Some(Align::Center));
    buffer.shape_until_scroll(fonts, false);
    let width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max);
    // Centring needs the width of the widest line.
    buffer.set_size(Some(width.ceil()), None);
    buffer.shape_until_scroll(fonts, false);
    let lines = buffer.layout_runs().count().max(1) as f32;
    (buffer, width, lines * size * LINE_HEIGHT)
}

impl CaptionPainter {
    /// The size of a text element's picture, in canvas units.
    pub fn measure_text(&mut self, content: &str, look: &TextLook) -> (u32, u32) {
        let (_, width, height) = layout(&mut self.fonts, content, look);
        let (pad_x, pad_y) = look.padding();
        (
            (width + pad_x * 2.0).ceil().max(1.0) as u32,
            (height + pad_y * 2.0).ceil().max(1.0) as u32,
        )
    }

    /// RGBA of a text element, transparent around it.
    pub fn paint_text(&mut self, content: &str, look: &TextLook) -> (u32, u32, Vec<u8>) {
        let (mut buffer, text_width, text_height) = layout(&mut self.fonts, content, look);
        let (pad_x, pad_y) = look.padding();
        let width = (text_width + pad_x * 2.0).ceil().max(1.0) as u32;
        let height = (text_height + pad_y * 2.0).ceil().max(1.0) as u32;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let (left, top) = (pad_x.round() as i32, pad_y.round() as i32);
        let frame = width as i32;

        let text_color = match look.decoration {
            Decoration::Chip => {
                chip(
                    &mut pixels,
                    width,
                    height,
                    look.size * CHIP_RADIUS,
                    look.color,
                );
                readable_on(look.color)
            }
            _ => look.color,
        };
        let mut draw = |fonts: &mut FontSystem,
                        cache: &mut cosmic_text::SwashCache,
                        pixels: &mut Vec<u8>,
                        dx: i32,
                        dy: i32,
                        tone: [u8; 4]| {
            let base = Color::rgba(tone[0], tone[1], tone[2], tone[3]);
            buffer.draw(fonts, cache, base, |x, y, w, h, glyph| {
                let alpha = (u32::from(glyph.a()) * u32::from(tone[3]) / 255) as u8;
                for row in 0..h as i32 {
                    for column in 0..w as i32 {
                        let (px, py) = (left + dx + x + column, top + dy + y + row);
                        if py < height as i32 {
                            blend(pixels, frame, px, py, [tone[0], tone[1], tone[2], alpha]);
                        }
                    }
                }
            });
        };
        let reach = (look.size * OUTLINE_REACH).round().max(1.0) as i32;
        match look.decoration {
            Decoration::Outline => {
                for (dx, dy) in [
                    (-reach, 0),
                    (reach, 0),
                    (0, -reach),
                    (0, reach),
                    (-reach, -reach),
                    (reach, reach),
                    (-reach, reach),
                    (reach, -reach),
                ] {
                    draw(&mut self.fonts, &mut self.cache, &mut pixels, dx, dy, INK);
                }
            }
            Decoration::Shadow => {
                let offset = (look.size * SHADOW_OFFSET).round().max(1.0) as i32;
                draw(
                    &mut self.fonts,
                    &mut self.cache,
                    &mut pixels,
                    offset,
                    offset,
                    SHADOW,
                );
            }
            Decoration::Plain | Decoration::Chip => {}
        }
        draw(
            &mut self.fonts,
            &mut self.cache,
            &mut pixels,
            0,
            0,
            text_color,
        );
        (width, height, pixels)
    }
}

/// A rounded rectangle filling the picture, with a soft one-pixel edge.
fn chip(pixels: &mut [u8], width: u32, height: u32, radius: f32, color: [u8; 4]) {
    let (w, h) = (width as f32, height as f32);
    let radius = radius.min(w / 2.0).min(h / 2.0);
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (px - w / 2.0).abs() - (w / 2.0 - radius);
            let dy = (py - h / 2.0).abs() - (h / 2.0 - radius);
            let outside = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
            let cover = (0.5 - outside).clamp(0.0, 1.0);
            let alpha = (f32::from(color[3]) * cover).round() as u8;
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
}

type Measures = (Option<CaptionPainter>, Option<HashMap<String, (u32, u32)>>);
static MEASURES: Mutex<Measures> = Mutex::new((None, None));

/// The size of a text element, from a shared painter: the selection box asks every frame.
pub fn text_size(content: &str, look: &TextLook) -> (u32, u32) {
    let mut guard = MEASURES.lock().unwrap_or_else(PoisonError::into_inner);
    let (painter, sizes) = &mut *guard;
    let sizes = sizes.get_or_insert_with(HashMap::new);
    let key = look.key(content);
    if let Some(size) = sizes.get(&key) {
        return *size;
    }
    let size = painter
        .get_or_insert_with(CaptionPainter::new)
        .measure_text(content, look);
    if sizes.len() >= MEASURES_KEPT {
        sizes.clear();
    }
    sizes.insert(key, size);
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(decoration: Decoration) -> TextLook {
        TextLook {
            font: "Space Grotesk".into(),
            size: 60.0,
            color: [255, 210, 63, 255],
            uppercase: false,
            decoration,
        }
    }

    fn ink(pixels: &[u8]) -> usize {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
    }

    #[test]
    fn the_picture_fits_the_words_and_its_measure() {
        let mut painter = CaptionPainter::new();
        let plain = look(Decoration::Plain);
        let (width, height, pixels) = painter.paint_text("Hello", &plain);
        assert_eq!((width, height), painter.measure_text("Hello", &plain));
        assert_eq!((width, height), text_size("Hello", &plain));
        assert!(width > 100 && width < 300, "{width}");
        assert!(height > 60 && height < 100, "{height}");
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert!(ink(&pixels) > 200);
    }

    #[test]
    fn each_line_of_the_content_is_a_line_of_text() {
        let mut painter = CaptionPainter::new();
        let plain = look(Decoration::Plain);
        let (_, one) = painter.measure_text("Hello", &plain);
        let (_, two) = painter.measure_text("Hello\nworld", &plain);
        assert!(two as f32 > one as f32 * 1.7, "{one} {two}");
    }

    #[test]
    fn a_chip_fills_behind_dark_words() {
        let mut painter = CaptionPainter::new();
        let (width, height, pixels) = painter.paint_text("Hi", &look(Decoration::Chip));
        let corner = &pixels[..4];
        assert_eq!(corner[3], 0, "the corner is rounded");
        let edge = ((height / 2 * width + 3) * 4) as usize;
        assert_eq!(&pixels[edge..edge + 4], &[255, 210, 63, 255]);
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[..3] == INK[..3]));
    }

    #[test]
    fn outline_and_shadow_add_dark_ink() {
        let mut painter = CaptionPainter::new();
        let dark = |pixels: &[u8]| {
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0 && p[0] < 60)
                .count()
        };
        let (_, _, plain) = painter.paint_text("Hi", &look(Decoration::Plain));
        let (_, _, outline) = painter.paint_text("Hi", &look(Decoration::Outline));
        let (_, _, shadow) = painter.paint_text("Hi", &look(Decoration::Shadow));
        assert_eq!(dark(&plain), 0);
        assert!(dark(&outline) > 100);
        assert!(dark(&shadow) > 50);
    }

    #[test]
    fn the_same_text_draws_the_same_pixels_twice() {
        let mut first = CaptionPainter::new();
        let mut second = CaptionPainter::new();
        let outline = look(Decoration::Outline);
        assert_eq!(
            first.paint_text("Same\nwords", &outline),
            second.paint_text("Same\nwords", &outline)
        );
    }
}
