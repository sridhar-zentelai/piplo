//! The global hotkey, in hold-to-talk mode.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::session;

pub const DEFAULT_ACCELERATOR: &str = "Ctrl+Space";

/// Windows repeats `Pressed` while a key is held. Without this, a five-second
/// hold would start fifty sessions.
#[derive(Default)]
pub struct KeyDown(AtomicBool);

/// `Wry` rather than a generic runtime: the handler calls into `session`, which
/// works with the concrete `AppHandle`.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| match event.state() {
            ShortcutState::Pressed => {
                if app.state::<KeyDown>().0.swap(true, Ordering::SeqCst) {
                    return; // auto-repeat
                }
                session::start(app, session::Trigger::Shortcut);
            }
            ShortcutState::Released => {
                if !app.state::<KeyDown>().0.swap(false, Ordering::SeqCst) {
                    return;
                }
                session::finish(app, session::Trigger::Shortcut);
            }
        })
        .build()
}

pub fn register(app: &AppHandle) {
    if let Err(err) = app.global_shortcut().register(DEFAULT_ACCELERATOR) {
        // Windows refuses combinations it already owns. Losing the binding is
        // worth a log, not a crash.
        eprintln!("piplo: could not register {DEFAULT_ACCELERATOR}: {err}");
    }
}
