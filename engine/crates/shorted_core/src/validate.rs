//! Document validation. A project from disk or a bundle is untrusted (docs/08): every limit of
//! NFR §8 is checked here, plus ranges and references.

use std::collections::{HashMap, HashSet};

use crate::error::ModelError;
use crate::model::*;

pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
/// A journal line carries one command, at most a whole list of clips or a group.
pub const MAX_JOURNAL_LINE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CLIPS: usize = 100;
pub const MAX_OVERLAYS: usize = 200;
pub const MAX_AUDIO_TRACKS: usize = 8;
pub const MAX_MEDIA: usize = 500;
pub const MAX_AUDIO_ITEMS: usize = 500;
pub const MAX_KEYS_PER_PROPERTY: usize = 2_000;
pub const MAX_PATH_SAMPLES: usize = 20_000;
pub const MAX_TRANSCRIPT_WORDS: usize = 20_000;
pub const MAX_TEXT_BYTES: usize = 10_000;
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 2.0;
pub const MIN_TRANSITION_US: i64 = 100_000;
pub const MAX_TRANSITION_US: i64 = 1_500_000;
/// Far above any real source; bounds arithmetic on untrusted times.
pub const MAX_TIME_US: i64 = 86_400_000_000;

fn check(condition: bool, what: &'static str) -> Result<(), ModelError> {
    if condition {
        Ok(())
    } else {
        Err(ModelError::Invalid { what })
    }
}

fn limit(what: &'static str, found: usize, limit: usize) -> Result<(), ModelError> {
    if found > limit {
        Err(ModelError::TooMany { what, found, limit })
    } else {
        Ok(())
    }
}

fn time(value: i64, what: &'static str) -> Result<(), ModelError> {
    check((0..=MAX_TIME_US).contains(&value), what)
}

fn color(value: &str) -> Result<(), ModelError> {
    let bytes = value.as_bytes();
    let valid =
        bytes.len() == 9 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit);
    check(valid, "colour")
}

fn text(value: &str, what: &'static str) -> Result<(), ModelError> {
    limit(what, value.len(), MAX_TEXT_BYTES)
}

fn unique(ids: impl Iterator<Item = Id>) -> Result<HashSet<Id>, ModelError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ModelError::DuplicateId(id));
        }
    }
    Ok(seen)
}

fn known(ids: &HashSet<Id>, id: Id) -> Result<(), ModelError> {
    if ids.contains(&id) {
        Ok(())
    } else {
        Err(ModelError::UnknownId(id))
    }
}

fn style(value: &TextStyle) -> Result<(), ModelError> {
    text(&value.font, "font name")?;
    check(value.size.is_finite() && value.size > 0.0, "text size")?;
    color(&value.color)
}

fn transcript(value: &Transcript) -> Result<(), ModelError> {
    text(&value.language, "transcript language")?;
    let words = value.segments.iter().map(|s| s.words.len()).sum();
    limit("transcript words", words, MAX_TRANSCRIPT_WORDS)?;
    for word in value.segments.iter().flat_map(|s| &s.words) {
        text(&word.text, "word")?;
        text(&word.original_text, "word")?;
        time(word.start_us, "word start")?;
        time(word.end_us, "word end")?;
        check(word.start_us <= word.end_us, "word range")?;
        check((0.0..=1.0).contains(&word.confidence), "word confidence")?;
    }
    Ok(())
}

fn clip(item: &ClipItem, media: &HashMap<Id, MediaKind>) -> Result<(), ModelError> {
    let kind = *media
        .get(&item.media_id)
        .ok_or(ModelError::UnknownId(item.media_id))?;
    check(kind != MediaKind::Audio, "clip media kind")?;
    check(
        item.image_motion.is_none() || kind == MediaKind::Image,
        "image motion",
    )?;
    time(item.source_in_us, "clip start")?;
    time(item.source_out_us, "clip end")?;
    check(item.source_in_us < item.source_out_us, "clip range")?;
    check((MIN_SPEED..=MAX_SPEED).contains(&item.speed), "clip speed")?;
    check(
        item.crop.scale.is_finite() && item.crop.scale > 0.0,
        "crop scale",
    )?;
    let audio = &item.original_audio;
    check((0.0..=4.0).contains(&audio.volume), "clip volume")?;
    time(audio.fade_in_us, "fade in")?;
    time(audio.fade_out_us, "fade out")?;
    item.transcript.as_ref().map_or(Ok(()), transcript)
}

