//! The words Piplo should get right.
//!
//! `apply`, `prompt`, `terms_in` and `echoed` are pure functions over data — no
//! `AppHandle`, no I/O — which is what makes them the things here worth unit
//! tests. `apply` runs on every dictation, twice; everything else in this file is
//! storage and validation around it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const FILE: &str = "vocabulary.json";

/// Whisper's prompt window is small, a long list is more likely to be echoed
/// than obeyed, and the terms just added are the ones about to be said.
const MAX_PROMPT: usize = 180;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// A uuid v4 minted on create. Empty on the way in means "this is new".
    pub id: String,
    /// Written verbatim into the transcript — the entire point of `ZentelAI`.
    pub term: String,
    /// What Whisper says instead. Replaced deterministically.
    #[serde(default)]
    pub variants: Vec<String>,
    /// `manual` or `learned`, and on the entry rather than the variant: a term
    /// the user asked for stays a term they asked for.
    #[serde(default = "manual")]
    pub source: String,
}

fn manual() -> String {
    "manual".to_string()
}

/// The live list, read by the pipeline on every dictation.
#[derive(Default)]
pub struct Store(Mutex<Vec<Entry>>);

impl Store {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self(Mutex::new(entries))
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Entry>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn get(&self) -> Vec<Entry> {
        self.lock().clone()
    }
}

/// Read the whole list once, so both `apply` passes and the two prompts in one
/// dictation see the same dictionary even if the page saves mid-flight.
pub fn snapshot(app: &AppHandle) -> Vec<Entry> {
    app.state::<Store>().get()
}

/// The corrected text, and whether anything actually changed.
///
/// One pass, left to right, longest variant first, whole words only,
/// case-insensitive going in and exact coming out. A replaced span is skipped
/// rather than rescanned, so a term containing another term's variant cannot
/// cascade. Nothing but the matched spans is touched.
///
/// No regex: whole-word matching is a scan with two boundary checks, and a
/// dependency needs a better reason than saving twenty lines.
/// One replacement that actually happened: the words that were there, and the term
/// that replaced them. Kept so the fix can be undone in the user's own app without
/// Piplo ever reading it back.
#[derive(Debug, Clone, Serialize)]
pub struct Fired {
    pub variant: String,
    pub term: String,
}

