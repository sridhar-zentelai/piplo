//! The only module that knows the order.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, Recorder};
use crate::{grammar, groq, history, insert, learn, platform, settings, snippets, vocabulary, widget};

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

/// Whether synthesised keystrokes are landing in someone else's window right now.
///
/// It exists for exactly one reader: [`catch_up`]. A UI Automation request makes
/// the target application stop and service it, and doing that while `SendInput`
/// is mid-delivery costs keystrokes — the app drops them, `SendInput` still
/// reports success, and the user gets "I on GentilAI." where Piplo sent "I am
/// working on GentilAI.". Starting a new recording before the last one had
/// finished typing was enough to trigger it.
///
/// Skipping the read is also right on its own terms: text still arriving is text
/// the user has not had a chance to correct.
#[derive(Default)]
pub struct Typing(AtomicBool);

impl Typing {
    fn set(&self, typing: bool) {
        self.0.store(typing, Ordering::SeqCst);
    }

    fn now(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

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
    /// Exactly what Whisper returned.
    heard: String,
    /// What went into grammar — the words as heard, with the terms applied.
    pre_grammar: String,
    /// What was actually typed, after both steps.
    typed: String,
    /// The replacements that fired, so the terms can be put back without
    /// discarding grammar's work.
    fired: Vec<vocabulary::Fired>,
    grammar_undone: bool,
    vocabulary_undone: bool,
}

impl Fix {
    /// What is on screen now.
    fn text(&self) -> String {
        self.text_with(self.grammar_undone, self.vocabulary_undone)
    }

    /// The four combinations, each computed exactly rather than approximated —
    /// which is why both intermediate texts are kept rather than re-derived.
    fn text_with(&self, grammar_undone: bool, vocabulary_undone: bool) -> String {
        match (grammar_undone, vocabulary_undone) {
            (false, false) => self.typed.clone(),
            // Grammar's work survives; only the terms go back.
            (false, true) => vocabulary::revert(&self.typed, &self.fired),
            (true, false) => self.pre_grammar.clone(),
            (true, true) => self.heard.clone(),
        }
    }

    /// Whether there is anything to toggle. A dictation grammar left alone has no
    /// cleanup to undo, and one with no replacement has no fix to undo.
    fn has_grammar(&self) -> bool {
        self.typed != self.pre_grammar
    }

    fn has_vocabulary(&self) -> bool {
        !self.fired.is_empty()
    }
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

/// Exactly what Piplo last typed into someone else's application, and the only
/// thing the read-back at the start of a recording is ever compared against.
///
/// Separate from [`LastFix`] rather than folded into it. `LastFix` exists for the
/// undo shortcut and is only kept when a dictation had something to undo, so a
/// plain one is forgotten — which is precisely the dictation a user is most likely
/// to go and fix by hand. The two answer different questions and one field is
/// cheaper than a second rule inside `remember_fix`.
#[derive(Default)]
pub struct LastTyped(Mutex<Option<String>>);

impl LastTyped {
    fn lock(&self) -> MutexGuard<'_, Option<String>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn set(&self, text: String) {
        *self.lock() = Some(text);
    }

    /// Read **and** clear, so one insertion is compared exactly once. Without
    /// this, every recording would re-read the field and re-learn from the same
    /// stale text until the next dictation replaced it.
    fn take(&self) -> Option<String> {
        self.lock().take()
    }
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

            // Open the Groq connection while the user is still speaking, so the
            // upload that follows does not pay for the handshake. Fire-and-forget
            // — nothing below waits on it.
            crate::http::warm(app);

            widen(app);
            set_status(app, Status::Recording);
            println!("piplo: session start ({trigger:?})");

            // Last, and off the thread: the recorder is already capturing and the
            // pill is already up, so nothing the read does can delay a word the
            // user has started saying.
            catch_up(app);
        }
        Err(err) => {
            drop(slot);

            eprintln!("piplo: could not start capture: {err} — {}", err.detail());
            fail(app, err.to_string());
        }
    }
}

