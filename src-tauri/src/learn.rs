//! Learning a variant from a correction the user made **in Piplo**.
//!
//! The seam is deliberate and it is the whole feature: a history row is edited,
//! the edit is compared with what was typed, and a mapping is extracted. Piplo
//! does not watch what you type in other applications — that needs a keyboard
//! hook or accessibility reads, both of which are on the do-not list.
//!
//! Nothing here runs in the dictation path. `session.rs` does not know this
//! module exists, so a bug in the learner cannot cost anyone a dictation.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{history, settings, vocabulary};

const FILE: &str = "corrections.jsonl";

/// Rewrites, and two unrelated edits in one line, are not corrections.
const MAX_WORDS: usize = 3;

/// Two is a coincidence, four is a user who has given up.
const LEARN_AT: usize = 3;

/// Shown under Suggested from here, with *Add* and *Never*.
const SUGGEST_AT: usize = 2;

/// One correction: what was typed, and what the user made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    pub from: String,
    pub to: String,
}

/// A candidate the user has not been asked about yet.
#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub from: String,
    pub to: String,
    pub count: usize,
}

/// What a correction did, so the row that was just edited can say so.
///
/// `Counted` is deliberately silent in the UI — the first two occurrences are
/// quiet by design. `Refused` is not: a correction that vanished with no
/// explanation is the one outcome a user cannot make sense of, and the only one
/// they can act on.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    /// Not a term, too wide an edit, a snippet expansion, or learning is off.
    Nothing,
    Counted { count: usize },
    Learned {
        term: String,
        from: String,
        count: usize,
    },
    /// The pair was refused before — by *Never*, or by deleting it.
    Refused { from: String, to: String },
}

/// One line per event, append-only — no read-modify-write on a file the user may
/// be looking at, and the feature's real behaviour stays greppable.
#[derive(Debug, Serialize, Deserialize)]
struct Event {
    at: String,
    from: String,
    to: String,
    /// `observed` or `rejected`. Counts are derived from these.
    status: String,
}

/// The minimal span covering **every** change, word by word.
///
/// There is no diff algorithm here and there does not need to be one:
/// prefix/suffix trimming means two edits far apart produce one very wide span,
/// and a wide span is refused by the word limit below. One rule, two jobs.
pub fn candidate(typed: &str, edited: &str) -> Option<Mapping> {
    let typed: Vec<&str> = typed.split_whitespace().collect();
    let edited: Vec<&str> = edited.split_whitespace().collect();

    let mut start = 0;
    while start < typed.len() && start < edited.len() && typed[start] == edited[start] {
        start += 1;
    }

    let mut end = 0;
    while end < typed.len() - start
        && end < edited.len() - start
        && typed[typed.len() - 1 - end] == edited[edited.len() - 1 - end]
    {
        end += 1;
    }

    let from = tidy(&typed[start..typed.len() - end]);
    let to = tidy(&edited[start..edited.len() - end]);

    // Pure insertions and deletions are writing, not correcting.
    if from.is_empty() || to.is_empty() {
        return None;
    }

    let from_words = from.split_whitespace().count();
    let to_words = to.split_whitespace().count();

    if from_words > MAX_WORDS || to_words > MAX_WORDS {
        return None;
    }

    // "very good" → "excellent, genuinely first rate" is a rewrite.
    if from_words.max(to_words) > 2 * from_words.min(to_words) {
        return None;
    }

    if !looks_like_a_term(&to, &from, start == 0) {
        return None;
    }

    Some(Mapping { from, to })
}

/// Whether the replacement looks like a name rather than a content edit.
///
/// The load-bearing rule. Capitalisation is what separates a proper noun from a
/// rewrite, and it is exactly what a user fixes when they correct a brand name —
/// which is why "yesterday" → "last night", "he go" → "he goes" and "very good" →
/// "excellent" all fail it. The cost is that an all-lowercase name like `shadcn`
/// is never learned on its own; Piplo would rather miss it than learn "excellent".
pub fn looks_like_a_term(to: &str, from: &str, first_word: bool) -> bool {
    // `S3`, `qwen3`.
    if to.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }

    for word in to.split_whitespace() {
        let chars: Vec<char> = word.chars().collect();

        // `ZentelAI`, `MongoDB`, `PostgreSQL`.
        if chars.iter().skip(1).any(|c| c.is_uppercase()) {
            return true;
        }

        // `Next.js`, `shadcn-ui`. Internal only — the ends are sentence
        // punctuation, which `tidy` has already taken off.
        if chars.len() > 2
            && chars[1..chars.len() - 1]
                .iter()
                .any(|c| matches!(c, '.' | '-' | '_' | '/'))
        {
            return true;
        }
    }

    // `prism` → `Prisma`, `verbal` → `Vercel`. Whisper capitalises sentence
    // starts on its own, so the first word of the utterance cannot count.
    !first_word && starts_upper(to) && !starts_upper(from)
}

