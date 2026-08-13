//! The only module that knows the order.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, Recorder};
use crate::{grammar, groq, history, insert, settings, snippets, vocabulary, widget};

/// How long an error sits on the pill before it returns to idle.
const ERROR_LINGER: Duration = Duration::from_millis(2500);

/// Longer, because "copied instead" is an instruction and not just a report.
const RESCUE_LINGER: Duration = Duration::from_millis(6000);

/// A brushed key should not cost an API call.
const MIN_DURATION: Duration = Duration::from_millis(300);

/// Nor should a recording that never rose above room noise.
const MIN_PEAK: f32 = 0.02;

/// The single tagged state the frontend renders from.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    Idle,
    Recording,
    Transcribing,
    Error { message: String },
}

/// How a session was started. Only the matching trigger can finish it: without
/// this, letting go of a key you happened to be holding would kill a
/// mouse-started take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Shortcut,
    Pointer,
}

/// Bumped on every start and cancel. Captured before each await and compared
/// after: without it, cancel-then-immediately-record types the previous take.
#[derive(Default)]
pub struct Generation(AtomicU64);

struct InFlight {
    recorder: Recorder,
    trigger: Trigger,
    started: Instant,
    generation: u64,
}

/// One at a time, enforced here rather than debounced in the UI — the shortcut,
/// the mic button and the menu are three entry points and only one place should
/// decide.
#[derive(Default)]
pub struct Active(Mutex<Option<InFlight>>);

/// The last dictation that had a word replaced, kept so the widget menu can undo
/// it in the app it was typed into.
///
/// Both versions of the text, and which one is on screen. Nothing is ever read
/// back from the target application — Piplo only knows what it typed itself, which
/// is why this is state here rather than an accessibility call.
struct Fix {
    typed: String,
    reverted: String,
    undone: bool,
    /// For the menu's label. The first replacement is enough: one is the normal
    /// case, and a list would not fit a menu row.
    variant: String,
    term: String,
}

#[derive(Default)]
pub struct LastFix(Mutex<Option<Fix>>);

impl LastFix {
    fn lock(&self) -> MutexGuard<'_, Option<Fix>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// What the menu needs to draw the item, or `None` when there is nothing to undo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixView {
    /// The word that is on screen now.
    pub from: String,
    /// What it would become.
    pub to: String,
    /// True once undone, so the item reads as a redo.
    pub undone: bool,
}

impl Active {
    fn lock(&self) -> MutexGuard<'_, Option<InFlight>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

pub fn start(app: &AppHandle, trigger: Trigger) {
    let active = app.state::<Active>();
    let mut slot = active.lock();

    if slot.is_some() {
        return;
    }

    let generation = bump(app);

    match Recorder::start(app.clone()) {
        Ok(recorder) => {
            *slot = Some(InFlight {
                recorder,
                trigger,
                started: Instant::now(),
                generation,
            });
            drop(slot);

            widen(app);
            set_status(app, Status::Recording);
            println!("piplo: session start ({trigger:?})");
        }
        Err(err) => {
            drop(slot);

            eprintln!("piplo: could not start capture: {err} — {}", err.detail());
            fail(app, err.to_string());
        }
    }
}

pub fn finish(app: &AppHandle, trigger: Trigger) {
    let session = {
        let active = app.state::<Active>();
        let mut slot = active.lock();

        match slot.as_ref() {
            // A key-up must not end a session the mouse started.
            Some(in_flight) if in_flight.trigger != trigger => return,
            Some(_) => slot.take(),
            None => return,
        }
    };

    let Some(session) = session else {
        return;
    };

    let held = session.started.elapsed();
    let generation = session.generation;
    let (samples, sample_rate) = session.recorder.stop();

    let _ = app.emit("level", 0.0_f32);

    let peak = samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()));

    if held < MIN_DURATION || peak < MIN_PEAK {
        println!(
            "piplo: ignoring tap — {}ms, peak {peak:.3}",
            held.as_millis()
        );
        go_idle(app);
        return;
    }

    let seconds = samples.len() as f32 / sample_rate.max(1) as f32;
    println!("piplo: session stop — {seconds:.2}s at {sample_rate} Hz");

    set_status(app, Status::Transcribing);

    let downsampled = audio::to_16k(&samples, sample_rate);

    let wav = match audio::encode_wav(&downsampled) {
        Ok(wav) => wav,
        Err(err) => {
            eprintln!("piplo: could not encode wav: {err} — {}", err.detail());
            fail(app, err.to_string());
            return;
        }
    };

    // Tauri's tokio runtime is already here, so no second runtime.
    let app = app.clone();
    tauri::async_runtime::spawn(deliver(app, wav, generation, seconds));
}