/// Did the user fix the last dictation by hand? Ask once, here, and nowhere else.
///
/// This is the only place in Piplo that looks at another application's text, and
/// the design is the timing: it happens at the start of a recording, it happens
/// once, and it reads the focused field only to compare it against what Piplo
/// itself typed there. Nothing watches, nothing polls, and nothing is retained —
/// the field text lives on this thread and is dropped with it.
///
/// Everything is conditional on there being a `LastTyped` at all, which only a
/// real insertion sets. A first dictation, a snippet, a blocked type: all leave it
/// empty, and this returns without reading anything.
fn catch_up(app: &AppHandle) {
    if !settings::current(app).learn_from_corrections {
        return;
    }

    // Never while keystrokes are still going out. See `Typing` — a UI Automation
    // request lands on the same window and eats them.
    if app.state::<Typing>().now() {
        println!("piplo: still typing, not reading the field this time");
        return;
    }

    let Some(typed) = app.state::<LastTyped>().take() else {
        return;
    };

    let app = app.clone();

    // Blocking, and detached. UI Automation is a cross-process COM call that can
    // take a while or hang against a stuck application — on its own thread that
    // costs a thread, and the dictation in flight never notices.
    tauri::async_runtime::spawn_blocking(move || {
        let Some(field) = platform::focused_text() else {
            return;
        };

        let Some(term) = learn::learn_from_field(&app, &typed, &field) else {
            return;
        };

        // The pill is showing Recording and must go on showing it — this rides
        // alongside as its own event rather than becoming a fifth Status.
        if let Err(err) = app.emit("learned", term) {
            eprintln!("piplo: could not announce the learned word: {err}");
        }
    });
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

    // TEMPORARY: starts the latency clock at the moment recording stops, before
    // the resample, so audio processing is inside the total.
    crate::timing::begin(seconds as f64);

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

    crate::timing::wav_ready();

    // TEMPORARY: set PIPLO_DUMP_WAV to a path to keep a copy of what was sent to
    // Whisper, so the same audio can be replayed against different prompts.
    if let Ok(path) = std::env::var("PIPLO_DUMP_WAV") {
        match std::fs::write(&path, &wav) {
            Ok(()) => println!("piplo: wrote {path}"),
            Err(err) => eprintln!("piplo: could not write {path}: {err}"),
        }
    }

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
        // A snippet hit or the setting being off skips the call entirely, which is
        // a real latency win and has to read as 0 rather than as missing data.
        crate::timing::grammar_skipped();
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
        // `MongoDB` → `Mongo DB` is the common one. An in-memory scan over a
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
    // Raised before the first keystroke and lowered after the last, so a
    // recording started mid-way through cannot read the field out from under it.
    app.state::<Typing>().set(true);
    let outcome = tauri::async_runtime::spawn_blocking(move || insert::type_text(&typed))
        .await
        .unwrap_or_else(|err| {
            eprintln!("piplo: typing task failed: {err}");
            insert::Insert::Blocked("the typing step failed")
        });
    app.state::<Typing>().set(false);

    let inserted = outcome == insert::Insert::Typed;

    // TEMPORARY: closes the latency clock. Placed on the same line as `inserted`
    // so it covers the blocking typing step above and nothing after it.
    crate::timing::inserted(text.split_whitespace().count(), corrected);

    // Only what Piplo typed itself, and only while it is the most recent thing it
    // typed. A snippet expansion is canned text, not a cleanup or a fix, so
    // nothing about it is offered.
    remember_fix(
        &app,
        Fix {
            heard: heard.clone(),
            pre_grammar: raw.clone(),
            typed: text.clone(),
            fired,
            grammar_undone: false,
            vocabulary_undone: false,
        },
        inserted && !is_snippet,
    );

    // The same two conditions, for a different reason: only text that actually
    // reached the user's application can have been corrected in it, and a snippet
    // is canned text whose wording teaches nothing. Set even when the dictation
    // had nothing to undo — that is the case `remember_fix` drops and the one
    // most likely to be fixed by hand.
    if inserted && !is_snippet {
        app.state::<LastTyped>().set(text.clone());
    }

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
fn remember_fix(app: &AppHandle, fix: Fix, keep: bool) {
    let state = app.state::<LastFix>();
    let mut slot = state.lock();

    // Nothing to toggle is the same as nothing to remember, and leaving a stale
    // one there would rub out text it did not write.
    *slot = (keep && (fix.has_grammar() || fix.has_vocabulary())).then_some(fix);
}

/// Which step a shortcut is toggling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Grammar,
    Vocabulary,
}

