//! The state machine. In M2 it grows the rest of the order — encode, Groq,
//! insert, history — around `start` and `stop`.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::Recorder;
use crate::widget;

/// How long an error sits on the pill before it returns to idle.
const ERROR_LINGER: Duration = Duration::from_millis(2500);

/// The single tagged state the frontend renders from.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    Idle,
    Recording,
    #[allow(dead_code)] // 2.3 sends this while Groq is working.
    Transcribing,
    Error {
        message: String,
    },
}

/// The capture in flight, if any. One at a time is enforced here rather than
/// debounced in the UI, because the shortcut, the mic button and the menu are
/// three entry points and only one place should decide.
#[derive(Default)]
pub struct Active(Mutex<Option<Recorder>>);

impl Active {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Recorder>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

pub fn start(app: &AppHandle) {
    let state = app.state::<Active>();
    let mut slot = state.lock();

    if slot.is_some() {
        return; // already recording
    }

    match Recorder::start(app.clone()) {
        Ok(recorder) => {
            *slot = Some(recorder);
            drop(slot);

            widen(app);
            set_status(app, Status::Recording);
            println!("piplo: session start");
        }
        Err(err) => {
            drop(slot);

            eprintln!("piplo: could not start capture: {err} — {}", err.detail());
            fail(app, err.to_string());
        }
    }
}

pub fn stop(app: &AppHandle) {
    let recorder = app.state::<Active>().lock().take();

    let Some(recorder) = recorder else {
        return; // key release with nothing in flight
    };

    let (samples, sample_rate) = recorder.stop();
    let seconds = samples.len() as f32 / sample_rate.max(1) as f32;

    println!(
        "piplo: session stop — {} mono samples at {sample_rate} Hz ({seconds:.2}s)",
        samples.len()
    );

    // Let the bars fall to the floor rather than freezing mid-height.
    let _ = app.emit("level", 0.0_f32);
    set_status(app, Status::Idle);
}

pub fn set_status(app: &AppHandle, status: Status) {
    if let Err(err) = app.emit("status", status) {
        eprintln!("piplo: could not emit status: {err}");
    }
}

/// Errors are shown on the pill, so the window has to be pill-width first.
fn fail(app: &AppHandle, message: String) {
    widen(app);
    set_status(app, Status::Error { message });

    // Never leave the widget stuck showing an error.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(ERROR_LINGER);
        set_status(&app, Status::Idle);
    });
}

fn widen(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(widget::LABEL) {
        widget::set_active(&window, true);
    }
}