pub fn cancel(app: &AppHandle) {
    let session = app.state::<Active>().lock().take();

    // Bump even when nothing is recording: a cancel during transcription must
    // invalidate the result that is already in flight.
    bump(app);

    if let Some(session) = session {
        // The samples go out of scope unread. No API call, nothing logged.
        let _ = session.recorder.stop();
    }

    let _ = app.emit("level", 0.0_f32);
    go_idle(app);
    println!("piplo: session cancelled");
}

async fn deliver(app: AppHandle, wav: Vec<u8>, generation: u64, seconds: f32) {
    // Read per dictation, not cached at startup, so a key saved in Settings
    // applies to the very next take.
    let key = crate::credentials::current(&app);

    let Some(key) = key else {
        eprintln!("piplo: no Groq API key");
        fail(&app, "No API key".into());
        return;
    };

    // Read once, so both `apply` passes and both prompts in this dictation see the
    // same dictionary even if the page saves halfway through.
    let terms = vocabulary::snapshot(&app);
    let hint = vocabulary::prompt(&terms);

    let result = groq::transcribe(&key, wav, hint.clone()).await;

    if stale(&app, generation) {
        println!("piplo: discarding stale transcription");
        return;
    }

    let transcription = match result {
        Ok(transcription) => transcription,
        Err(err) => {
            eprintln!("piplo: transcription failed: {err} — {}", err.detail());
            fail(&app, err.to_string());
            return;
        }
    };

    let heard = transcription.text.trim().to_string();

    if heard.is_empty() {
        println!("piplo: nothing transcribed");
        go_idle(&app);
        return;
    }

    // A near-silent take can come back as the hint itself. Typing the user's own
    // dictionary at them is worse than typing nothing.
    if hint.as_deref().is_some_and(|hint| vocabulary::echoed(hint, &heard)) {
        println!("piplo: transcript was the prompt hint — {heard}");
        go_idle(&app);
        return;
    }

    println!("piplo: transcript — {heard}");

    // Before the snippet check, because a trigger can contain a term: a misheard
    // brand name would otherwise turn the whole shorthand into a typed sentence.
    let (raw, mut fired) = vocabulary::apply_tracked(&terms, &heard);
    let said_wrong = !fired.is_empty();

    if said_wrong {
        println!("piplo: vocabulary — {raw}");
    }

    // Triggers are short and clean, so this is the check that usually fires. A
    // hit skips grammar entirely, taking a network call out of the most repeated
    // action in the app.
    let expanded = snippets::expand(&app, &raw);

    // Read here rather than cached at startup, so flipping the toggle applies to
    // the very next dictation. Status stays Transcribing across both calls:
    // nothing on screen should reveal that there are two rather than one.
    let cleaned = if expanded.is_none() && settings::current(&app).grammar_enabled {
        // Only the terms this transcript actually contains — see VOCABULARY.md.
        grammar::run(&key, &raw, &vocabulary::terms_in(&terms, &raw)).await
    } else {
        None
    };

    // Checked after the second call too, so ✕ during the extra round trip
    // discards the result exactly as it does for transcription.
    if stale(&app, generation) {
        println!("piplo: discarding stale dictation");
        return;
    }

    let corrected = cleaned.is_some();
    let mut spoken = cleaned.unwrap_or_else(|| raw.clone());
    let mut vocabulary = said_wrong;

    if corrected {
        println!("piplo: cleaned — {spoken}");

        // Again, because the cleanup sometimes puts the mistake back —
        // `ZentelAI` → `Zentel AI` is the common one. An in-memory scan over a
        // few dozen strings, so the second pass costs nothing.
        let (fixed, again) = vocabulary::apply_tracked(&terms, &spoken);

        if !again.is_empty() {
            println!("piplo: vocabulary again — {fixed}");
            spoken = fixed;
            vocabulary = true;
            fired.extend(again);
        }
    }

    // The second check, and only when cleanup happened: grammar sometimes fixes a
    // trigger into matchability — "my e-mail" → "my email". Without it, a
    // mishearing silently turns a snippet into a typed sentence.
    let snippet = match expanded {
        Some(content) => Some(content),
        None if corrected => snippets::expand(&app, &spoken),
        None => None,
    };

    let is_snippet = snippet.is_some();

    // A match replaces the whole utterance: the user said a shorthand, not a
    // sentence they wanted typed.
    let text = match snippet {
        Some(content) => {
            println!("piplo: snippet — {content}");
            content
        }
        None => spoken,
    };

    // Blocking: it polls for modifiers and then paces the synthesised input.
    let typed = text.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || insert::type_text(&typed))
        .await
        .unwrap_or_else(|err| {
            eprintln!("piplo: typing task failed: {err}");
            insert::Insert::Blocked("the typing step failed")
        });

    let inserted = outcome == insert::Insert::Typed;

    // Only what Piplo typed itself, and only while it is the most recent thing it
    // typed. A snippet expansion is not a word fix, so it is not offered.
    remember_fix(&app, &text, &fired, inserted && !is_snippet);

    // Whisper's own words, not the corrected ones: `raw_text` is what was heard,
    // and the vocabulary pass is one of the things it should be compared against.
    // Only when it differs — an unchanged transcript has nothing to compare.
    let raw_text = (text != heard).then_some(heard);

    // Written before anything is shown on the pill, and with the honest
    // `inserted` value. The history entry is the last line of defence: whatever
    // happens next, the dictation exists somewhere the user can get at it.
    let mut entry = history::Entry::now(text.clone(), raw_text);
    entry.duration_secs = transcription.duration.map(|d| d as f32).unwrap_or(seconds);
    entry.language = transcription.language;
    entry.inserted = inserted;
    entry.corrected = corrected;
    entry.snippet = is_snippet;
    entry.vocabulary = vocabulary;
    history::append(&app, &entry);

    if let insert::Insert::Blocked(why) = outcome {
        rescue(&app, &text, why);
        return;
    }

    go_idle(&app);
}

