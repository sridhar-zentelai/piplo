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
    /// A snippet expansion — `text` is the canned content, `raw_text` the
    /// trigger. Not shown in the UI; it is here so the feature's real-world
    /// behaviour is greppable. `default` so lines written before it still load.
    #[serde(default)]
    pub snippet: bool,
    /// Whether the vocabulary replaced anything, on either side of grammar.
    /// Greppable for the same reason as `corrected` and `snippet`.
    #[serde(default)]
    pub vocabulary: bool,
    /// The user's own correction, present only on rows they fixed. `text` keeps
    /// meaning **what was typed into the application** — the one guarantee that
    /// field has, and nothing here may change it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<String>,
}

/// What the learner needs to know about the row it just corrected.
pub struct Corrected {
    /// What was typed into the application, which is what the edit is against.
    pub typed: String,
    /// Editing canned text is editing the snippet, not the transcript.
    pub snippet: bool,
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
            snippet: false,
            vocabulary: false,
            edited: None,
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

/// Whole file, newest first. Malformed lines are skipped rather than fatal — one
/// bad line from an interrupted write must not hide the entire history.
#[tauri::command]
pub fn get_history(app: AppHandle) -> Vec<Entry> {
    let Some(dir) = directory(&app) else {
        return Vec::new();
    };

    let path = dir.join(FILE);

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new(); // no dictations yet
    };

    let mut entries: Vec<Entry> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<Entry>(line) {
            Ok(entry) => Some(entry),
            Err(err) => {
                eprintln!("piplo: skipping malformed history line: {err}");
                None
            }
        })
        .collect();

    entries.reverse();
    entries
}

/// Record the user's correction on one row, and report what was there before.
///
/// Editing a row means rewriting the file whole, which is a change of character
/// for an append-only log and the right trade: it is already read whole on every
/// render, so rewriting costs the same class, and the alternative — appending a
/// supersede record — pushes a fold onto every reader of the file forever.
///
/// Lines that will not parse are carried through untouched rather than dropped: a
/// rewrite must not be a way to lose history.
pub fn correct(app: &AppHandle, id: &str, edited: &str) -> Result<Corrected, String> {
    let dir = directory(app).ok_or_else(|| "no history directory".to_string())?;
    let path = dir.join(FILE);

    let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;

    let mut found: Option<Corrected> = None;
    let mut out = String::with_capacity(text.len() + edited.len());

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<Entry>(line) {
            Ok(mut entry) if entry.id == id => {
                found = Some(Corrected {
                    typed: entry.text.clone(),
                    snippet: entry.snippet,
                });

                entry.edited = Some(edited.to_string());

                match serde_json::to_string(&entry) {
                    Ok(json) => out.push_str(&json),
                    Err(err) => return Err(err.to_string()),
                }
            }
            _ => out.push_str(line),
        }

        out.push('\n');
    }

    let found = found.ok_or_else(|| "that dictation is no longer in history".to_string())?;

    // Temp file and rename, so an interrupted write leaves either the old file or
    // the new one and never half of one.
    let temp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&temp, out).map_err(|err| err.to_string())?;
    std::fs::rename(&temp, &path).map_err(|err| err.to_string())?;

    Ok(found)
}

/// Drop one row from the log.
///
/// The same rewrite `correct` does, and for the same reason: the file is read
/// whole on every render, so rewriting it costs the same class of work as
/// reading it, and appending a tombstone would push a fold onto every reader of
/// the file forever.
///
/// Lines that will not parse are carried through untouched. They are not the row
/// being deleted, and a delete must never be a way to lose the rest of the
/// history.
#[tauri::command]
pub fn delete_entry(app: AppHandle, id: String) -> Result<(), String> {
    let dir = directory(&app).ok_or_else(|| "no history directory".to_string())?;
    let path = dir.join(FILE);

    let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;

    let mut found = false;
    let mut out = String::with_capacity(text.len());

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let is_target = serde_json::from_str::<Entry>(line)
            .map(|entry| entry.id == id)
            .unwrap_or(false);

        if is_target {
            found = true;
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    if !found {
        return Err("that dictation is no longer in history".to_string());
    }

    // Temp file and rename, so an interrupted write leaves either the old file or
    // the new one and never half of one.
    let temp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&temp, out).map_err(|err| err.to_string())?;
    std::fs::rename(&temp, &path).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    let Some(dir) = directory(&app) else {
        return Err("no history directory".to_string());
    };

    let path = dir.join(FILE);

    if !path.exists() {
        return Ok(());
    }

    // Truncate rather than delete, so the file keeps its permissions and the
    // append path does not have to recreate it.
    std::fs::write(&path, "").map_err(|err| err.to_string())
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
