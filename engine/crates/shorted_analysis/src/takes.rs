//! Duplicate takes (FR-ARR-05, docs/07 §2): a sentence said again later is a retake; the last
//! complete take stays and the earlier ones go.

use shorted_core::model::{Id, Project, Segment};

use crate::speech::Range;

/// Two sentences are the same take at or above this similarity (1 − normalised edit distance).
pub const SAME_SENTENCE: f32 = 0.8;
/// Sentences this short ("okay", "so") repeat without being retakes.
const MIN_WORDS: usize = 4;
/// Kept around a removed take, unless the next word is closer.
const PADDING_US: i64 = 150_000;

/// Lower case letters and digits, single spaces: punctuation and case say nothing of a take.
pub fn normalise(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 1 − Levenshtein distance / longer length, on characters.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    1.0 - previous[b.len()] as f32 / longest as f32
}

struct Sentence<'a> {
    clip: Id,
    segment: &'a Segment,
    text: String,
    words: usize,
}

/// The stretches of source to remove: every sentence said again later in the timeline, by
/// clip, from its first word to its last.
pub fn duplicate_takes(project: &Project) -> Vec<(Id, Range)> {
    let sentences: Vec<Sentence<'_>> = project
        .timeline
        .clips
        .iter()
        .flat_map(|clip| {
            clip.transcript
                .iter()
                .flat_map(|t| &t.segments)
                .filter(|segment| !segment.hidden)
                .map(move |segment| {
                    let spoken: Vec<&str> = segment
                        .words
                        .iter()
                        .filter(|w| !w.deleted)
                        .map(|w| w.text.as_str())
                        .collect();
                    Sentence {
                        clip: clip.id,
                        segment,
                        text: normalise(&spoken.join(" ")),
                        words: spoken.len(),
                    }
                })
        })
        .collect();
    let mut removed = Vec::new();
    for (index, sentence) in sentences.iter().enumerate() {
        if sentence.words < MIN_WORDS {
            continue;
        }
        // A later take counts only when it is at least as complete.
        let retaken = sentences[index + 1..].iter().any(|later| {
            later.words + 1 >= sentence.words
                && similarity(&sentence.text, &later.text) >= SAME_SENTENCE
        });
        if retaken && let Some(range) = span(sentence.segment, project, sentence.clip) {
            removed.push((sentence.clip, range));
        }
    }
    removed
}

/// From the sentence's first word to its last, padded but never into a neighbouring word.
fn span(segment: &Segment, project: &Project, clip: Id) -> Option<Range> {
    let words: Vec<_> = segment.words.iter().filter(|w| !w.deleted).collect();
    let (first, last) = (words.first()?.start_us, words.last()?.end_us);
    let others = project
        .timeline
        .clips
        .iter()
        .filter(|c| c.id == clip)
        .flat_map(|c| c.transcript.iter().flat_map(|t| &t.segments))
        .filter(|s| s.id != segment.id)
        .flat_map(|s| &s.words);
    let mut from = first - PADDING_US;
    let mut to = last + PADDING_US;
    for word in others {
        if word.end_us <= first {
            from = from.max((word.end_us + first) / 2);
        }
        if word.start_us >= last {
            to = to.min((word.start_us + last) / 2);
        }
    }
    Some((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retakes_are_alike_and_other_sentences_are_not() {
        let first = normalise("So, today I want to show you my coffee setup.");
        let again = normalise("so today I wanna show you my coffee set-up");
        let other = normalise("Then we grind the beans fresh every morning.");
        assert_eq!(first, "so today i want to show you my coffee setup");
        assert!(similarity(&first, &again) >= SAME_SENTENCE);
        assert!(similarity(&first, &other) < 0.5);
        assert_eq!(similarity("", ""), 1.0);
        assert_eq!(similarity("abc", "abc"), 1.0);
    }
}
