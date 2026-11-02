//! Rules for captions that stay the same on every platform: emphasis, presets, the SRT file.

use std::collections::HashMap;
use std::fmt::Write;

use crate::evaluate::clip_positions;
use crate::model::*;

const MIN_WORD_LENGTH: usize = 4;
/// Emphasise at most one word in this many (docs/07 §7: about 20 %).
const WORDS_PER_EMPHASIS: usize = 5;
const REPEAT_SCORE: i32 = 2;
const CAPITAL_SCORE: i32 = 1;
const NUMBER_SCORE: i32 = 3;
const LENGTH_SCORE: i32 = 2;
const GROUP_FOR_HEURISTIC: usize = 3;

const STOP_WORDS: &[&str] = &[
    // en
    "this", "that", "with", "have", "from", "they", "will", "would", "there", "their", "what",
    "about", "which", "when", "your", "them", "were", "been", "just", "like", "because", "very",
    // fr
    "dans", "pour", "avec", "cette", "mais", "nous", "vous", "elle", "elles", "sont", "comme",
    "plus", "tout", "tous", "être", "avoir", "fait", "leur", "leurs", "aussi", "alors", "donc",
    // es
    "para", "como", "pero", "este", "esta", "esto", "esos", "estas", "porque", "cuando", "muy",
    "también", "desde", "sobre", "entre", "hasta", "donde", "todo", "todos", // pt
    "isso", "esse", "essa", "mais", "mas", "você", "vocês", "quando", "porque", "muito", "também",
    "sobre", "entre", "até", "onde", "tudo", "todos",
];

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn score(word: &Word, counts: &HashMap<String, usize>) -> i32 {
    let plain = bare(&word.text);
    if plain.is_empty() || STOP_WORDS.contains(&plain.as_str()) {
        return 0;
    }
    if plain.chars().any(|c| c.is_ascii_digit()) {
        return NUMBER_SCORE;
    }
    if plain.chars().count() < MIN_WORD_LENGTH {
        return 0;
    }
    let mut total = LENGTH_SCORE;
    if word.text.chars().next().is_some_and(char::is_uppercase) {
        total += CAPITAL_SCORE;
    }
    if counts.get(&plain).is_some_and(|count| *count > 1) {
        total += REPEAT_SCORE;
    }
    total
}

/// Marks key words: at most one per group of words and about one word in five. A choice the user
/// made is never changed.
pub fn apply_auto_emphasis(transcript: &mut Transcript) {
    let mut counts = HashMap::new();
    let words = transcript.segments.iter().flat_map(|s| &s.words);
    for word in words {
        *counts.entry(bare(&word.text)).or_insert(0) += 1;
    }
    let total: usize = transcript.segments.iter().map(|s| s.words.len()).sum();
    let mut left = total / WORDS_PER_EMPHASIS;
    if left == 0 && total > 0 {
        left = 1;
    }
    let mut picks: Vec<(i32, usize, usize)> = Vec::new();
    for (segment_index, segment) in transcript.segments.iter().enumerate() {
        for (offset, group) in segment.words.chunks(GROUP_FOR_HEURISTIC).enumerate() {
            let best = group
                .iter()
                .enumerate()
                .map(|(index, word)| (score(word, &counts), index))
                .max_by_key(|(points, index)| (*points, std::cmp::Reverse(*index)));
            if let Some((points, index)) = best.filter(|(points, _)| *points > 0) {
                picks.push((points, segment_index, offset * GROUP_FOR_HEURISTIC + index));
            }
        }
    }
    picks.sort_by_key(|(points, ..)| std::cmp::Reverse(*points));
    for segment in &mut transcript.segments {
        for word in &mut segment.words {
            if matches!(word.emphasis, EmphasisState::Auto(_)) {
                word.emphasis = EmphasisState::Auto(false);
            }
        }
    }
    for (_, segment, word) in picks {
        if left == 0 {
            break;
        }
        let word = &mut transcript.segments[segment].words[word];
        if matches!(word.emphasis, EmphasisState::Auto(_)) {
            word.emphasis = EmphasisState::Auto(true);
            left -= 1;
        }
    }
}

