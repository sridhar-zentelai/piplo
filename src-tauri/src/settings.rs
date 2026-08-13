//! Three settings, in the platform config dir. Applied without a restart.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{shortcut, widget};

const FILE: &str = "settings.json";

/// Every field has a serde default, so a file written by an older build loads
/// what it has rather than falling back wholesale and losing the user's
/// shortcut.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub shortcut: String,
    pub grammar_enabled: bool,
    pub widget_visible: bool,
    /// Whether editing a history row records a correction. Only the learning half
    /// has a switch: the terms themselves are words the user typed in, and a
    /// toggle that ignores what you typed is a worse control than deleting it.
    pub learn_from_corrections: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: shortcut::DEFAULT_ACCELERATOR.to_string(),
            grammar_enabled: true,
            widget_visible: true,
            learn_from_corrections: true,
        }
    }
}

pub struct Store(Mutex<Settings>);

impl Store {
    pub fn new(settings: Settings) -> Self {
        Self(Mutex::new(settings))
    }

    fn lock(&self) -> MutexGuard<'_, Settings> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn get(&self) -> Settings {
        self.lock().clone()
    }
}

/// Read on every dictation rather than cached, so the toggle applies to the very
/// next one.
pub fn current(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

/// A corrupt or half-written file is ignored in favour of the defaults rather
/// than failing the launch. The next save overwrites it. A settings file must
/// never be able to prevent the app from starting.
pub fn load(app: &AppHandle) -> Settings {
    let Some(path) = path(app) else {
        return Settings::default();
    };

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };

    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!(
                "piplo: {} is not readable ({err}); using defaults",
                path.display()
            );
            Settings::default()
        }
    }
}

fn write(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = path(app).ok_or_else(|| "no config directory".to_string())?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let json = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    std::fs::write(&path, json).map_err(|err| err.to_string())
}

/// `%APPDATA%\com.codea.piplo` on Windows. Deliberately not in the repo: history
/// is meant to be read and grepped, settings are machine state that should follow
/// the install rather than the clone.
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
pub fn get_settings(app: AppHandle) -> Settings {
    current(&app)
}

/// Rebind, then update state, then write the file. A rejected shortcut leaves the
/// live binding *and* the saved file untouched, so the UI can roll back its
/// optimistic update.
#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    let previous = current(&app);

    if settings.shortcut != previous.shortcut {
        shortcut::rebind(&app, &previous.shortcut, &settings.shortcut)?;
    }

    if settings.widget_visible != previous.widget_visible {
        widget::set_visible(&app, settings.widget_visible);
    }

    {
        let store = app.state::<Store>();
        *store.lock() = settings.clone();
    }

    // The file is last: a write failure should not undo a binding that already
    // works, and the next successful save will fix the file.
    if let Err(err) = write(&app, &settings) {
        eprintln!("piplo: could not save settings: {err}");
    }

    Ok(settings)
}