/// [`apply`], plus what it replaced.
pub fn apply_tracked(entries: &[Entry], text: &str) -> (String, Vec<Fired>) {
    let mut pairs: Vec<(Vec<char>, &str)> = Vec::new();

    for entry in entries {
        if entry.term.trim().is_empty() {
            continue;
        }

        for variant in &entry.variants {
            let chars: Vec<char> = variant.trim().chars().collect();

            if !chars.is_empty() {
                pairs.push((chars, entry.term.as_str()));
            }
        }
    }

    if pairs.is_empty() {
        return (text.to_string(), Vec::new());
    }

    // `gentle AI studio` must beat `gentle AI` whatever order the file has them
    // in — list order may never decide a replacement.
    pairs.sort_by_key(|(variant, _)| std::cmp::Reverse(variant.len()));

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut fired: Vec<Fired> = Vec::new();

    while at < chars.len() {
        let hit = pairs
            .iter()
            .find(|(variant, _)| matches_word_at(&chars, at, variant));

        match hit {
            Some((variant, term)) => {
                let was: String = chars[at..at + variant.len()].iter().collect();

                // Only a difference counts: a variant that already reads exactly
                // like its term must not flag the dictation or offer an undo.
                if was != *term {
                    fired.push(Fired {
                        variant: was,
                        term: (*term).to_string(),
                    });
                }

                out.push_str(term);
                at += variant.len();
            }
            None => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }

    (out, fired)
}

/// Put back what [`apply_tracked`] replaced, so the user can undo a fix in the app
/// they are still typing in.
///
/// Works on the **final** text, after grammar, so the cleanup's work survives —
/// only the terms go back to the words that were actually said. Longest term first,
/// whole words only, same rules as the forward pass.
pub fn revert(text: &str, fired: &[Fired]) -> String {
    let mut pairs: Vec<(Vec<char>, &str)> = fired
        .iter()
        .map(|hit| (hit.term.chars().collect(), hit.variant.as_str()))
        .collect();

    pairs.sort_by_key(|(term, _)| std::cmp::Reverse(term.len()));

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;

    while at < chars.len() {
        match pairs
            .iter()
            .find(|(term, _)| matches_word_at(&chars, at, term))
        {
            Some((term, variant)) => {
                out.push_str(variant);
                at += term.len();
            }
            None => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }

    out
}

/// The terms that **actually appear** in this text, for the grammar prompt's
/// preserve line. Never the whole dictionary — a list of words the model has not
/// seen in the text is an invitation to insert them.
pub fn terms_in(entries: &[Entry], text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();

    entries
        .iter()
        .filter(|entry| !entry.term.trim().is_empty())
        .filter(|entry| {
            let term: Vec<char> = entry.term.trim().chars().collect();
            (0..chars.len()).any(|at| matches_word_at(&chars, at, &term))
        })
        .map(|entry| entry.term.trim().to_string())
        .collect()
}

/// The capped comma list for Whisper, most recently added first, or `None` when
/// there is nothing to hint at.
///
/// Free — no extra call, no extra latency — and the only mechanism that can make
/// Whisper produce the right token before there is any variant to replace. It is
/// a hint and nothing more; [`apply`] is the mechanism.
pub fn prompt(entries: &[Entry]) -> Option<String> {
    let mut hint = String::new();

    for entry in entries.iter().rev() {
        let term = entry.term.trim();

        if term.is_empty() {
            continue;
        }

        let addition = if hint.is_empty() {
            term.len()
        } else {
            term.len() + 2
        };

        // Whole terms only. Half a term is a word Whisper has never seen.
        if hint.chars().count() + addition > MAX_PROMPT {
            break;
        }

        if !hint.is_empty() {
            hint.push_str(", ");
        }

        hint.push_str(term);
    }

    (!hint.is_empty()).then_some(hint)
}

/// Whisper sometimes returns the prompt itself on a near-silent take.
///
/// Only a transcript covering **more than one** term counts: a dictation of a
/// single term is indistinguishable from someone saying that term, and refusing
/// to type it would be worse than the echo it prevents. `MIN_PEAK` in
/// `session.rs` already drops most of the takes that cause this.
pub fn echoed(hint: &str, text: &str) -> bool {
    let spoken = loose(text);

    if !spoken.contains(',') {
        return false;
    }

    loose(hint).starts_with(&spoken)
}

/// Lowercased, whitespace-collapsed, and with the full stop Whisper adds on its
/// own taken off the end. Commas are kept — they are what separates the terms.
fn loose(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .trim_end_matches(['.', '!', '?'])
        .to_string()
}

/// `needle` at `at`, compared case-insensitively, with a non-alphanumeric
/// character on either side.
///
/// The boundary check is what keeps the feature safe in a real document:
/// `verbal` → `Vercel` must not fire inside "verbally".
fn matches_word_at(chars: &[char], at: usize, needle: &[char]) -> bool {
    let end = at + needle.len();

    if end > chars.len() {
        return false;
    }

    if at > 0 && chars[at - 1].is_alphanumeric() {
        return false;
    }

    if chars.get(end).is_some_and(|next| next.is_alphanumeric()) {
        return false;
    }

    chars[at..end]
        .iter()
        .zip(needle)
        .all(|(a, b)| a.eq_ignore_ascii_case(b) || a.to_lowercase().eq(b.to_lowercase()))
}

/// Lowercased and whitespace-collapsed, for clash checks only. Punctuation is
/// kept: `.net` and `net` are different entries, and trimming would merge them.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A corrupt file loads as no entries rather than failing the launch — dictation
/// works fine without a dictionary, and the next save overwrites the bad file.
pub fn load(app: &AppHandle) -> Vec<Entry> {
    let Some(path) = path(app) else {
        return Vec::new();
    };

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new(); // none saved yet
    };

    match serde_json::from_str::<Vec<Entry>>(&text) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!(
                "piplo: {} is not readable ({err}); starting with no vocabulary",
                path.display()
            );
            Vec::new()
        }
    }
}

/// Write, **then** adopt. A failed write leaves memory and disk in agreement,
/// both holding the last good state.
fn commit(app: &AppHandle, entries: Vec<Entry>) -> Result<Vec<Entry>, String> {
    write(app, &entries)?;

    *app.state::<Store>().lock() = entries.clone();

    Ok(entries)
}

