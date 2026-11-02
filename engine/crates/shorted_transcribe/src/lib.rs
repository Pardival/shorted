//! Speech model management and whisper inference with word timestamps.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_vendor = "apple")]
pub mod whisper;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_transcribe";

/// A speech model, pinned to one exact file (docs/07 §1).
pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// The default model, quantised.
pub const BASE_MODEL: ModelSpec = ModelSpec {
    file: "ggml-base-q5_1.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
    sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
    bytes: 59_707_625,
};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
const DOWNSAMPLE: usize = 3;
/// A new phrase starts after this silence.
const PHRASE_GAP_US: i64 = 700_000;
const MAX_PHRASE_WORDS: usize = 14;

#[derive(Clone, Debug, PartialEq)]
pub struct SpokenWord {
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
}

/// Interleaved stereo at 48 kHz to mono at 16 kHz, averaging each group of samples.
pub fn to_whisper_input(stereo: &[f32]) -> Vec<f32> {
    let group = shorted_audio::mixer::CHANNELS * DOWNSAMPLE;
    stereo
        .chunks_exact(group)
        .map(|chunk| chunk.iter().sum::<f32>() / group as f32)
        .collect()
}

fn ends_sentence(word: &str) -> bool {
    word.ends_with(['.', '!', '?'])
}

/// Splits words into phrases, at silences and the ends of sentences.
pub fn group_words(words: Vec<SpokenWord>) -> Vec<Vec<SpokenWord>> {
    let mut phrases: Vec<Vec<SpokenWord>> = Vec::new();
    for word in words {
        let starts_new = phrases
            .last()
            .and_then(|phrase| phrase.last())
            .is_none_or(|last| {
                word.start_us - last.end_us > PHRASE_GAP_US || ends_sentence(&last.text)
            })
            || phrases
                .last()
                .is_some_and(|phrase| phrase.len() >= MAX_PHRASE_WORDS);
        if starts_new {
            phrases.push(Vec::new());
        }
        if let Some(phrase) = phrases.last_mut() {
            phrase.push(word);
        }
    }
    phrases
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start_us: i64, end_us: i64) -> SpokenWord {
        SpokenWord {
            text: text.into(),
            start_us,
            end_us,
            confidence: 1.0,
        }
    }

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn links_audio() {
        assert_eq!(shorted_audio::CRATE_NAME, "shorted_audio");
    }

    #[test]
    fn input_becomes_mono_at_a_third_of_the_rate() {
        let stereo = [
            0.0, 0.6, 0.0, 0.6, 0.0, 0.6, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5,
        ];
        let mono = to_whisper_input(&stereo);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.3).abs() < 1e-6);
        assert_eq!(mono[1], 1.0);
    }

    #[test]
    fn phrases_break_at_silences_and_sentences() {
        let phrases = group_words(vec![
            word("one", 0, 100),
            word("two.", 100, 200),
            word("three", 200, 300),
            word("four", 2_000_000, 2_100_000),
        ]);
        let sizes: Vec<_> = phrases.iter().map(Vec::len).collect();
        assert_eq!(sizes, [2, 1, 1]);
    }

    #[test]
    fn long_speech_is_split() {
        let words = (0..30).map(|i| word("a", i, i + 1)).collect();
        assert!(
            group_words(words)
                .iter()
                .all(|p| p.len() <= MAX_PHRASE_WORDS)
        );
    }
}
