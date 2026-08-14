//! The global hotkey, in hold-to-talk mode.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::session;

#[cfg(not(target_os = "macos"))]
pub const DEFAULT_ACCELERATOR: &str = "Ctrl+Space";

/// Undo — or redo — the vocabulary replacement in the last dictation, without
/// leaving the app the text was typed into.
///
/// Fixed rather than a setting: [four settings](SETTINGS.md) is the whole surface,
/// and these are listed there read-only instead. `Ctrl+Alt+Z` reads as an undo and
/// is claimed by very little.
#[cfg(not(target_os = "macos"))]
pub const UNDO_ACCELERATOR: &str = "Ctrl+Alt+Z";

/// `Cmd+Z` is undo and `Cmd+Shift+Z` is redo in every Mac app, so neither is
/// Piplo's to take.
#[cfg(target_os = "macos")]
pub const UNDO_ACCELERATOR: &str = "Cmd+Alt+Z";

/// The same for the grammar cleanup: put the words back as they were spoken.
///
/// A separate chord rather than one that cycles both, because they are separate
/// judgements — the cleanup can be wrong when the word fix was right, and the other
/// way round.
#[cfg(not(target_os = "macos"))]
pub const GRAMMAR_ACCELERATOR: &str = "Ctrl+Alt+G";

#[cfg(target_os = "macos")]
pub const GRAMMAR_ACCELERATOR: &str = "Cmd+Alt+G";

/// One row of the read-only list on the settings page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Binding {
    pub accelerator: String,
    pub label: String,
    pub hint: String,
    /// False when the system refused it, so the page can say so rather than
    /// promising a chord that does nothing.
    pub bound: bool,
}

/// `Ctrl+Space` is "select the previous input source" on macOS, so it would fight
/// the system on any Mac with more than one keyboard layout installed — and the
/// symptom is a shortcut that sometimes dictates and sometimes changes your
/// language. `Cmd+Shift+Space` is unclaimed.
#[cfg(target_os = "macos")]
pub const DEFAULT_ACCELERATOR: &str = "Cmd+Shift+Space";

/// Which platform's rules to apply to a chord.
///
/// A parameter rather than a `cfg` inside the check, so both sets of rules are
/// exercised by `cargo test` on both CI runners. The Windows rules were the only
/// rules until macOS arrived, and they refuse the very modifier macOS needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rules {
    Windows,
    MacOs,
}

pub const RULES: Rules = if cfg!(target_os = "macos") {
    Rules::MacOs
} else {
    Rules::Windows
};

/// Windows repeats `Pressed` while a key is held. Without this, a five-second
/// hold would start fifty sessions.
#[derive(Default)]
pub struct KeyDown(AtomicBool);

/// `Wry` rather than a generic runtime: the handler calls into `session`, which
/// works with the concrete `AppHandle`.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| match event.state() {
            // On release, so they fire once: `Pressed` auto-repeats while held, and
            // a repeating undo would flip the text back and forth.
            ShortcutState::Released if matches(shortcut, UNDO_ACCELERATOR) => {
                session::toggle_from_shortcut(app, session::Step::Vocabulary);
            }
            ShortcutState::Released if matches(shortcut, GRAMMAR_ACCELERATOR) => {
                session::toggle_from_shortcut(app, session::Step::Grammar);
            }
            ShortcutState::Pressed
                if matches(shortcut, UNDO_ACCELERATOR)
                    || matches(shortcut, GRAMMAR_ACCELERATOR) => {}
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

/// Every accelerator reaches the same handler, so it has to tell them apart.
/// Compared as parsed chords rather than strings — the plugin's own formatting is
/// not the text we wrote.
fn matches(shortcut: &Shortcut, accelerator: &str) -> bool {
    accelerator
        .parse::<Shortcut>()
        .is_ok_and(|parsed| parsed == *shortcut)
}

/// Failure is not fatal, and not silent: the settings page reads the same state, so
/// a chord another app already owns is shown as unavailable rather than promised.
pub fn register_fixed(app: &AppHandle) {
    for (accelerator, what) in [
        (UNDO_ACCELERATOR, "the last word fix"),
        (GRAMMAR_ACCELERATOR, "the grammar cleanup"),
    ] {
        match app.global_shortcut().register(accelerator) {
            Ok(()) => println!("piplo: {accelerator} undoes {what}"),
            Err(err) => eprintln!("piplo: could not bind {accelerator}: {err}"),
        }
    }
}

