//! The Groq API key.
//!
//! Kept in its own file and its own state rather than as a field on
//! `Settings`, because `get_settings` hands that struct straight to the
//! webview — a key living there would be one `#[derive(Serialize)]` away from
//! leaking. Nothing here returns the key to the frontend; `ApiKeyStatus` is
//! all it can ever see.
//!
//! `credentials.json` sits next to `settings.json` in the config dir, in plain
//! text. That matches the threat model of a single-user desktop tool: anything
//! that can read `%APPDATA%` as this user can already read the process memory
//! holding the key.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const FILE: &str = "credentials.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    api_key: Option<String>,
}

/// `GROQ_API_KEY` from the environment, read once at startup.
///
/// It wins over a saved key: a `.env` in the repo is an explicit choice by
/// whoever is running the build, and being silently overridden by whatever is
/// in `%APPDATA%` would be very hard to debug.
pub struct Env(pub Option<String>);

/// The saved key, swappable at runtime so a new one applies to the next
/// dictation rather than the next launch.
pub struct Store(Mutex<Option<String>>);

impl Store {
    pub fn new(key: Option<String>) -> Self {
        Self(Mutex::new(key))
    }

    fn lock(&self) -> MutexGuard<'_, Option<String>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Everything the webview is allowed to know. Deliberately not the key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    configured: bool,
    /// Last four characters — enough to tell which key is loaded, useless alone.
    hint: Option<String>,
    /// `"env"`, `"settings"` or `"none"`.
    source: &'static str,
}

fn hint_of(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    let tail: Vec<char> = key.chars().rev().take(4).collect();
    Some(tail.into_iter().rev().collect())
}

fn status(app: &AppHandle) -> ApiKeyStatus {
    let env = app.state::<Env>();

    if let Some(key) = env.0.as_deref() {
        return ApiKeyStatus {
            configured: true,
            hint: hint_of(key),
            source: "env",
        };
    }

    let store = app.state::<Store>();
    let saved = store.lock();

    match saved.as_deref() {
        Some(key) => ApiKeyStatus {
            configured: true,
            hint: hint_of(key),
            source: "settings",
        },
        None => ApiKeyStatus {
            configured: false,
            hint: None,
            source: "none",
        },
    }
}

/// The key to dictate with, or `None`. Read per dictation, so saving one in the
/// UI applies immediately.
pub fn current(app: &AppHandle) -> Option<String> {
    let env = app.state::<Env>();

    if let Some(key) = env.0.clone() {
        return Some(key);
    }

    let store = app.state::<Store>();
    let saved = store.lock().clone();
    saved
}

/// A corrupt or missing file yields no key rather than failing the launch — the
/// same rule `settings.json` follows.
pub fn load(app: &AppHandle) -> Option<String> {
    let path = path(app)?;
    let text = std::fs::read_to_string(&path).ok()?;

    match serde_json::from_str::<Stored>(&text) {
        Ok(stored) => stored.api_key.filter(|key| !key.trim().is_empty()),
        Err(err) => {
            // The error carries a position, never the file's contents.
            eprintln!(
                "piplo: {} is not readable ({err}); ignoring it",
                path.display()
            );
            None
        }
    }
}

fn write(app: &AppHandle, key: Option<&str>) -> Result<(), String> {
    let path = path(app).ok_or_else(|| "no config directory".to_string())?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let stored = Stored {
        api_key: key.map(str::to_string),
    };
    let json = serde_json::to_string_pretty(&stored).map_err(|err| err.to_string())?;

    std::fs::write(&path, json).map_err(|err| err.to_string())
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join(FILE)),
        Err(err) => {
            eprintln!("piplo: no config directory: {err}");
            None
        }
    }
}

#[tauri::command]
pub fn get_api_key_status(app: AppHandle) -> ApiKeyStatus {
    status(&app)
}

/// The file is written before the in-memory key changes: there is no live
/// binding to preserve here, so a failed write should leave nothing altered
/// rather than leave the app running on a key that vanishes at restart.
#[tauri::command]
pub fn set_api_key(app: AppHandle, key: String) -> Result<ApiKeyStatus, String> {
    let key = key.trim().to_string();

    if key.is_empty() {
        return Err("The key is empty.".into());
    }

    write(&app, Some(&key))?;

    let store = app.state::<Store>();
    *store.lock() = Some(key);

    Ok(status(&app))
}

#[tauri::command]
pub fn clear_api_key(app: AppHandle) -> Result<ApiKeyStatus, String> {
    write(&app, None)?;

    let store = app.state::<Store>();
    *store.lock() = None;

    Ok(status(&app))
}