fn starts_upper(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase)
}

/// The span as a phrase, without the sentence punctuation that happened to sit at
/// its edges. Internal punctuation is part of the word — `Next.js` depends on it.
fn tidy(words: &[&str]) -> String {
    words
        .join(" ")
        .trim_matches(|c: char| {
            c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\'' | '(' | ')' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}')
        })
        .to_string()
}

/// Counted, not scored: a confidence float derived from a count is a number
/// nobody can act on.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    count: usize,
    rejected: bool,
}

/// Derived by reading the ledger — the `observed` lines for a pair, unless a
/// later `rejected` line supersedes them.
fn tallies(app: &AppHandle) -> HashMap<(String, String), Tally> {
    let mut tallies: HashMap<(String, String), Tally> = HashMap::new();

    for event in read(app) {
        let tally = tallies.entry(key(&event.from, &event.to)).or_default();

        match event.status.as_str() {
            "observed" => tally.count += 1,
            "rejected" => {
                *tally = Tally {
                    count: 0,
                    rejected: true,
                }
            }
            // An explicit add is consent, and it supersedes an earlier refusal —
            // otherwise "no" is permanent with no way back, and a user who
            // deleted a word to start over can never teach it again.
            "accepted" => tally.rejected = false,
            other => eprintln!("piplo: unknown correction status {other:?}"),
        }
    }

    tallies
}

/// Pairs are compared normalized, so "Gentle AI" and "gentle ai" are one pair.
fn key(from: &str, to: &str) -> (String, String) {
    (from.trim().to_lowercase(), to.trim().to_lowercase())
}

/// Record one correction, and promote it once it has been seen three times.
///
/// Returns what was learned, if anything — the first two occurrences are
/// deliberately quiet: a single correction is the most likely one to be a typo or
/// a one-off name.
pub fn record(app: &AppHandle, typed: &str, edited: &str) -> Outcome {
    let Some(mapping) = candidate(typed, edited) else {
        return Outcome::Nothing;
    };

    let tally = tallies(app)
        .get(&key(&mapping.from, &mapping.to))
        .copied()
        .unwrap_or_default();

    // A pair the user already said no to is not counted again — but they are told
    // so, and offered the way back.
    if tally.rejected {
        println!("piplo: ignoring a rejected correction — {}", mapping.from);
        return Outcome::Refused {
            from: mapping.from,
            to: mapping.to,
        };
    }

    if let Err(err) = append(app, &mapping, "observed") {
        eprintln!("piplo: could not record the correction: {err}");
        return Outcome::Nothing;
    }

    let count = tally.count + 1;
    println!(
        "piplo: correction {} → {} (seen {count})",
        mapping.from, mapping.to
    );

    if count < LEARN_AT {
        return Outcome::Counted { count };
    }

    match vocabulary::add_variant(app, &mapping.to, &mapping.from) {
        Ok(_) => Outcome::Learned {
            term: mapping.to,
            from: mapping.from,
            count,
        },
        Err(err) => {
            eprintln!("piplo: could not save the learned variant: {err}");
            Outcome::Nothing
        }
    }
}

/// Pairs seen twice that are neither rejected nor already in the dictionary.
pub fn suggestions(app: &AppHandle) -> Vec<Suggestion> {
    let entries = vocabulary::snapshot(app);

    let mut found: Vec<Suggestion> = tallies(app)
        .into_iter()
        .filter(|(_, tally)| !tally.rejected && tally.count >= SUGGEST_AT)
        .filter(|((from, to), _)| !vocabulary::has_variant(&entries, to, from))
        .map(|((from, to), tally)| Suggestion {
            from,
            to,
            count: tally.count,
        })
        .collect();

    // Most-corrected first, then stable — a HashMap has no order of its own and a
    // list that reshuffles on every render is unusable.
    found.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.to.cmp(&b.to)));
    found
}