/// What the settings page lists. Built from the constants above so the page cannot
/// drift from what is actually bound, and `bound` is read from the plugin rather
/// than assumed.
#[tauri::command]
pub fn list_shortcuts(app: AppHandle) -> Vec<Binding> {
    [
        (
            UNDO_ACCELERATOR,
            "Undo the last word fix",
            "Types the word Piplo heard instead of the term it replaced it with. Press again to put it back.",
        ),
        (
            GRAMMAR_ACCELERATOR,
            "Undo the grammar cleanup",
            "Retypes the last dictation as you said it, before the cleanup. Press again to put it back.",
        ),
    ]
    .into_iter()
    .map(|(accelerator, label, hint)| Binding {
        accelerator: accelerator.to_string(),
        label: label.to_string(),
        hint: hint.to_string(),
        bound: app
            .global_shortcut()
            .is_registered(accelerator),
    })
    .collect()
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
    reject_reserved(accelerator, RULES)?;

    app.global_shortcut()
        .register(accelerator)
        .map_err(|err| format!("the system would not accept {accelerator}: {err}"))
}

/// Chords the platform accepts in name and then swallows.
///
/// Enforced here as well as in the recorder, because `settings.json` is a plain
/// file and can be edited by hand.
fn reject_reserved(accelerator: &str, rules: Rules) -> Result<(), String> {
    let tokens: Vec<String> = accelerator
        .split('+')
        .map(|token| token.trim().to_ascii_uppercase())
        .collect();

    let has = |name: &str| tokens.iter().any(|token| token == name);
    let super_key = ["SUPER", "CMD", "COMMAND", "META", "WIN"]
        .iter()
        .any(|name| has(name));

    match rules {
        // `RegisterHotKey` reports success for combinations the shell has already
        // claimed — `Ctrl+Win+D` and friends — and then never fires, so the
        // shortcut looks bound and silently does nothing. That is worse than being
        // told no, and the reserved set is undocumented and grows with each
        // Windows release, so allowing "the rest" would be guesswork.
        Rules::Windows if super_key => Err("The Windows key can't be used — \
             Windows reserves these combinations and would swallow the shortcut \
             without telling you."
            .to_string()),

        // Cmd is the ordinary modifier on macOS and must be allowed, or Piplo
        // would refuse its own default. `RegisterEventHotKey` also reports a real
        // error on a conflict rather than pretending, so the OS can be trusted to
        // say no — except for these two, which are muscle memory strong enough
        // that taking them over would be a mistake even if it worked.
        Rules::MacOs if super_key && has("SPACE") && !has("SHIFT") && !has("ALT") => {
            Err("Cmd+Space is Spotlight. Try adding Shift.".to_string())
        }
        Rules::MacOs if super_key && has("TAB") => {
            Err("Cmd+Tab switches apps — macOS keeps that one.".to_string())
        }

        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_refuses_the_windows_key() {
        for chord in ["Super+D", "Ctrl+Super+Left", "cmd+k"] {
            assert!(reject_reserved(chord, Rules::Windows).is_err(), "{chord}");
        }
    }

    #[test]
    fn windows_allows_the_ordinary_modifiers() {
        for chord in ["Ctrl+Space", "Alt+Shift+D", "Ctrl+S"] {
            assert!(reject_reserved(chord, Rules::Windows).is_ok(), "{chord}");
        }
    }

    /// The rule that would have broken macOS: Cmd is ordinary there, and refusing
    /// it would leave Piplo unable to bind its own default.
    #[test]
    fn macos_allows_cmd() {
        for chord in ["Cmd+Shift+Space", "Cmd+K", "Cmd+Alt+D"] {
            assert!(reject_reserved(chord, Rules::MacOs).is_ok(), "{chord}");
        }
    }

    #[test]
    fn macos_refuses_only_what_the_system_owns() {
        assert!(reject_reserved("Cmd+Space", Rules::MacOs).is_err());
        assert!(reject_reserved("Cmd+Tab", Rules::MacOs).is_err());

        // Adding a modifier makes it Piplo's to take.
        assert!(reject_reserved("Cmd+Shift+Space", Rules::MacOs).is_ok());
        // Ctrl+Space is not Spotlight, only Cmd+Space is.
        assert!(reject_reserved("Ctrl+Space", Rules::MacOs).is_ok());
    }

    /// The whole point of the per-platform default. Whatever the platform, the
    /// shortcut Piplo ships with must be one it is willing to bind — otherwise a
    /// fresh install binds nothing and cannot dictate at all.
    #[test]
    fn the_default_is_bindable_on_this_platform() {
        assert!(
            reject_reserved(DEFAULT_ACCELERATOR, RULES).is_ok(),
            "{DEFAULT_ACCELERATOR} is refused by its own platform's rules"
        );
    }

    /// "Superscript" is not a modifier token; only whole tokens count.
    #[test]
    fn only_exact_tokens_match() {
        assert!(reject_reserved("Ctrl+S", Rules::Windows).is_ok());
        assert!(reject_reserved("Cmd+Spacebar", Rules::MacOs).is_ok());
    }
}
