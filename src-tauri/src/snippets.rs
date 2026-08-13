//! Spoken triggers that get typed as longer canned text.
//!
//! `normalize` and `match_trigger` are pure functions over data — no
//! `AppHandle`, no I/O — which is what makes them the two things here worth
//! unit tests. Everything else is storage and validation around them.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const FILE: &str = "snippets.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    /// A uuid v4 minted on create. Empty on the way in means "this is new".
    pub id: String,
    /// What you say.
    pub trigger: String,
    /// What gets typed instead.
    pub content: String,
}

/// The live list, read by the pipeline on every dictation.
#[derive(Default)]
pub struct Store(Mutex<Vec<Snippet>>);

impl Store {
    pub fn new(snippets: Vec<Snippet>) -> Self {
        Self(Mutex::new(snippets))
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Snippet>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn get(&self) -> Vec<Snippet> {
        self.lock().clone()
    }
}

/// The comparison both sides go through — the spoken text and the stored
/// trigger. Whisper decides on its own whether an utterance ends in a full stop
/// and how it is capitalised, so neither can be part of the comparison.
pub fn normalize(text: &str) -> String {
    text.trim_matches(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The content, if the **whole utterance** is a trigger.
///
/// Deliberately not a substring search: a trigger found inside a sentence is far
/// more likely to be someone talking about it, and unpredictable expansion in a
/// real document is worse than no expansion at all.
pub fn match_trigger(snippets: &[Snippet], text: &str) -> Option<String> {
    let spoken = normalize(text);

    // Silence normalizes to nothing, and so could a corrupt trigger. Guard both
    // ends or every empty dictation expands.
    if spoken.is_empty() {
        return None;
    }

    snippets
        .iter()
        .find(|snippet| {
            let trigger = normalize(&snippet.trigger);
            !trigger.is_empty() && trigger == spoken
        })
        .map(|snippet| snippet.content.clone())
}

/// The pipeline's entry point: match against the live list.
pub fn expand(app: &AppHandle, text: &str) -> Option<String> {
    let store = app.state::<Store>();
    let snippets = store.lock();
    match_trigger(&snippets, text)
}

/// A corrupt file loads as no snippets rather than failing the launch —
/// dictation works fine without them, and the next save overwrites the bad file.
pub fn load(app: &AppHandle) -> Vec<Snippet> {
    let Some(path) = path(app) else {
        return Vec::new();
    };

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new(); // none saved yet
    };

    match serde_json::from_str::<Vec<Snippet>>(&text) {
        Ok(snippets) => snippets,
        Err(err) => {
            eprintln!(
                "piplo: {} is not readable ({err}); starting with no snippets",
                path.display()
            );
            Vec::new()
        }
    }
}

/// Write, **then** adopt. A failed write leaves memory and disk in agreement,
/// both holding the last good state — the alternative gives the user a snippet
/// that works until they restart, which is worse than a visible error.
fn commit(app: &AppHandle, snippets: Vec<Snippet>) -> Result<Vec<Snippet>, String> {
    write(app, &snippets)?;

    let store = app.state::<Store>();
    *store.lock() = snippets.clone();

    Ok(snippets)
}

fn write(app: &AppHandle, snippets: &[Snippet]) -> Result<(), String> {
    let path = path(app).ok_or_else(|| "no config directory".to_string())?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let json = serde_json::to_string_pretty(snippets).map_err(|err| err.to_string())?;
    std::fs::write(&path, json).map_err(|err| err.to_string())
}

/// Next to `settings.json`, for the same reason: state that should follow the
/// install rather than the checkout.
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
pub fn list_snippets(app: AppHandle) -> Vec<Snippet> {
    app.state::<Store>().get()
}

/// Create and update both, keyed on `id`. The page has one form and the
/// difference is whether `id` is empty, so splitting this in two would put that
/// branch in two places.
///
/// The backend is the only place that decides what is valid. The form pre-checks
/// the same rules for instant feedback, but never authoritatively.
#[tauri::command]
pub fn save_snippet(app: AppHandle, snippet: Snippet) -> Result<Vec<Snippet>, String> {
    // Trimmed before storing, so a trailing space can never be the invisible
    // difference between two entries.
    let trigger = snippet.trigger.trim().to_string();
    let content = snippet.content.trim().to_string();

    if normalize(&trigger).is_empty() {
        return Err("Give the snippet something to say.".into());
    }

    if content.is_empty() {
        return Err("Give the snippet some text to type.".into());
    }

    let mut snippets = app.state::<Store>().get();
    let normalized = normalize(&trigger);

    // Two snippets answering to the same phrase makes which one wins a matter of
    // list order — something the user never sees and cannot control. The row
    // being edited is excluded, or saving it unchanged would clash with itself.
    let clash = snippets
        .iter()
        .any(|other| other.id != snippet.id && normalize(&other.trigger) == normalized);

    if clash {
        return Err(format!("'{trigger}' is already a trigger."));
    }

    match snippets.iter_mut().find(|other| other.id == snippet.id) {
        Some(existing) => {
            existing.trigger = trigger;
            existing.content = content;
        }
        // Appended, which is what makes file order the created order that
        // Newest / Oldest sort on.
        None => snippets.push(Snippet {
            id: uuid::Uuid::new_v4().to_string(),
            trigger,
            content,
        }),
    }

    commit(&app, snippets)
}

#[tauri::command]
pub fn delete_snippet(app: AppHandle, id: String) -> Result<Vec<Snippet>, String> {
    let mut snippets = app.state::<Store>().get();
    snippets.retain(|snippet| snippet.id != id);
    commit(&app, snippets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(trigger: &str, content: &str) -> Snippet {
        Snippet {
            id: trigger.to_string(),
            trigger: trigger.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn normalizes_case_padding_and_punctuation() {
        assert_eq!(normalize("My email."), "my email");
        assert_eq!(normalize("  my   EMAIL  "), "my email");
        assert_eq!(normalize("...my email!"), "my email");
        // Inner punctuation is part of the phrase — only the ends are stripped.
        assert_eq!(normalize("my e-mail"), "my e-mail");
    }

    #[test]
    fn matches_the_whole_utterance_only() {
        let snippets = vec![snippet("my email", "codeaprogram@gmail.com")];

        assert_eq!(
            match_trigger(&snippets, "My email."),
            Some("codeaprogram@gmail.com".into())
        );
        assert_eq!(
            match_trigger(&snippets, "  my   EMAIL  "),
            Some("codeaprogram@gmail.com".into())
        );

        // The case the whole feature's trustworthiness rests on.
        assert_eq!(match_trigger(&snippets, "send my email to Bob"), None);
        assert_eq!(match_trigger(&snippets, "my emails"), None);
    }

    #[test]
    fn empty_input_never_matches() {
        let snippets = vec![snippet("", "never"), snippet("my email", "address")];

        assert_eq!(match_trigger(&snippets, ""), None);
        assert_eq!(match_trigger(&snippets, "   "), None);
        assert_eq!(match_trigger(&snippets, "...!"), None);
    }

    #[test]
    fn picks_the_right_one_out_of_several() {
        let snippets = vec![
            snippet("my email", "codeaprogram@gmail.com"),
            snippet("sign off", "Thanks,\nSridhar"),
            snippet("intro email", "Hi, I hope you're doing well."),
        ];

        assert_eq!(
            match_trigger(&snippets, "sign off"),
            Some("Thanks,\nSridhar".into())
        );
        assert_eq!(
            match_trigger(&snippets, "Intro email!"),
            Some("Hi, I hope you're doing well.".into())
        );
        assert_eq!(match_trigger(&snippets, "sign"), None);
    }
}
