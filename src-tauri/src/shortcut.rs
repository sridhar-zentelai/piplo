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

/// Falls back to `Ctrl+Space` when the saved shortcut no longer parses or has
/// since been claimed by something else. A saved setting should never be able to
/// brick the app.
pub fn register_initial(app: &AppHandle, accelerator: &str) {
    if try_register(app, accelerator).is_ok() {
        return;
    }

    if accelerator == DEFAULT_ACCELERATOR {
        eprintln!("piplo: nothing bound — {DEFAULT_ACCELERATOR} was refused");
        return;
    }

    eprintln!("piplo: '{accelerator}' could not be bound; falling back to {DEFAULT_ACCELERATOR}");

    if let Err(err) = try_register(app, DEFAULT_ACCELERATOR) {
        eprintln!("piplo: nothing bound — {err}");
    }
}

/// **The order matters.** Register the new accelerator first, and only unregister
/// the old one once that succeeds. The other way round means a chord already
/// claimed by another app leaves Piplo with nothing bound — no way to dictate,
/// and no obvious way to recover.
pub fn rebind(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    try_register(app, new)?;

    if let Err(err) = app.global_shortcut().unregister(old) {
        // The new binding already works, so this is untidy rather than broken.
        eprintln!("piplo: could not release '{old}': {err}");
    }

    println!("piplo: shortcut rebound to {new}");
    Ok(())
}

fn try_register(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    reject_super(accelerator)?;

    app.global_shortcut()
        .register(accelerator)
        .map_err(|err| format!("Windows would not accept {accelerator}: {err}"))
}

/// Enforced here as well as in the recorder, because `settings.json` is a plain
/// file and can be edited by hand.
///
/// `RegisterHotKey` reports success for combinations the shell has already
/// claimed — `Ctrl+Win+D` and friends — and then never fires, so the shortcut
/// looks bound and silently does nothing. That is worse than being told no, and
/// the reserved set is undocumented and grows with each Windows release.
fn reject_super(accelerator: &str) -> Result<(), String> {
    let has_super = accelerator.split('+').any(|token| {
        matches!(
            token.trim().to_ascii_uppercase().as_str(),
            "SUPER" | "CMD" | "COMMAND" | "META" | "WIN"
        )
    });

    if has_super {
        return Err("The Windows key can't be used — Windows reserves these \
                    combinations and would swallow the shortcut without telling you."
            .to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_the_windows_key() {
        assert!(reject_super("Super+D").is_err());
        assert!(reject_super("Ctrl+Super+Left").is_err());
        assert!(reject_super("cmd+k").is_err());
    }

    #[test]
    fn allows_the_ordinary_modifiers() {
        assert!(reject_super("Ctrl+Space").is_ok());
        assert!(reject_super("Alt+Shift+D").is_ok());
        // "Superscript" is not a modifier token; only exact matches are refused.
        assert!(reject_super("Ctrl+S").is_ok());
    }
}