/// *Never*, and *Undo* on a row that just learned something. Removes the variant
/// if it is already in the dictionary, so the two are one action.
pub fn reject(app: &AppHandle, mapping: &Mapping) -> Result<(), String> {
    append(app, mapping, "rejected")?;

    if let Err(err) = vocabulary::remove_variant(app, &mapping.to, &mapping.from) {
        eprintln!("piplo: could not remove the rejected variant: {err}");
    }

    Ok(())
}

/// Deleting a variant is the answer to "how do I undo this?", so it also writes
/// the rejection — otherwise the next correction brings it straight back, which is
/// the first thing anyone asks after deleting one.
///
/// **Only for pairs Piplo actually put in front of the user** — suggested or
/// learned. Below that threshold the pair has never been shown or applied, so
/// there is nothing to undo, and rejecting it would silently blacklist a
/// correction Piplo has not even offered yet. Deleting a hand-typed variant, or an
/// entry created while experimenting, must not cost the user the feature for that
/// word permanently and invisibly.
///
/// Called from the vocabulary commands. It is the one call in the other direction,
/// and it cannot change what a dictation does.
pub fn forget(app: &AppHandle, term: &str, variants: &[String]) {
    if variants.is_empty() {
        return;
    }

    let tallies = tallies(app);

    for variant in variants {
        let mapping = Mapping {
            from: variant.clone(),
            to: term.to_string(),
        };

        let surfaced = tallies
            .get(&key(&mapping.from, &mapping.to))
            .is_some_and(|tally| tally.count >= SUGGEST_AT);

        if !surfaced {
            continue;
        }

        if let Err(err) = append(app, &mapping, "rejected") {
            eprintln!("piplo: could not record the rejection: {err}");
        }
    }
}

/// The user asked for this pair outright — from the Vocabulary form, or *Add* on a
/// suggestion. Recorded so an earlier refusal stops applying: "never" has to be
/// reversible, or deleting a word to start over locks it out for good.
///
/// Only written for pairs that were actually refused, so the ledger does not fill
/// with a line per hand-typed variant.
pub fn allow(app: &AppHandle, term: &str, variants: &[String]) {
    if variants.is_empty() {
        return;
    }

    let tallies = tallies(app);

    for variant in variants {
        let mapping = Mapping {
            from: variant.clone(),
            to: term.to_string(),
        };

        let refused = tallies
            .get(&key(&mapping.from, &mapping.to))
            .is_some_and(|tally| tally.rejected);

        if !refused {
            continue;
        }

        println!("piplo: allowing {} → {} again", mapping.from, mapping.to);

        if let Err(err) = append(app, &mapping, "accepted") {
            eprintln!("piplo: could not record the acceptance: {err}");
        }
    }
}

/// A line that will not parse is skipped and the rest of the file is read.
fn read(app: &AppHandle) -> Vec<Event> {
    let Some(path) = path(app) else {
        return Vec::new();
    };

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new(); // nothing corrected yet
    };

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<Event>(line) {
            Ok(event) => Some(event),
            Err(err) => {
                eprintln!("piplo: skipping malformed correction line: {err}");
                None
            }
        })
        .collect()
}

fn append(app: &AppHandle, mapping: &Mapping, status: &str) -> Result<(), String> {
    let path = path(app).ok_or_else(|| "no config directory".to_string())?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let event = Event {
        at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::from("unknown")),
        from: mapping.from.clone(),
        to: mapping.to.clone(),
        status: status.to_string(),
    };

    let line = serde_json::to_string(&event).map_err(|err| err.to_string())?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| err.to_string())?;

    file.write_all(format!("{line}\n").as_bytes())
        .map_err(|err| err.to_string())
}

/// Next to `vocabulary.json`, and never in the repo.
fn path(app: &AppHandle) -> Option<PathBuf> {
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join(FILE)),
        Err(err) => {
            eprintln!("piplo: no config directory: {err}");
            None
        }
    }
}