/// The dictation could not be typed. Get it somewhere reachable and say so.
///
/// The clipboard is deliberately untouched on the happy path — clobbering what
/// the user had copied is its own small data loss. It is only the lesser evil
/// than losing what they just spoke.
fn rescue(app: &AppHandle, text: &str, why: &str) {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let message = match app.clipboard().write_text(text.to_string()) {
        Ok(()) => {
            println!("piplo: not typed ({why}); copied to the clipboard");
            format!("{why} — copied instead, press paste")
        }
        Err(err) => {
            eprintln!("piplo: could not copy the dictation either: {err}");
            format!("{why} — saved to history")
        }
    };

    // Its own linger: this message asks the user to go and do something, and
    // 2.5s is not long enough to read it, let alone act.
    fail_for(app, message, RESCUE_LINGER);
}

/// Remember — or forget — the fix the menu can undo.
///
/// Cleared on any dictation that replaced nothing, because the offer is always
/// about the *last* thing typed: leaving a stale one there would rub out text it
/// did not write.
fn remember_fix(app: &AppHandle, text: &str, fired: &[vocabulary::Fired], keep: bool) {
    let state = app.state::<LastFix>();
    let mut slot = state.lock();

    let Some(first) = fired.first().filter(|_| keep) else {
        *slot = None;
        return;
    };

    *slot = Some(Fix {
        typed: text.to_string(),
        reverted: vocabulary::revert(text, fired),
        undone: false,
        variant: first.variant.clone(),
        term: first.term.clone(),
    });
}

/// Whether the menu has a fifth item to make room for.
pub fn has_word_fix(app: &AppHandle) -> bool {
    let state = app.state::<LastFix>();
    let slot = state.lock();
    slot.is_some()
}

/// What the widget menu should offer, if anything.
#[tauri::command]
pub fn last_word_fix(app: AppHandle) -> Option<FixView> {
    let state = app.state::<LastFix>();
    let slot = state.lock();
    let fix = slot.as_ref()?;

    Some(if fix.undone {
        FixView {
            from: fix.variant.clone(),
            to: fix.term.clone(),
            undone: true,
        }
    } else {
        FixView {
            from: fix.term.clone(),
            to: fix.variant.clone(),
            undone: false,
        }
    })
}

