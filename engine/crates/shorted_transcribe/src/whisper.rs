//! Inference with whisper.cpp. Each word comes out as its own segment, with its own timestamps.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{ModelSpec, SpokenWord};

const CENTISECONDS_TO_US: i64 = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("the speech model did not load")]
    Model,
    #[error("transcription failed")]
    Failed,
    #[error("transcription cancelled")]
    Cancelled,
}

/// True only for the exact pinned file.
pub fn verify(path: &Path, spec: &ModelSpec) -> io::Result<bool> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(digest == spec.sha256)
}

fn is_punctuation(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_punctuation())
}

fn is_word(text: &str) -> bool {
    !text.is_empty() && !(text.starts_with('[') && text.ends_with(']'))
}

/// Called by whisper.cpp. `flag` is the `AtomicBool` given as user data.
unsafe extern "C" fn should_abort(flag: *mut c_void) -> bool {
    // SAFETY: see `Transcriber::transcribe`, which keeps the flag alive through the call.
    unsafe { &*flag.cast::<AtomicBool>() }.load(Ordering::Relaxed)
}

/// A loaded model: loading takes seconds, so it is shared across clips.
pub struct Transcriber(WhisperContext);

impl Transcriber {
    pub fn load(model: &Path) -> Result<Self, TranscribeError> {
        let path = model.to_str().ok_or(TranscribeError::Model)?;
        WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map(Self)
            .map_err(|_| TranscribeError::Model)
    }

    /// Words of mono 16 kHz speech, with the language found. `language` skips detection.
    pub fn transcribe(
        &self,
        speech: &[f32],
        language: Option<&str>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(Vec<SpokenWord>, String), TranscribeError> {
        let mut state = self.0.create_state().map_err(|_| TranscribeError::Model)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language.or(Some("auto")));
        params.set_token_timestamps(true);
        params.set_max_len(1);
        params.set_split_on_word(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        // SAFETY: `cancelled` outlives `state.full`, the only caller of the callback. The
        // library's safe wrapper reads its closure back as the wrong type, so it is not used.
        unsafe {
            params.set_abort_callback(Some(should_abort));
            params.set_abort_callback_user_data(Arc::as_ptr(&cancelled).cast_mut().cast());
        }
        let outcome = state.full(params, speech);
        if cancelled.load(Ordering::Relaxed) {
            return Err(TranscribeError::Cancelled);
        }
        outcome.map_err(|_| TranscribeError::Failed)?;

        let mut words = Vec::new();
        for segment in state.as_iter() {
            let text = segment
                .to_str_lossy()
                .map_err(|_| TranscribeError::Failed)?;
            let text = text.trim();
            if !is_word(text) {
                continue;
            }
            // Punctuation arrives as its own segment: it belongs to the word before.
            if let Some(last) = words.last_mut().filter(|_| is_punctuation(text)) {
                let last: &mut SpokenWord = last;
                last.text.push_str(text);
                last.end_us = segment.end_timestamp() * CENTISECONDS_TO_US;
                continue;
            }
            words.push(SpokenWord {
                text: text.to_owned(),
                start_us: segment.start_timestamp() * CENTISECONDS_TO_US,
                end_us: segment.end_timestamp() * CENTISECONDS_TO_US,
                confidence: 1.0 - segment.no_speech_probability(),
            });
        }
        let found = whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .unwrap_or("en")
            .to_owned();
        Ok((words, found))
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::{BASE_MODEL, group_words, to_whisper_input};

    #[test]
    fn only_the_pinned_file_verifies() {
        let path = std::env::temp_dir().join(format!("shorted-model-{}", std::process::id()));
        std::fs::write(&path, b"not a model").unwrap();
        assert!(!verify(&path, &BASE_MODEL).unwrap());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn noise_markers_are_not_words() {
        assert!(is_word("hello"));
        assert!(!is_word("[BLANK_AUDIO]"));
        assert!(!is_word(""));
        assert!(is_punctuation("!"));
        assert!(!is_punctuation("a."));
    }

    #[test]
    #[ignore = "needs the speech model"]
    fn a_raised_flag_stops_inference() {
        let model = std::env::var("SHORTED_WHISPER_MODEL").unwrap();
        let transcriber = Transcriber::load(Path::new(&model)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        let silence = vec![0.0; 160_000];
        let outcome = transcriber.transcribe(&silence, Some("en"), cancelled);
        assert!(matches!(outcome, Err(TranscribeError::Cancelled)));
    }

    /// Needs SHORTED_WHISPER_MODEL. Speech comes from the macOS `say` command.
    #[test]
    #[ignore = "needs the speech model"]
    fn real_speech_gives_timed_words() {
        let model = std::env::var("SHORTED_WHISPER_MODEL").unwrap();
        let wav = std::env::temp_dir().join(format!("shorted-say-{}.wav", std::process::id()));
        let spoken = Command::new("say")
            .args(["-o"])
            .arg(&wav)
            .args(["--data-format=LEF32@48000", "--channels=2"])
            .arg("Hello everyone, welcome to this short video about cooking.")
            .status()
            .unwrap();
        assert!(spoken.success());
        let stereo = shorted_media::apple::decode_audio(&wav).unwrap();
        std::fs::remove_file(&wav).unwrap();

        let transcriber = Transcriber::load(Path::new(&model)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let input = to_whisper_input(&stereo);
        let (words, language) = transcriber.transcribe(&input, None, cancelled).unwrap();
        println!("{language}: {words:?}");
        // The `say` voice follows the Mac's language, and its accent can sway detection.
        assert!(!language.is_empty());
        let text = words
            .iter()
            .map(|w| w.text.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("welcome"), "{text}");
        assert!(text.contains("video"), "{text}");
        let ordered = words
            .windows(2)
            .all(|pair| pair[0].start_us <= pair[1].start_us);
        assert!(ordered);
        assert!(!group_words(words).is_empty());
    }
}