fn write(app: &AppHandle, entries: &[Entry]) -> Result<(), String> {
    let path = path(app).ok_or_else(|| "no config directory".to_string())?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let json = serde_json::to_string_pretty(entries).map_err(|err| err.to_string())?;
    std::fs::write(&path, json).map_err(|err| err.to_string())
}

/// Next to `settings.json` and `snippets.json`, for the same reason: state that
/// should follow the install rather than the checkout.
fn path(app: &AppHandle) -> Option<PathBuf> {
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join(FILE)),
        Err(err) => {
            eprintln!("piplo: no config directory: {err}");
            None
        }
    }
}

/// Whether `variant` already belongs to `term`, compared normalized. Used by the
/// learner to keep a pair it has already saved out of the suggestions.
pub fn has_variant(entries: &[Entry], term: &str, variant: &str) -> bool {
    entries
        .iter()
        .filter(|entry| normalize(&entry.term) == normalize(term))
        .any(|entry| {
            entry
                .variants
                .iter()
                .any(|mine| normalize(mine) == normalize(variant))
        })
}

/// Add a variant to the entry for `term`, creating the entry when there is none.
///
/// The learner's only way in, and it goes through `commit` like everything else.
/// A term Piplo invented is marked `learned`; a term the user asked for keeps its
/// own provenance even when it picks up learned variants.
pub fn add_variant(app: &AppHandle, term: &str, variant: &str) -> Result<Vec<Entry>, String> {
    let term = term.trim();
    let variant = variant.trim();

    if term.is_empty() || variant.is_empty() {
        return Err("nothing to learn".into());
    }

    let mut entries = app.state::<Store>().get();

    match entries
        .iter_mut()
        .find(|entry| normalize(&entry.term) == normalize(term))
    {
        Some(entry) => {
            if entry
                .variants
                .iter()
                .any(|mine| normalize(mine) == normalize(variant))
            {
                return Ok(entries);
            }

            entry.variants.push(variant.to_string());
        }
        None => entries.push(Entry {
            id: uuid::Uuid::new_v4().to_string(),
            term: term.to_string(),
            variants: vec![variant.to_string()],
            source: "learned".to_string(),
        }),
    }

    commit(app, entries)
}

/// The other half of *Undo*: take the variant back out, leaving the term alone —
/// a term is still worth having as a hint even with nothing to replace.
pub fn remove_variant(app: &AppHandle, term: &str, variant: &str) -> Result<Vec<Entry>, String> {
    let mut entries = app.state::<Store>().get();
    let mut found = false;

    for entry in entries
        .iter_mut()
        .filter(|entry| normalize(&entry.term) == normalize(term))
    {
        let before = entry.variants.len();
        entry
            .variants
            .retain(|mine| normalize(mine) != normalize(variant));
        found |= entry.variants.len() != before;
    }

    if !found {
        return Ok(entries);
    }

    commit(app, entries)
}

#[tauri::command]
pub fn list_vocabulary(app: AppHandle) -> Vec<Entry> {
    app.state::<Store>().get()
}

