//! Captions as the frame shows them, shared by the compositor and taps on the preview.

use shorted_core::evaluate::FrameDescription;
use shorted_core::model::{CaptionAnimation, CaptionPreset, Project};
use shorted_render::captions::{parse_color, CaptionLook, CaptionWord};

pub(crate) const TYPEWRITER_US_PER_CHARACTER: i64 = 30_000;

pub(crate) fn caption_look(project: &Project) -> CaptionLook {
    let settings = &project.captions;
    CaptionLook {
        font: settings.style.font.clone(),
        size: settings.style.size,
        color: parse_color(&settings.style.color),
        highlight: parse_color(&settings.emphasis_color),
        uppercase: settings.style.uppercase,
        outline: matches!(
            settings.preset,
            CaptionPreset::Outline | CaptionPreset::Bold
        ),
        boxed: settings.preset == CaptionPreset::Box,
        anchor: settings.vertical_anchor,
    }
}

/// Words as they show at this instant. Typewriter reveals one character at a time.
pub(crate) fn caption_words(
    frame: &FrameDescription,
    karaoke: bool,
    animation: CaptionAnimation,
) -> Vec<CaptionWord> {
    let mut left = if animation == CaptionAnimation::Typewriter {
        (frame.caption_age_us / TYPEWRITER_US_PER_CHARACTER) as usize + 1
    } else {
        usize::MAX
    };
    let mut words = Vec::new();
    for run in &frame.captions {
        if left == 0 {
            break;
        }
        let text: String = run.text.chars().take(left).collect();
        left = left.saturating_sub(run.text.chars().count());
        words.push(CaptionWord {
            text,
            highlighted: if karaoke { run.active } else { run.emphasis },
        });
    }
    words
}