/// The history row's save.
///
/// The rewrite happens whatever the learning setting says — the user asked for
/// their text to be corrected, and that is not the same request as teaching Piplo
/// a word.
#[tauri::command]
pub fn record_correction(app: AppHandle, id: String, edited: String) -> Result<Outcome, String> {
    let row = history::correct(&app, &id, &edited)?;

    if !settings::current(&app).learn_from_corrections {
        return Ok(Outcome::Nothing);
    }

    // Editing canned text is editing the snippet, not the transcript.
    if row.snippet {
        println!("piplo: a snippet expansion teaches nothing");
        return Ok(Outcome::Nothing);
    }

    Ok(record(&app, &row.typed, &edited))
}

#[tauri::command]
pub fn list_suggestions(app: AppHandle) -> Vec<Suggestion> {
    suggestions(&app)
}

/// *Add*. Returns the whole dictionary, like every other mutation.
#[tauri::command]
pub fn accept_suggestion(
    app: AppHandle,
    mapping: Mapping,
) -> Result<Vec<vocabulary::Entry>, String> {
    let entries = vocabulary::add_variant(&app, &mapping.to, &mapping.from)?;
    allow(&app, &mapping.to, std::slice::from_ref(&mapping.from));
    Ok(entries)
}

/// *Never*, and *Undo*.
#[tauri::command]
pub fn reject_suggestion(app: AppHandle, mapping: Mapping) -> Result<Vec<Suggestion>, String> {
    reject(&app, &mapping)?;
    Ok(suggestions(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_span_between_a_common_prefix_and_suffix() {
        assert_eq!(
            candidate("I worked with gentle AI on Monday", "I worked with ZentelAI on Monday"),
            Some(Mapping {
                from: "gentle AI".into(),
                to: "ZentelAI".into()
            })
        );
    }

    #[test]
    fn ignores_the_punctuation_at_the_edges() {
        assert_eq!(
            candidate("I need to check gentle AI.", "I need to check ZentelAI."),
            Some(Mapping {
                from: "gentle AI".into(),
                to: "ZentelAI".into()
            })
        );
    }

    #[test]
    fn learns_a_name_a_capital_gives_away() {
        assert_eq!(
            candidate("we deploy on verbal", "we deploy on Vercel"),
            Some(Mapping {
                from: "verbal".into(),
                to: "Vercel".into()
            })
        );
        assert_eq!(
            candidate("the prism schema", "the Prisma schema"),
            Some(Mapping {
                from: "prism".into(),
                to: "Prisma".into()
            })
        );
    }

    #[test]
    fn learns_a_name_a_digit_or_a_dot_gives_away() {
        assert!(candidate("upload it to s three", "upload it to S3").is_some());
        assert!(candidate("built with next js", "built with Next.js").is_some());
    }

    #[test]
    fn refuses_an_ordinary_content_edit() {
        // The check the whole feature's credibility rests on.
        assert_eq!(candidate("we shipped yesterday", "we shipped last night"), None);
        assert_eq!(candidate("he go to the office", "he goes to the office"), None);
        assert_eq!(candidate("it was very good", "it was excellent"), None);
    }

    #[test]
    fn refuses_a_wide_edit() {
        assert_eq!(
            candidate(
                "we shipped yesterday and told the team",
                "we shipped last night and told the crew"
            ),
            None
        );
    }

    #[test]
    fn refuses_an_insertion_or_a_deletion() {
        assert_eq!(candidate("ship it Monday", "ship it Monday please"), None);
        assert_eq!(candidate("ship it Monday please", "ship it Monday"), None);
        assert_eq!(candidate("ship it Monday", "ship it Monday"), None);
    }

    #[test]
    fn refuses_a_rewrite_of_very_different_lengths() {
        assert_eq!(candidate("it was good", "it was Really Quite Good"), None);
    }

    #[test]
    fn a_capital_on_the_first_word_is_whispers_own() {
        // Whisper capitalises sentence starts, so this is not evidence of a name.
        assert_eq!(candidate("prism is the orm", "Prisma is the orm"), None);
        // The same word later in the sentence is.
        assert!(candidate("the orm is prism", "the orm is Prisma").is_some());
    }

    #[test]
    fn the_term_test_reads_the_replacement_not_the_original() {
        assert!(looks_like_a_term("ZentelAI", "gentle AI", false));
        assert!(looks_like_a_term("Next.js", "next js", true));
        assert!(!looks_like_a_term("last night", "yesterday", false));
        assert!(!looks_like_a_term("Prisma", "prism", true));
    }
}