/// Create and update both, keyed on `id`. One form, one branch, one place.
///
/// The backend is the only place that decides what is valid. The form pre-checks
/// the same rules for instant feedback, but never authoritatively.
#[tauri::command]
pub fn save_term(app: AppHandle, entry: Entry) -> Result<Vec<Entry>, String> {
    let term = entry.term.trim().to_string();

    if term.is_empty() {
        return Err("Give the entry a word to type.".into());
    }

    let mut variants: Vec<String> = Vec::new();

    for variant in &entry.variants {
        let variant = variant.trim().to_string();

        if variant.is_empty() {
            continue;
        }

        if normalize(&variant) == normalize(&term) {
            return Err(format!("'{variant}' is already the term."));
        }

        // Silently deduped rather than rejected: two identical variants in one
        // field is a typo, not a decision worth an error message.
        if !variants.iter().any(|kept| normalize(kept) == normalize(&variant)) {
            variants.push(variant);
        }
    }

    let mut entries = app.state::<Store>().get();

    // The row being edited is excluded from its own clash check, or saving it
    // unchanged would fail against itself.
    let others = || entries.iter().filter(|other| other.id != entry.id);

    if let Some(clash) = others().find(|other| normalize(&other.term) == normalize(&term)) {
        return Err(format!("'{}' is already here.", clash.term.trim()));
    }

    // A variant that two entries claim makes which term wins a matter of list
    // order — something the user never sees and cannot control.
    for variant in &variants {
        let owner = others().find(|other| {
            other
                .variants
                .iter()
                .any(|mine| normalize(mine) == normalize(variant))
        });

        if let Some(owner) = owner {
            return Err(format!(
                "'{variant}' already becomes '{}'.",
                owner.term.trim()
            ));
        }
    }

    // Removing a variant by hand is how you undo a learned one, so it has to mean
    // "and stop learning it" too — otherwise the next correction brings it
    // straight back, which is the first thing anyone asks after deleting one.
    // Typing a variant in is the user asking for it outright, so it also undoes an
    // earlier refusal of that pair — see `learn::allow`.
    let added: Vec<String> = match entries.iter().find(|other| other.id == entry.id) {
        Some(existing) => variants
            .iter()
            .filter(|new| {
                !existing
                    .variants
                    .iter()
                    .any(|old| normalize(old) == normalize(new))
            })
            .cloned()
            .collect(),
        None => variants.clone(),
    };

    let dropped = match entries.iter().position(|other| other.id == entry.id) {
        Some(index) => {
            let dropped: Vec<String> = entries[index]
                .variants
                .iter()
                .filter(|old| !variants.iter().any(|kept| normalize(kept) == normalize(old)))
                .cloned()
                .collect();

            entries[index].term = term.clone();
            entries[index].variants = variants;
            dropped
        }
        // Appended, which is what makes file order the created order that
        // Newest / Oldest sort on.
        None => {
            entries.push(Entry {
                id: uuid::Uuid::new_v4().to_string(),
                term: term.clone(),
                variants,
                source: manual(),
            });
            Vec::new()
        }
    };

    let saved = commit(&app, entries)?;

    // After the write: a rejection for a variant that is still on disk, or an
    // acceptance of one that failed to save, would be a lie about what the
    // dictionary contains.
    crate::learn::forget(&app, &term, &dropped);
    crate::learn::allow(&app, &term, &added);

    Ok(saved)
}