fn audio_item(
    item: &AudioItem,
    media: &HashMap<Id, MediaKind>,
    tracks: &HashSet<Id>,
) -> Result<(), ModelError> {
    let kind = *media
        .get(&item.media_id)
        .ok_or(ModelError::UnknownId(item.media_id))?;
    // A video's own sound can be taken out of its clip (detach).
    check(
        matches!(kind, MediaKind::Audio | MediaKind::Video),
        "audio media kind",
    )?;
    if let AudioTrackRef::User(track) = item.track {
        known(tracks, track)?;
    }
    time(item.start_us, "audio start")?;
    time(item.source_in_us, "audio source start")?;
    time(item.source_out_us, "audio source end")?;
    check(item.source_in_us < item.source_out_us, "audio range")?;
    check((0.0..=4.0).contains(&item.volume), "audio volume")?;
    time(item.fade_in_us, "fade in")?;
    time(item.fade_out_us, "fade out")?;
    check(
        item.ducking.is_none() || item.role == AudioRole::Music,
        "ducking",
    )?;
    item.transcript.as_ref().map_or(Ok(()), transcript)
}

/// Items on one track never overlap (docs/06 §3).
fn no_overlaps(items: &[AudioItem]) -> Result<(), ModelError> {
    let mut by_track: HashMap<AudioTrackRef, Vec<(i64, i64)>> = HashMap::new();
    for item in items {
        by_track
            .entry(item.track)
            .or_default()
            .push((item.start_us, item.end_us()));
    }
    for ranges in by_track.values_mut() {
        ranges.sort_unstable();
        check(
            ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0),
            "audio overlap",
        )?;
    }
    Ok(())
}

fn overlay(item: &OverlayItem, media: &HashSet<Id>) -> Result<(), ModelError> {
    time(item.start_us, "overlay start")?;
    time(item.end_us, "overlay end")?;
    check(item.start_us < item.end_us, "overlay range")?;
    check(
        item.base.scale.is_finite() && item.base.scale > 0.0,
        "overlay scale",
    )?;
    check((0.0..=1.0).contains(&item.base.opacity), "overlay opacity")?;
    match &item.kind {
        OverlayKind::Image { asset } | OverlayKind::AnimatedImage { asset } => {
            known(media, *asset)?
        }
        OverlayKind::Video {
            media: id,
            source_in_us,
        } => {
            known(media, *id)?;
            time(*source_in_us, "video element start")?;
        }
        OverlayKind::Text {
            content,
            style: value,
        } => {
            text(content, "overlay text")?;
            style(value)?;
        }
        OverlayKind::Shape { fill, .. } => color(fill)?,
    }
    if let Some(tint) = &item.tint {
        color(tint)?;
    }
    let mut properties = HashSet::new();
    for track in &item.keyframes {
        check(
            properties.insert(track.property as u8),
            "duplicate keyframe property",
        )?;
        limit("keyframes", track.keys.len(), MAX_KEYS_PER_PROPERTY)?;
        check(
            track
                .keys
                .windows(2)
                .all(|pair| pair[0].t_us < pair[1].t_us),
            "keyframe order",
        )?;
        for key in &track.keys {
            time(key.t_us, "keyframe time")?;
        }
    }
    if let Some(path) = &item.path {
        limit("path samples", path.samples.len(), MAX_PATH_SAMPLES)?;
        check((0.0..=1.0).contains(&path.smoothing), "path smoothing")?;
        check(
            path.samples
                .windows(2)
                .all(|pair| pair[0].t_us < pair[1].t_us),
            "path order",
        )?;
        for sample in &path.samples {
            time(sample.t_us, "path time")?;
        }
    }
    for anim in [&item.anim_in, &item.anim_loop, &item.anim_out]
        .into_iter()
        .flatten()
    {
        time(anim.duration_us, "preset duration")?;
    }
    Ok(())
}