/// Rub out what Piplo typed and type the other version instead — the word Whisper
/// actually heard, or the term again on a second press.
///
/// This assumes the caret has not moved since the dictation, which is why it is
/// only ever offered for the most recent one and is worded as an undo of it. Piplo
/// does not read the target application to check; that is the boundary the whole
/// vocabulary feature is built against.
#[tauri::command]
pub async fn undo_word_fix(app: AppHandle) -> Result<(), String> {
    toggle_word_fix(app).await
}

/// The shortcut's way in. Fire-and-forget: a hotkey has nobody to report to, so
/// the reason lands in the log.
pub fn toggle_from_shortcut(app: &AppHandle) {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(why) = toggle_word_fix(app).await {
            eprintln!("piplo: could not undo the word fix — {why}");
        }
    });
}

async fn toggle_word_fix(app: AppHandle) -> Result<(), String> {
    // The keystrokes go wherever focus is. If that is Piplo's own window, the undo
    // would rub out part of the history list instead of the user's document.
    if crate::platform::foreground_is_ours() {
        return Err("click back into your document first".into());
    }

    // Out of the way before anything is typed, so the user sees the change rather
    // than a menu sitting over it.
    crate::menu::hide(&app);

    let (count, target) = {
        let state = app.state::<LastFix>();
        let slot = state.lock();
        let fix = slot.as_ref().ok_or("nothing to undo")?;

        let on_screen = if fix.undone { &fix.reverted } else { &fix.typed };
        let target = if fix.undone {
            fix.typed.clone()
        } else {
            fix.reverted.clone()
        };

        (on_screen.chars().count(), target)
    };

    println!("piplo: word fix — erasing {count} chars, typing {target:?}");

    // Blocking: it waits for modifiers and paces synthesised input.
    let typed = target.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        insert::replace_typed(count, &typed)
    })
    .await
    .unwrap_or_else(|err| {
        eprintln!("piplo: undo task failed: {err}");
        insert::Insert::Blocked("the typing step failed")
    });

    if let insert::Insert::Blocked(why) = outcome {
        return Err(why.to_string());
    }

    // Flipped only after the keystrokes landed, so a blocked undo leaves the state
    // describing what is actually on screen.
    let state = app.state::<LastFix>();
    let mut slot = state.lock();

    if let Some(fix) = slot.as_mut() {
        fix.undone = !fix.undone;
        println!("piplo: word fix {} — {target}", if fix.undone { "undone" } else { "redone" });
    }

    Ok(())
}

pub fn set_status(app: &AppHandle, status: Status) {
    if let Err(err) = app.emit("status", status) {
        eprintln!("piplo: could not emit status: {err}");
    }
}

fn bump(app: &AppHandle) -> u64 {
    app.state::<Generation>().0.fetch_add(1, Ordering::SeqCst) + 1
}

fn stale(app: &AppHandle, generation: u64) -> bool {
    app.state::<Generation>().0.load(Ordering::SeqCst) != generation
}

/// Errors are shown on the pill, so the window has to be pill-width first.
fn fail(app: &AppHandle, message: String) {
    fail_for(app, message, ERROR_LINGER);
}

fn fail_for(app: &AppHandle, message: String, linger: Duration) {
    widen(app);
    set_status(app, Status::Error { message });

    // Never leave the widget stuck showing an error.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(linger);
        go_idle(&app);
    });
}

fn widen(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(widget::LABEL) {
        widget::set_active(&window, true);

        // The pill appears for the session even when the chip is hidden by the
        // "Show floating widget" setting — that setting is not a kill switch.
        if let Err(err) = window.show() {
            eprintln!("piplo: could not show the pill: {err}");
        }
    }
}

/// Back to idle, and back out of sight if the user asked for no chip.
fn go_idle(app: &AppHandle) {
    set_status(app, Status::Idle);

    if !settings::current(app).widget_visible {
        widget::set_visible(app, false);
    }
}

#[tauri::command]
pub fn start_dictation(app: AppHandle) {
    start(&app, Trigger::Pointer);
}

#[tauri::command]
pub fn finish_dictation(app: AppHandle) {
    finish(&app, Trigger::Pointer);
}

#[tauri::command]
pub fn cancel_dictation(app: AppHandle) {
    cancel(&app);
}
