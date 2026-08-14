//! Groq transcription. One API key, read only here.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

const BASE_URL: &str = "https://api.groq.com";
const MODEL: &str = "whisper-large-v3-turbo";

/// ISO-639-1. Dictation is English-only; there is no language picker.
const LANGUAGE: &str = "en";

/// Generous: a long dictation is a large upload, and failing early would cost
/// the user the whole take.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Groq's own error text can be long. The pill cannot show it all.
const MAX_MESSAGE: usize = 60;

#[derive(Debug)]
pub enum GroqError {
    Unauthorized,
    TooLarge,
    RateLimited,
    Api { status: u16, message: String },
    Network(String),
    Decode(String),
}

impl fmt::Display for GroqError {
    /// Shown on the pill, so short and specific enough to act on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "Check your API key"),
            Self::TooLarge => write!(f, "Recording too long"),
            Self::RateLimited => write!(f, "Rate limited, try again"),
            Self::Network(_) => write!(f, "No connection"),
            Self::Decode(_) => write!(f, "Unexpected response"),
            Self::Api { message, .. } => {
                let trimmed: String = message.chars().take(MAX_MESSAGE).collect();
                write!(f, "{trimmed}")
            }
        }
    }
}

impl GroqError {
    /// The full story, for the log.
    pub fn detail(&self) -> String {
        match self {
            Self::Unauthorized => "401 unauthorized".into(),
            Self::TooLarge => "413 payload too large".into(),
            Self::RateLimited => "429 rate limited".into(),
            Self::Api { status, message } => format!("{status}: {message}"),
            Self::Network(detail) | Self::Decode(detail) => detail.clone(),
        }
    }
}

/// `verbose_json` rather than `json`: it also returns `language` and `duration`,
/// both of which go straight into the history entry.
#[derive(Debug, Deserialize)]
pub struct Transcription {
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
}

/// `hint` is the user's vocabulary as a comma list. Absent — not empty — when
/// there is nothing to hint at: an empty `prompt` field is still a field Whisper
/// conditions on.
pub async fn transcribe(
    api_key: &str,
    wav: Vec<u8>,
    hint: Option<String>,
) -> Result<Transcription, GroqError> {
    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|err| GroqError::Network(err.to_string()))?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", MODEL)
        // Piplo is English-only for now. Left to auto-detect, Whisper reads a
        // noisy or very short take as another language and types back a script
        // the user cannot even correct — and the grammar step, which is told the
        // text is English, then rejects it. Pinning also skips detection.
        .text("language", LANGUAGE)
        .text("temperature", "0")
        .text("response_format", "verbose_json");

    if let Some(hint) = hint {
        println!("piplo: vocabulary hint — {hint}");
        form = form.text("prompt", hint);
    }

    // Shared, so this request reuses the connection the warm-up opened while the
    // user was still speaking. The timeout is set per request rather than on the
    // client because the client is shared with grammar, which wants a much
    // shorter one.
    crate::timing::whisper_sent();
    let response = crate::http::client()
        .post(format!("{BASE_URL}/openai/v1/audio/transcriptions"))
        .bearer_auth(api_key)
        .timeout(TIMEOUT)
        .multipart(form)
        .send()
        .await
        .map_err(|err| GroqError::Network(err.to_string()))?;

    let status = response.status();

    if !status.is_success() {
        // Read the body before mapping: Groq explains itself, and "failed" is a
        // worse message than whatever it said.
        let body = response.text().await.unwrap_or_default();
        crate::timing::whisper_done();

        return Err(match status.as_u16() {
            401 | 403 => GroqError::Unauthorized,
            413 => GroqError::TooLarge,
            429 => GroqError::RateLimited,
            code => GroqError::Api {
                status: code,
                message: api_message(&body),
            },
        });
    }

    let parsed = response
        .json::<Transcription>()
        .await
        .map_err(|err| GroqError::Decode(err.to_string()));
    // Includes the JSON decode as well as the body read — sub-millisecond for a
    // transcript, and it keeps the mark on every exit path.
    crate::timing::whisper_done();

    parsed
}

/// Groq wraps failures as `{"error":{"message":"..."}}`. Fall back to the raw
/// body when it doesn't.
fn api_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|json| {
            json.get("error")?
                .get("message")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_owned())
}

pub fn model() -> &'static str {
    MODEL
}