/// Paths from a project file are untrusted: a copy must stay inside the project folder.
fn media_location(location: &MediaLocation) -> Result<(), ModelError> {
    if let MediaLocation::ProjectCopy { rel_path } = location {
        let inside = !rel_path.is_empty()
            && !rel_path.starts_with(['/', '\\'])
            && !rel_path.contains(':')
            && rel_path.split(['/', '\\']).all(|part| part != "..");
        check(inside, "media path")?;
    }
    Ok(())
}

impl Project {
    /// Checks the whole document. Called on load and by commands that replace whole values.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version > SCHEMA_VERSION {
            return Err(ModelError::NewerSchema {
                found: self.schema_version,
            });
        }
        if self.schema_version < SCHEMA_VERSION {
            return Err(ModelError::OlderSchema {
                found: self.schema_version,
            });
        }
        text(&self.title, "title")?;
        check(
            self.canvas.width == CANVAS_WIDTH && self.canvas.height == CANVAS_HEIGHT,
            "canvas",
        )?;
        check(matches!(self.canvas.fps_hint, 30 | 60), "canvas fps")?;

        let timeline = &self.timeline;
        limit("media", self.media.len(), MAX_MEDIA)?;
        limit("clips", timeline.clips.len(), MAX_CLIPS)?;
        limit("overlays", timeline.overlays.len(), MAX_OVERLAYS)?;
        limit(
            "audio tracks",
            timeline.audio_tracks.len(),
            MAX_AUDIO_TRACKS,
        )?;
        limit("audio items", timeline.audio.len(), MAX_AUDIO_ITEMS)?;

        let media_ids = unique(self.media.iter().map(|m| m.id))?;
        let media: HashMap<Id, MediaKind> = self.media.iter().map(|m| (m.id, m.kind)).collect();
        let clips = unique(timeline.clips.iter().map(|c| c.id))?;
        let tracks = unique(timeline.audio_tracks.iter().map(|t| t.id))?;
        let audio = unique(timeline.audio.iter().map(|a| a.id))?;
        unique(timeline.overlays.iter().map(|o| o.id))?;

        for item in &self.media {
            media_location(&item.location)?;
            if let Some(name) = &item.name {
                text(name, "media name")?;
            }
        }
        for item in &timeline.clips {
            clip(item, &media)?;
        }
        for item in &timeline.audio {
            audio_item(item, &media, &tracks)?;
        }
        no_overlaps(&timeline.audio)?;
        for item in &timeline.overlays {
            overlay(item, &media_ids)?;
        }
        for track in &timeline.audio_tracks {
            text(&track.name, "track name")?;
        }
        for transition in &timeline.transitions {
            known(&clips, transition.left)?;
            known(&clips, transition.right)?;
            let adjacent = timeline
                .clips
                .windows(2)
                .any(|pair| pair[0].id == transition.left && pair[1].id == transition.right);
            check(adjacent, "transition clips")?;
            check(
                (MIN_TRANSITION_US..=MAX_TRANSITION_US).contains(&transition.duration_us),
                "transition duration",
            )?;
        }
        let captions = &self.captions;
        color(&captions.emphasis_color)?;
        style(&captions.style)?;
        check(
            (0.0..=1.0).contains(&captions.vertical_anchor),
            "caption anchor",
        )?;
        for source in &captions.sources {
            if let CaptionSource::AudioItem(id) = source {
                known(&audio, *id)?;
            }
        }
        Ok(())
    }
}