/// The settings a preset stands for. Colours and the position stay the user's.
pub fn apply_preset(settings: &mut CaptionSettings, preset: CaptionPreset) {
    settings.preset = preset;
    let (grouping, animation, uppercase) = match preset {
        CaptionPreset::WordByWord => (Grouping::OneWord, CaptionAnimation::Pop, true),
        CaptionPreset::WordGroups => (Grouping::UpTo(3), CaptionAnimation::Pop, true),
        CaptionPreset::Sentence => (Grouping::Sentence, CaptionAnimation::Fade, false),
        CaptionPreset::Karaoke => (Grouping::Sentence, CaptionAnimation::None, false),
        CaptionPreset::Outline => (Grouping::UpTo(3), CaptionAnimation::Fade, true),
        CaptionPreset::Box => (Grouping::Sentence, CaptionAnimation::Fade, false),
        CaptionPreset::Bold => (Grouping::OneWord, CaptionAnimation::Pop, true),
        CaptionPreset::Minimal => (Grouping::Sentence, CaptionAnimation::None, false),
    };
    settings.grouping = grouping;
    settings.animation = animation;
    settings.style.uppercase = uppercase;
}

fn timestamp(us: i64) -> String {
    let ms = us.max(0) / 1_000;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}

fn visible_words(transcript: &Transcript) -> Vec<Vec<&Word>> {
    transcript
        .segments
        .iter()
        .filter(|segment| !segment.hidden)
        .map(|segment| segment.words.iter().filter(|w| !w.deleted).collect())
        .collect()
}

/// Cues on the project's time, from source times, for the words that stay in the edit.
fn cues_of(
    transcript: &Transcript,
    grouping: Grouping,
    place: impl Fn(i64) -> Option<i64>,
    end_us: i64,
    cues: &mut Vec<(i64, i64, String)>,
) {
    for words in visible_words(transcript) {
        let words: Vec<&Word> = words
            .into_iter()
            .filter(|word| place(word.start_us).is_some())
            .collect();
        let size = match grouping {
            Grouping::OneWord => 1,
            Grouping::UpTo(count) => usize::from(count).max(1),
            Grouping::Sentence => words.len().max(1),
        };
        for group in words.chunks(size) {
            let (Some(first), Some(last)) = (group.first(), group.last()) else {
                continue;
            };
            let Some(start) = place(first.start_us) else {
                continue;
            };
            let end = place(last.end_us).unwrap_or(end_us);
            let text: Vec<&str> = group.iter().map(|word| word.text.as_str()).collect();
            cues.push((start, end.min(end_us), text.join(" ")));
        }
    }
}