/// The two flags with one of them flipped. The other is left exactly as it is:
/// undoing the cleanup must not quietly re-apply a word fix the user just undid.
fn flipped(fix: &Fix, step: Step) -> (bool, bool) {
    match step {
        Step::Grammar => (!fix.grammar_undone, fix.vocabulary_undone),
        Step::Vocabulary => (fix.grammar_undone, !fix.vocabulary_undone),
    }
}

/// Rub out what Piplo typed and type it without one of the two steps — the words
/// as spoken instead of the cleaned-up version, or the mishearing instead of the
/// term. Pressing again puts it back, so each chord is an undo and a redo.
///
/// The two are independent: undoing the cleanup and undoing a word fix can both be
/// in force, and the text is computed for whichever combination is current rather
/// than patched step by step.
///
/// A shortcut is the only way in, and deliberately so: this is something you do
/// while looking at the text, and reaching for the widget means moving the mouse
/// away from it. Fire-and-forget — a hotkey has nobody to report to, so the reason
/// lands in the log.
///
/// It assumes the caret has not moved since the dictation, which is why it only
/// ever applies to the most recent one. Piplo does not read the target application
/// to check; that is the boundary the whole feature is built against.
pub fn toggle_from_shortcut(app: &AppHandle, step: Step) {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(why) = toggle(app, step).await {
            eprintln!("piplo: could not undo the {step:?} step — {why}");
        }
    });
}

async fn toggle(app: AppHandle, step: Step) -> Result<(), String> {
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

        match step {
            Step::Grammar if !fix.has_grammar() => {
                return Err("grammar changed nothing in that dictation".into())
            }
            Step::Vocabulary if !fix.has_vocabulary() => {
                return Err("no word was replaced in that dictation".into())
            }
            _ => {}
        }

        let (grammar, vocab) = flipped(fix, step);

        (fix.text().chars().count(), fix.text_with(grammar, vocab))
    };

    println!("piplo: undo {step:?} — erasing {count} chars, typing {target:?}");

    // Blocking: it waits for modifiers and paces synthesised input.
    let typed = target.clone();
    // The same guard as the dictation path: this erases and retypes in the user's
    // own window, and a read-back landing in the middle would eat keystrokes.
    app.state::<Typing>().set(true);
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        insert::replace_typed(count, &typed)
    })
    .await
    .unwrap_or_else(|err| {
        eprintln!("piplo: undo task failed: {err}");
        insert::Insert::Blocked("the typing step failed")
    });
    app.state::<Typing>().set(false);

    if let insert::Insert::Blocked(why) = outcome {
        return Err(why.to_string());
    }

    // Flipped only after the keystrokes landed, so a blocked undo leaves the state
    // describing what is actually on screen.
    let state = app.state::<LastFix>();
    let mut slot = state.lock();

    if let Some(fix) = slot.as_mut() {
        let (grammar, vocab) = flipped(fix, step);
        fix.grammar_undone = grammar;
        fix.vocabulary_undone = vocab;

        let undone = match step {
            Step::Grammar => grammar,
            Step::Vocabulary => vocab,
        };

        println!(
            "piplo: {step:?} {} — {target}",
            if undone { "undone" } else { "redone" }
        );
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
