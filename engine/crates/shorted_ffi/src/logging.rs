//! File logging with rotation. Everything passes through [`Redactor`] before reaching disk, as a
//! safety net: callers must still never log secrets or user content on purpose.

use std::borrow::Cow;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;

use crate::error::EngineError;

const MAX_LOG_FILES: usize = 7;
const REDACTED: &str = "[REDACTED]";

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Provider key shapes (OpenAI, Anthropic, OpenRouter, Google, Groq, xAI), bearer tokens, and
/// fields that hold user text or key material.
#[allow(clippy::expect_used)] // Proven invariant: constant pattern, covered by the tests below.
static SECRET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?i)\b(?:transcript|transcript_text|api_key|apikey|authorization|prompt)(?:=|: ?)"#,
        r#"(?:bearer\s+)?(?:"(?:[^"\\]|\\.)*"|\S+)"#,
        r"|(?i:bearer)\s+[A-Za-z0-9._~+/=-]{8,}",
        r"|\b(?:sk|gsk|xai|pk)[-_][A-Za-z0-9_-]{8,}",
        r"|\bAIza[A-Za-z0-9_-]{20,}",
    ))
    .expect("constant regex is valid")
});

pub fn redact(line: &str) -> Cow<'_, str> {
    SECRET_PATTERN.replace_all(line, REDACTED)
}

/// Writer that redacts each record. `tracing` writes one record per `write` call.
pub struct Redactor<W>(pub W);

impl<W: Write> Write for Redactor<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[derive(Clone)]
struct RedactingMakeWriter<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for RedactingMakeWriter<M> {
    type Writer = Redactor<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        Redactor(self.0.make_writer())
    }
}

/// Starts daily-rotated logs in `dir`, off the calling thread. Later calls have no effect.
pub fn init(dir: &Path) -> Result<(), EngineError> {
    if LOG_GUARD.get().is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("shorted")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .map_err(|error| EngineError::Logging(error.to_string()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(RedactingMakeWriter(writer))
        .try_init()
        .map_err(|error| EngineError::Logging(error.to_string()))?;
    let _ = LOG_GUARD.set(guard);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Buffer {
        type Writer = Self;

        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }

    fn logged(emit: impl FnOnce()) -> String {
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(RedactingMakeWriter(buffer.clone()))
            .finish();
        tracing::subscriber::with_default(subscriber, emit);
        let bytes = buffer.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn api_keys_are_redacted_in_logs() {
        let keys = [
            "sk-abcdefghijklmnopqrstuvwxyz123456",
            "sk-ant-api03-abcdefghijklmnop",
            "sk-or-v1-abcdefghijklmnop",
            "gsk_abcdefghijklmnopqrst",
            "AIzaSyAbcdefghijklmnopqrstuvwxyz123",
        ];
        for key in keys {
            let output = logged(|| tracing::info!("provider rejected {key} today"));
            assert!(!output.contains(key), "{key} leaked: {output}");
            assert!(output.contains(REDACTED), "{output}");
            assert!(output.contains("provider rejected"), "{output}");
        }
    }

    #[test]
    fn transcript_fields_are_redacted_in_logs() {
        let output = logged(|| tracing::info!(transcript = "my private words", "transcribed"));
        assert!(!output.contains("private words"), "{output}");
        let output = logged(|| tracing::info!(transcript = ?"my private words", "transcribed"));
        assert!(!output.contains("private words"), "{output}");
    }

    #[test]
    fn bearer_tokens_are_redacted() {
        assert_eq!(redact("Authorization: Bearer abc.def-123456"), REDACTED);
        assert_eq!(redact("got bearer abcdef123456"), format!("got {REDACTED}"));
        assert_eq!(redact(r#"authorization="Bearer abc.def""#), REDACTED);
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let line = "rendered frame 42 in 3ms, task-count=4";
        assert_eq!(redact(line), line);
    }
}