/// The captions as an SRT file, matching what the video shows.
pub fn srt(project: &Project) -> String {
    let settings = &project.captions;
    let mut cues = Vec::new();
    for source in &settings.sources {
        match source {
            CaptionSource::OriginalSound => {
                let positions = clip_positions(project);
                for ((_, start, end), clip) in positions.iter().zip(&project.timeline.clips) {
                    let Some(transcript) = clip.transcript.as_ref() else {
                        continue;
                    };
                    if !clip.original_audio.transcribe {
                        continue;
                    }
                    let place = |source_us: i64| {
                        (clip.source_in_us..clip.source_out_us)
                            .contains(&source_us)
                            .then(|| {
                                let local = (source_us - clip.source_in_us) as f64;
                                start + (local / f64::from(clip.speed)).round() as i64
                            })
                    };
                    cues_of(transcript, settings.grouping, place, *end, &mut cues);
                }
            }
            CaptionSource::AudioItem(id) => {
                let item = project.timeline.audio.iter().find(|a| a.id == *id);
                let Some((item, transcript)) =
                    item.and_then(|item| Some((item, item.transcript.as_ref()?)))
                else {
                    continue;
                };
                let place = |source_us: i64| {
                    (item.source_in_us..item.source_out_us)
                        .contains(&source_us)
                        .then(|| item.start_us + source_us - item.source_in_us)
                };
                cues_of(
                    transcript,
                    settings.grouping,
                    place,
                    item.end_us(),
                    &mut cues,
                );
            }
        }
    }
    cues.sort_by_key(|(start, ..)| *start);
    let mut out = String::new();
    for (index, (start, end, text)) in cues.iter().enumerate() {
        let _ = write!(
            out,
            "{}\n{} --> {}\n{text}\n\n",
            index + 1,
            timestamp(*start),
            timestamp(*end)
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn word(text: &str, start_us: i64, end_us: i64) -> Word {
        Word {
            text: text.into(),
            original_text: text.into(),
            start_us,
            end_us,
            confidence: 1.0,
            emphasis: EmphasisState::Auto(false),
            deleted: false,
        }
    }

    fn transcript(texts: &[&str]) -> Transcript {
        Transcript {
            language: "en".into(),
            segments: vec![Segment {
                id: Uuid::nil(),
                hidden: false,
                words: texts
                    .iter()
                    .enumerate()
                    .map(|(i, text)| word(text, i as i64 * 100_000, (i as i64 + 1) * 100_000))
                    .collect(),
            }],
        }
    }

    fn marked(transcript: &Transcript) -> Vec<&str> {
        transcript.segments[0]
            .words
            .iter()
            .filter(|w| {
                matches!(
                    w.emphasis,
                    EmphasisState::Auto(true) | EmphasisState::User(true)
                )
            })
            .map(|w| w.text.as_str())
            .collect()
    }

    #[test]
    fn numbers_and_key_words_beat_stop_words() {
        let mut spoken = transcript(&["this", "costs", "100", "dollars", "with", "that"]);
        apply_auto_emphasis(&mut spoken);
        assert_eq!(marked(&spoken), ["100"]);
    }

    #[test]
    fn about_a_fifth_of_words_at_most_and_one_per_group() {
        let texts: Vec<String> = (0..40).map(|i| format!("keyword{i}x")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let mut spoken = transcript(&refs);
        apply_auto_emphasis(&mut spoken);
        let words = &spoken.segments[0].words;
        assert!(marked(&spoken).len() <= 8);
        for group in words.chunks(3) {
            let count = group
                .iter()
                .filter(|w| matches!(w.emphasis, EmphasisState::Auto(true)))
                .count();
            assert!(count <= 1);
        }
    }

    #[test]
    fn the_user_choice_survives() {
        let mut spoken = transcript(&["this", "wonderful", "cake"]);
        spoken.segments[0].words[1].emphasis = EmphasisState::User(false);
        apply_auto_emphasis(&mut spoken);
        assert!(!marked(&spoken).contains(&"wonderful"));
    }

    #[test]
    fn every_preset_sets_a_look() {
        for preset in [
            CaptionPreset::WordByWord,
            CaptionPreset::WordGroups,
            CaptionPreset::Sentence,
            CaptionPreset::Karaoke,
            CaptionPreset::Outline,
            CaptionPreset::Box,
            CaptionPreset::Bold,
            CaptionPreset::Minimal,
        ] {
            let mut settings = CaptionSettings {
                vertical_anchor: 0.4,
                ..CaptionSettings::default()
            };
            apply_preset(&mut settings, preset);
            assert_eq!(settings.preset, preset);
            assert_eq!(settings.vertical_anchor, 0.4);
        }
    }

    #[test]
    fn timestamps_use_the_srt_format() {
        assert_eq!(timestamp(0), "00:00:00,000");
        assert_eq!(timestamp(3_723_004_000), "01:02:03,004");
    }

    fn project() -> Project {
        let json = include_str!("../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.transitions.clear();
        project.timeline.clips.truncate(1);
        project.timeline.audio.clear();
        project.captions.sources = vec![CaptionSource::OriginalSound];
        project.captions.grouping = Grouping::UpTo(2);
        let clip = &mut project.timeline.clips[0];
        clip.speed = 1.0;
        clip.source_in_us = 1_000_000;
        clip.source_out_us = 5_000_000;
        clip.original_audio.transcribe = true;
        clip.transcript = Some(Transcript {
            language: "en".into(),
            segments: vec![Segment {
                id: Uuid::nil(),
                hidden: false,
                words: vec![
                    word("cut", 0, 500_000),
                    word("hello", 1_000_000, 1_500_000),
                    word("big", 1_500_000, 2_000_000),
                    word("world", 2_000_000, 2_500_000),
                ],
            }],
        });
        project
    }

    #[test]
    fn srt_cues_land_on_the_project_time() {
        let text = srt(&project());
        assert!(
            text.contains("00:00:00,000 --> 00:00:01,000\nhello big"),
            "{text}"
        );
        assert!(
            text.contains("00:00:01,000 --> 00:00:01,500\nworld"),
            "{text}"
        );
        assert!(!text.contains("cut"));
    }

    #[test]
    fn hidden_phrases_stay_out_of_the_srt() {
        let mut project = project();
        let clip = &mut project.timeline.clips[0];
        clip.transcript.as_mut().unwrap().segments[0].hidden = true;
        assert_eq!(srt(&project), "");
    }

    #[test]
    fn srt_blocks_have_the_standard_shape() {
        let text = srt(&project());
        for block in text.trim_end().split("\n\n") {
            let lines: Vec<_> = block.lines().collect();
            assert!(lines[0].parse::<u32>().is_ok());
            assert_eq!(lines[1].len(), 29);
            assert!(lines[1].contains(" --> "));
            assert!(lines.len() >= 3);
        }
    }
}
