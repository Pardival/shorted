//! Caption text drawn to a transparent, frame-sized picture, which the compositor then lays over
//! the video. Fonts are embedded, so preview and export can never differ.

use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};

const SPACE_GROTESK: &[u8] = include_bytes!("../../../../app/assets/fonts/SpaceGrotesk.ttf");
const INTER: &[u8] = include_bytes!("../../../../app/assets/fonts/Inter.ttf");

/// Style sizes are for a 1080 wide frame.
const REFERENCE_WIDTH: f32 = 1080.0;
pub(crate) const LINE_HEIGHT: f32 = 1.15;
const TEXT_WIDTH_SHARE: f32 = 0.84;
/// Platform buttons cover the bottom of a short, and its top bar (FR-CAP-05).
const SAFE_TOP: f32 = 0.15;
const SAFE_BOTTOM: f32 = 0.75;
const OUTLINE_REACH: i32 = 5;
const BOX_PADDING: f32 = 24.0;
const BOX_COLOR: [u8; 4] = [0, 0, 0, 170];

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionWord {
    pub text: String,
    pub highlighted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionLook {
    pub font: String,
    pub size: f32,
    pub color: [u8; 4],
    pub highlight: [u8; 4],
    pub uppercase: bool,
    pub outline: bool,
    pub boxed: bool,
    /// Vertical centre of the block, from 0 at the top to 1.
    pub anchor: f32,
}

/// `#RRGGBBAA`, or white for anything else: a project file is untrusted.
pub fn parse_color(value: &str) -> [u8; 4] {
    let digits = value.strip_prefix('#').unwrap_or_default();
    let byte = |at: usize| {
        digits
            .get(at..at + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
    };
    match (byte(0), byte(2), byte(4), byte(6)) {
        (Some(r), Some(g), Some(b), Some(a)) => [r, g, b, a],
        _ => [255; 4],
    }
}

/// Where a block of `height` goes, kept inside the safe zone.
fn block_top(anchor: f32, height: f32, frame: f32) -> f32 {
    let top = (anchor * frame - height / 2.0).max(SAFE_TOP * frame);
    top.min((SAFE_BOTTOM * frame - height).max(SAFE_TOP * frame))
}

pub(crate) fn blend(pixels: &mut [u8], width: i32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width {
        return;
    }
    let at = (y as usize * width as usize + x as usize) * 4;
    let Some(pixel) = pixels.get_mut(at..at + 4) else {
        return;
    };
    // "Over" with straight alpha: a soft edge on a clear pixel keeps its colour, not black.
    let alpha = u32::from(color[3]);
    let below = u32::from(pixel[3]) * (255 - alpha) / 255;
    let total = alpha + below;
    if total == 0 {
        return;
    }
    for channel in 0..3 {
        let mixed = u32::from(color[channel]) * alpha + u32::from(pixel[channel]) * below;
        pixel[channel] = (mixed / total) as u8;
    }
    pixel[3] = total as u8;
}

pub struct CaptionPainter {
    pub(crate) fonts: FontSystem,
    pub(crate) cache: SwashCache,
}

impl Default for CaptionPainter {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptionPainter {
    pub fn new() -> Self {
        let fonts = [
            cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(SPACE_GROTESK)),
            cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(INTER)),
        ];
        Self {
            fonts: FontSystem::new_with_fonts(fonts),
            cache: SwashCache::new(),
        }
    }

    /// RGBA of a `width` by `height` frame, transparent around the text.
    pub fn paint(
        &mut self,
        words: &[CaptionWord],
        look: &CaptionLook,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let scale = width as f32 / REFERENCE_WIDTH;
        let size = (look.size * scale).max(1.0);
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * LINE_HEIGHT));
        let box_width = width as f32 * TEXT_WIDTH_SHARE;
        buffer.set_size(Some(box_width), None);
        buffer.set_wrap(Wrap::Word);

        let family = if look.font == "Inter" {
            "Inter"
        } else {
            "Space Grotesk"
        };
        let attrs = Attrs::new().family(Family::Name(family));
        let color = |rgba: [u8; 4]| Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
        let texts: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let text = if look.uppercase {
                    word.text.to_uppercase()
                } else {
                    word.text.clone()
                };
                if index + 1 < words.len() {
                    format!("{text} ")
                } else {
                    text
                }
            })
            .collect();
        let spans = texts.iter().zip(words).map(|(text, word)| {
            let tone = if word.highlighted {
                look.highlight
            } else {
                look.color
            };
            (text.as_str(), attrs.clone().color(color(tone)))
        });
        buffer.set_rich_text(spans, &attrs, Shaping::Advanced, Some(Align::Center));
        buffer.shape_until_scroll(&mut self.fonts, false);

        let lines = buffer.layout_runs().count() as f32;
        let block_height = lines * size * LINE_HEIGHT;
        let top = block_top(look.anchor, block_height, height as f32);
        let left = (width as f32 - box_width) / 2.0;
        let (left, top) = (left as i32, top as i32);

        if look.boxed {
            let padding = (BOX_PADDING * scale) as i32;
            for y in top - padding..top + block_height as i32 + padding {
                for x in left - padding..left + box_width as i32 + padding {
                    blend(&mut pixels, width as i32, x, y, BOX_COLOR);
                }
            }
        }
        let frame_width = width as i32;
        let mut draw = |fonts: &mut FontSystem,
                        cache: &mut SwashCache,
                        pixels: &mut Vec<u8>,
                        dx: i32,
                        dy: i32,
                        force: Option<[u8; 4]>| {
            buffer.draw(fonts, cache, color(look.color), |x, y, w, h, tone| {
                let rgba = force.map_or([tone.r(), tone.g(), tone.b(), tone.a()], |base| {
                    [base[0], base[1], base[2], tone.a()]
                });
                for row in 0..h as i32 {
                    for column in 0..w as i32 {
                        let (px, py) = (left + dx + x + column, top + dy + y + row);
                        blend(pixels, frame_width, px, py, rgba);
                    }
                }
            });
        };
        if look.outline {
            let reach = ((OUTLINE_REACH as f32 * scale) as i32).max(1);
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
                draw(
                    &mut self.fonts,
                    &mut self.cache,
                    &mut pixels,
                    dx,
                    dy,
                    Some([0, 0, 0, 255]),
                );
            }
        }
        draw(&mut self.fonts, &mut self.cache, &mut pixels, 0, 0, None);
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> CaptionLook {
        CaptionLook {
            font: "Space Grotesk".into(),
            size: 96.0,
            color: [255, 255, 255, 255],
            highlight: [200, 240, 49, 255],
            uppercase: true,
            outline: false,
            boxed: false,
            anchor: 0.7,
        }
    }

    fn word(text: &str, highlighted: bool) -> CaptionWord {
        CaptionWord {
            text: text.into(),
            highlighted,
        }
    }

    fn rows_with_ink(pixels: &[u8], width: usize) -> (usize, usize) {
        let rows: Vec<_> = pixels
            .chunks(width * 4)
            .enumerate()
            .filter(|(_, row)| row.chunks(4).any(|p| p[3] > 0))
            .map(|(index, _)| index)
            .collect();
        (rows[0], rows[rows.len() - 1])
    }

    #[test]
    fn colours_parse_and_bad_ones_fall_back_to_white() {
        assert_eq!(parse_color("#C6F135FF"), [0xC6, 0xF1, 0x35, 0xFF]);
        assert_eq!(parse_color("nonsense"), [255; 4]);
        assert_eq!(parse_color("#12"), [255; 4]);
    }

    #[test]
    fn blocks_stay_inside_the_safe_zone() {
        assert_eq!(block_top(0.0, 100.0, 1000.0), 150.0);
        assert_eq!(block_top(1.0, 100.0, 1000.0), 650.0);
        assert_eq!(block_top(0.5, 100.0, 1000.0), 450.0);
        assert_eq!(block_top(0.5, 900.0, 1000.0), 150.0);
    }

    #[test]
    fn text_lands_near_its_anchor_and_scales_with_the_frame() {
        let mut painter = CaptionPainter::new();
        let pixels = painter.paint(&[word("Hello", false)], &look(), 360, 640);
        let (first, last) = rows_with_ink(&pixels, 360);
        let middle = (first + last) / 2;
        assert!(
            middle.abs_diff((0.7 * 640.0) as usize) < 40,
            "{first} {last}"
        );
    }

    #[test]
    fn a_highlighted_word_takes_its_colour() {
        let mut painter = CaptionPainter::new();
        let pixels = painter.paint(&[word("Hi", true)], &look(), 360, 640);
        let solid = pixels.chunks(4).find(|p| p[3] == 255).unwrap();
        assert_eq!(&solid[..3], &[200, 240, 49]);
    }

    #[test]
    fn long_text_wraps_inside_the_frame() {
        let mut painter = CaptionPainter::new();
        let long = "This is a very long sentence that cannot fit on one line at this size";
        let pixels = painter.paint(&[word(long, false)], &look(), 360, 640);
        let edge = pixels.chunks(360 * 4).any(|row| {
            row[..8 * 4].chunks(4).any(|p| p[3] > 0)
                || row[(360 - 8) * 4..].chunks(4).any(|p| p[3] > 0)
        });
        assert!(!edge, "text touches the frame edges");
    }

    #[test]
    fn an_outline_and_a_box_add_ink() {
        let mut painter = CaptionPainter::new();
        let ink = |pixels: Vec<u8>| pixels.chunks(4).filter(|p| p[3] > 0).count();
        let plain = ink(painter.paint(&[word("Hi", false)], &look(), 360, 640));
        let mut outlined = look();
        outlined.outline = true;
        let outline = ink(painter.paint(&[word("Hi", false)], &outlined, 360, 640));
        let mut boxed = look();
        boxed.boxed = true;
        let filled = ink(painter.paint(&[word("Hi", false)], &boxed, 360, 640));
        assert!(outline > plain && filled > outline);
    }
}
