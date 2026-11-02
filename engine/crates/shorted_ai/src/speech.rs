//! Transcription in the cloud with OpenAI (T-0601 stays the default): the sound alone, never the
//! picture, as a 16 kHz mono WAV, and words with their timings back.

use std::fmt;

use serde::Deserialize;

use crate::providers::{AiError, HttpResponse, Secret, status_error};

const OPENAI_URL: &str = "https://api.openai.com";
const TRANSCRIPTION_MODEL: &str = "whisper-1";
const BOUNDARY: &str = "shorted-upload-7d1f0c2b9a";
/// OpenAI refuses files over 25 MB: ten minutes of 16 kHz mono is about 19 MB.
pub const MAX_CHUNK_SAMPLES: usize = 16_000 * 600;

/// A request with a binary body. Header values hold the key, so `Debug` leaves them out.
#[derive(Clone, PartialEq)]
pub struct Upload {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl fmt::Debug for Upload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("Upload")
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// Sends an upload. Implemented by the app over its HTTP client, and by tests.
pub trait UploadHttp {
    fn upload(&self, upload: &Upload) -> Result<HttpResponse, AiError>;
}

/// Mono samples between -1 and 1 as a 16-bit PCM WAV file.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn field(body: &mut Vec<u8>, name: &str, value: &str) {
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

/// Asks for the words of `wav`, each with its start and end.
pub fn transcription_upload(key: &Secret, base_url: Option<&str>, wav: &[u8]) -> Upload {
    let base = base_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or(OPENAI_URL)
        .trim_end_matches('/');
    let mut body = Vec::with_capacity(wav.len() + 1024);
    field(&mut body, "model", TRANSCRIPTION_MODEL);
    field(&mut body, "response_format", "verbose_json");
    field(&mut body, "timestamp_granularities[]", "word");
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sound.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Upload {
        url: format!("{base}/v1/audio/transcriptions"),
        headers: vec![
            (
                "content-type".into(),
                format!("multipart/form-data; boundary={BOUNDARY}"),
            ),
            ("authorization".into(), format!("Bearer {}", key.expose())),
        ],
        body,
    }
}

/// One word heard, in microseconds from the start of the sound sent.
#[derive(Clone, Debug, PartialEq)]
pub struct HeardWord {
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    language: String,
    #[serde(default)]
    words: Vec<AnswerWord>,
}

#[derive(Deserialize)]
struct AnswerWord {
    word: String,
    start: f64,
    end: f64,
}

/// OpenAI names the language in English ("french"); captions keep ISO 639-1 codes.
fn language_code(name: &str) -> String {
    const NAMES: [(&str, &str); 14] = [
        ("english", "en"),
        ("french", "fr"),
        ("spanish", "es"),
        ("portuguese", "pt"),
        ("german", "de"),
        ("italian", "it"),
        ("dutch", "nl"),
        ("japanese", "ja"),
        ("chinese", "zh"),
        ("korean", "ko"),
        ("arabic", "ar"),
        ("russian", "ru"),
        ("turkish", "tr"),
        ("polish", "pl"),
    ];
    let name = name.trim().to_lowercase();
    NAMES.iter().find(|(full, _)| *full == name).map_or_else(
        || name.chars().take(2).collect(),
        |(_, code)| (*code).into(),
    )
}

/// The words and the language of an answer, or why there are none.
pub fn read_transcription(response: &HttpResponse) -> Result<(Vec<HeardWord>, String), AiError> {
    if let Some(error) = status_error(response) {
        return Err(error);
    }
    let answer: Answer = serde_json::from_str(&response.body)
        .map_err(|_| AiError::BadResponse("not a transcription".into()))?;
    let to_us = |seconds: f64| (seconds * 1_000_000.0).round() as i64;
    let words = answer
        .words
        .into_iter()
        .filter(|word| !word.word.trim().is_empty())
        .map(|word| HeardWord {
            text: word.word.trim().to_string(),
            start_us: to_us(word.start),
            end_us: to_us(word.end.max(word.start)),
        })
        .collect();
    Ok((words, language_code(&answer.language)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-proj-secret-42";

    #[test]
    fn the_upload_carries_the_sound_and_asks_for_word_timings() {
        let sound = wav(&[0.0, 0.5, -1.0], 16_000);
        assert_eq!(&sound[..4], b"RIFF");
        assert_eq!(sound.len(), 44 + 6);
        let upload = transcription_upload(&Secret::new(KEY.into()), None, &sound);
        assert_eq!(upload.url, "https://api.openai.com/v1/audio/transcriptions");
        let text = String::from_utf8_lossy(&upload.body);
        assert!(text.contains("whisper-1"));
        assert!(text.contains("timestamp_granularities[]"));
        assert!(
            upload
                .body
                .windows(sound.len())
                .any(|w| w == sound.as_slice())
        );
        assert!(!format!("{upload:?}").contains(KEY));
    }

    #[test]
    fn words_come_back_in_microseconds_with_a_language_code() {
        let response = HttpResponse {
            status: 200,
            body: r#"{"language":"french","text":"salut toi","words":[
                {"word":"salut","start":0.12,"end":0.5},{"word":" ","start":0.5,"end":0.5},
                {"word":"toi","start":0.5,"end":0.9}]}"#
                .into(),
        };
        let (words, language) = read_transcription(&response).unwrap();
        assert_eq!(language, "fr");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].start_us, 120_000);
        assert_eq!(words[1].end_us, 900_000);
    }

    #[test]
    fn a_rejected_key_is_reported_as_such() {
        let response = HttpResponse {
            status: 401,
            body: "{}".into(),
        };
        assert_eq!(read_transcription(&response), Err(AiError::KeyRejected));
    }
}
