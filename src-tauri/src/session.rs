//! The only module that knows the order.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, Recorder};
use crate::{grammar, groq, history, insert, settings, snippets, widget};

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

    let result = groq::transcribe(&key, wav).await;

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

    let raw = transcription.text.trim().to_string();

    if raw.is_empty() {
        println!("piplo: nothing transcribed");
        go_idle(&app);
        return;
    }

    println!("piplo: transcript — {raw}");

    // Triggers are short and clean, so this is the check that usually fires. A
    // hit skips grammar entirely, taking a network call out of the most repeated
    // action in the app.
    let expanded = snippets::expand(&app, &raw);

    // Read here rather than cached at startup, so flipping the toggle applies to
    // the very next dictation. Status stays Transcribing across both calls:
    // nothing on screen should reveal that there are two rather than one.
    let cleaned = if expanded.is_none() && settings::current(&app).grammar_enabled {
        grammar::run(&key, &raw).await
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
    let spoken = cleaned.unwrap_or_else(|| raw.clone());

    if corrected {
        println!("piplo: cleaned — {spoken}");
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

    // Only when it differs — an unchanged transcript has nothing to compare.
    let raw_text = (text != raw).then_some(raw);

    // Written before anything is shown on the pill, and with the honest
    // `inserted` value. The history entry is the last line of defence: whatever
    // happens next, the dictation exists somewhere the user can get at it.
    let mut entry = history::Entry::now(text.clone(), raw_text);
    entry.duration_secs = transcription.duration.map(|d| d as f32).unwrap_or(seconds);
    entry.language = transcription.language;
    entry.inserted = inserted;
    entry.corrected = corrected;
    entry.snippet = is_snippet;
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
