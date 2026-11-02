//! The arrangement prompt (docs/07 §6): the template filled with each clip's timing and words,
//! by alias only. No file names, paths or ids leave the device.

use std::fmt::Write;

use shorted_core::model::{MediaKind, Project};

use crate::plan::{Aliases, MAX_SHORT_US};

pub const ARRANGE_V1: &str = include_str!("../prompts/arrange.v1.md");

/// What the model should do if its plan did not validate: the errors, and its own answer.
pub const REPAIR_INSTRUCTION: &str = "Your plan was refused for these reasons. Return the whole \
corrected plan, as JSON only.";

fn seconds(us: i64) -> String {
    format!("{:.1}", us as f64 / 1e6)
}

/// One line per clip, then one per sentence with its source times.
pub fn clips_block(project: &Project, aliases: &Aliases) -> String {
    let mut block = String::new();
    for clip in &project.timeline.clips {
        let Some(alias) = aliases.clip_alias(clip.id) else {
            continue;
        };
        let media = project.media.iter().find(|m| m.id == clip.media_id);
        let kind = match media.map(|m| m.kind) {
            Some(MediaKind::Video) => "video",
            Some(MediaKind::Image | MediaKind::AnimatedImage) => "picture",
            Some(MediaKind::Audio) => "sound",
            None => "missing",
        };
        let _ = write!(
            block,
            "{alias}: {kind}, source {}–{} s",
            seconds(clip.source_in_us),
            seconds(clip.source_out_us)
        );
        if let Some(when) = media.and_then(|m| m.captured_at.as_deref()) {
            let _ = write!(block, ", captured {when}");
        }
        block.push('\n');
        for segment in clip.transcript.iter().flat_map(|t| &t.segments) {
            let words: Vec<_> = segment.words.iter().filter(|w| !w.deleted).collect();
            let (Some(first), Some(last), Some(name)) = (
                words.first(),
                words.last(),
                aliases.segment_alias(segment.id),
            ) else {
                continue;
            };
            let text: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
            let _ = writeln!(
                block,
                "  {name} [{}–{} s]: {}",
                seconds(first.start_us),
                seconds(last.end_us),
                text.join(" ")
            );
        }
    }
    block
}

/// The system and user messages for one arrangement request.
pub fn arrange_messages(
    project: &Project,
    aliases: &Aliases,
    ui_language: &str,
) -> (String, String) {
    let system = ARRANGE_V1
        .split("Clips:")
        .next()
        .unwrap_or(ARRANGE_V1)
        .replace("{max_seconds}", &(MAX_SHORT_US / 1_000_000).to_string())
        .replace("{ui_language}", ui_language)
        .trim()
        .to_string();
    let user = format!("Clips:\n{}", clips_block(project, aliases));
    (system, user)
}

/// The language name the model is asked to write reasons in.
pub fn language_name(code: &str) -> &'static str {
    match code {
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Portuguese",
        _ => "English",
    }
}
