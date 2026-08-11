//! One JSONL line per dictation.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const FILE: &str = "history.jsonl";

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// RFC 3339, UTC.
    pub at: String,
    pub duration_secs: f32,
    pub model: String,
    #[serde(default)]
    pub language: Option<String>,
    pub chars: usize,
    /// False when transcription succeeded but typing did not.
    pub inserted: bool,
    /// What was actually typed — after grammar.
    pub text: String,
    /// What Whisper returned. Only present when it differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    /// Whether grammar won, or the fallback did.
    pub corrected: bool,
}

impl Entry {
    pub fn now(text: String, raw_text: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| String::from("unknown")),
            duration_secs: 0.0,
            model: crate::groq::model().to_string(),
            language: None,
            chars: text.chars().count(),
            inserted: false,
            text,
            raw_text,
            corrected: false,
        }
    }
}

/// Baked at compile time — the clone this binary was built in.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub fn append(app: &AppHandle, entry: &Entry) {
    let Some(dir) = directory(app) else {
        return;
    };

    if let Err(err) = std::fs::create_dir_all(&dir) {
        eprintln!("piplo: could not create {}: {err}", dir.display());
        return;
    }

    let line = match serde_json::to_string(entry) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("piplo: could not serialise history entry: {err}");
            return;
        }
    };

    let path = dir.join(FILE);

    // Append mode is atomic enough for single-process line writes; no locking.
    let opened = OpenOptions::new().create(true).append(true).open(&path);

    match opened {
        Ok(mut file) => {
            if let Err(err) = file.write_all(format!("{line}\n").as_bytes()) {
                eprintln!("piplo: could not write history: {err}");
            }
        }
        Err(err) => eprintln!("piplo: could not open {}: {err}", path.display()),
    }
}

fn directory(app: &AppHandle) -> Option<PathBuf> {
    let root = repo_root();

    if root.exists() {
        return Some(root.join("history"));
    }

    // The build moved to another machine. Record somewhere rather than silently
    // dropping what the user just dictated.
    match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join("history")),
        Err(err) => {
            eprintln!("piplo: no history directory available: {err}");
            None
        }
    }
}