#[tauri::command]
pub fn delete_term(app: AppHandle, id: String) -> Result<Vec<Entry>, String> {
    let mut entries = app.state::<Store>().get();

    // Same reasoning as an edit that drops a variant: deleting the entry has to
    // stop the learning that would recreate it.
    let gone = entries
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| (entry.term.clone(), entry.variants.clone()));

    entries.retain(|entry| entry.id != id);
    let saved = commit(&app, entries)?;

    if let Some((term, variants)) = gone {
        crate::learn::forget(&app, &term, &variants);
    }

    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text and whether anything changed — what most of these tests care
    /// about. `session.rs` needs the replacements themselves, so that is what the
    /// real function returns.
    fn apply(entries: &[Entry], text: &str) -> (String, bool) {
        let (text, fired) = apply_tracked(entries, text);
        (text, !fired.is_empty())
    }

    fn entry(term: &str, variants: &[&str]) -> Entry {
        Entry {
            id: term.to_string(),
            term: term.to_string(),
            variants: variants.iter().map(|v| v.to_string()).collect(),
            source: manual(),
        }
    }

    #[test]
    fn replaces_a_variant_with_the_term_verbatim() {
        let entries = vec![entry("ZentelAI", &["gentle AI"])];
        let (text, changed) = apply(&entries, "I need to check gentle AI.");

        assert_eq!(text, "I need to check ZentelAI.");
        assert!(changed);
    }

    #[test]
    fn is_case_insensitive_going_in_and_exact_coming_out() {
        let entries = vec![entry("ZentelAI", &["gentle ai"])];

        assert_eq!(
            apply(&entries, "Gentle AI and GENTLE AI.").0,
            "ZentelAI and ZentelAI."
        );
    }

    #[test]
    fn matches_whole_words_only() {
        // The case that makes the feature safe in a real document.
        let entries = vec![entry("Vercel", &["verbal"])];

        assert_eq!(apply(&entries, "we agreed verbally").0, "we agreed verbally");
        assert_eq!(apply(&entries, "a verbal agreement").0, "a Vercel agreement");
        assert!(!apply(&entries, "we agreed verbally").1);
    }

    #[test]
    fn longest_variant_wins_whatever_the_order() {
        let short = entry("ZentelAI", &["gentle AI"]);
        let long = entry("ZentelAI Studio", &["gentle AI studio"]);

        for entries in [
            vec![short.clone(), long.clone()],
            vec![long.clone(), short.clone()],
        ] {
            assert_eq!(
                apply(&entries, "open gentle AI studio now").0,
                "open ZentelAI Studio now"
            );
        }
    }

    #[test]
    fn does_not_rescan_a_replaced_span() {
        // `ZentelAI` contains `ai`, which is another entry's variant. Rescanning
        // the output would cascade the replacement into the term it just wrote.
        let entries = vec![
            entry("ZentelAI", &["gentle AI"]),
            entry("A.I.", &["zentelai"]),
        ];

        assert_eq!(apply(&entries, "check gentle AI").0, "check ZentelAI");
    }

    #[test]
    fn touches_nothing_else() {
        let entries = vec![entry("Next.js", &["next JS"])];
        let (text, changed) = apply(&entries, "  we use   next js, and  nothing else. ");

        assert_eq!(text, "  we use   Next.js, and  nothing else. ");
        assert!(changed);
    }

    #[test]
    fn reports_no_change_when_the_text_already_reads_right() {
        let entries = vec![entry("ZentelAI", &["Zentel AI"])];

        assert!(!apply(&entries, "I checked ZentelAI today").1);
        assert!(apply(&entries, "I checked Zentel AI today").1);
    }

    #[test]
    fn an_empty_dictionary_changes_nothing() {
        assert_eq!(apply(&[], "anything at all"), ("anything at all".into(), false));
        assert_eq!(
            apply(&[entry("Piplo", &[])], "anything at all"),
            ("anything at all".into(), false)
        );
    }

    #[test]
    fn a_fix_can_be_put_back_exactly() {
        let entries = vec![entry("ZentelAI", &["gently"])];
        let (typed, fired) = apply_tracked(&entries, "I need to act like gently.");

        assert_eq!(typed, "I need to act like ZentelAI.");

        // Grammar runs in between, so the reversal works on its output — the
        // cleanup's changes have to survive, only the term goes back.
        let cleaned = "I need to act like ZentelAI, please.";
        assert_eq!(
            revert(cleaned, &fired),
            "I need to act like gently, please."
        );

        // Nothing fired, nothing to put back.
        assert_eq!(revert(cleaned, &[]), cleaned);
    }

    #[test]
    fn reverting_leaves_other_words_alone() {
        let fired = vec![Fired {
            variant: "gently".into(),
            term: "ZentelAI".into(),
        }];

        // Whole words only, on the way back as well as forward.
        assert_eq!(
            revert("ZentelAIx and ZentelAI", &fired),
            "ZentelAIx and gently"
        );
    }

    #[test]
    fn the_prompt_is_newest_first_and_capped() {
        let entries = vec![entry("Piplo", &[]), entry("ZentelAI", &[])];

        assert_eq!(prompt(&entries), Some("ZentelAI, Piplo".into()));
        assert_eq!(prompt(&[]), None);

        // Whole terms only, and never past the cap.
        let long: Vec<Entry> = (0..40).map(|n| entry(&format!("Term{n:02}"), &[])).collect();
        let hint = prompt(&long).expect("terms to hint at");

        assert!(hint.chars().count() <= MAX_PROMPT);
        assert!(hint.split(", ").all(|term| term.len() == 6));
    }

    #[test]
    fn only_the_terms_present_reach_grammar() {
        let entries = vec![
            entry("ZentelAI", &[]),
            entry("Next.js", &[]),
            entry("Prisma", &[]),
        ];

        assert_eq!(
            terms_in(&entries, "ZentelAI ships on next.js"),
            vec!["ZentelAI".to_string(), "Next.js".to_string()]
        );
        assert!(terms_in(&entries, "nothing familiar here").is_empty());
    }

    #[test]
    fn an_echoed_prompt_is_not_a_transcript() {
        let hint = "ZentelAI, Piplo, Tauri";

        assert!(echoed(hint, "ZentelAI, Piplo, Tauri."));
        assert!(echoed(hint, "zentelai, piplo"));

        // A real dictation of one term must still be typed.
        assert!(!echoed(hint, "ZentelAI"));
        assert!(!echoed(hint, "I need to check ZentelAI, then Piplo."));
    }
}
