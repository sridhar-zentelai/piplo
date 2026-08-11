//! The state machine. In M1 it only knows chip ⇄ pill; audio, Groq, grammar and
//! history hang off `start`/`stop` in M2.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::widget;

/// The single tagged state the frontend renders from.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    Idle,
    Recording,
    #[allow(dead_code)] // M2 sends this between the two network calls.
    Transcribing,
    #[allow(dead_code)]
    Error { message: String },
}

/// Whether a fake-level thread should keep running. Replaced by real RMS in 2.1.
#[derive(Default)]
pub struct Levels(Arc<AtomicBool>);

pub fn start(app: &AppHandle) {
    println!("piplo: session start");

    if let Some(window) = app.get_webview_window(widget::LABEL) {
        widget::set_active(&window, true);
    }

    set_status(app, Status::Recording);
    spawn_fake_levels(app);
}

pub fn stop(app: &AppHandle) {
    println!("piplo: session stop");

    app.state::<Levels>().0.store(false, Ordering::Relaxed);
    set_status(app, Status::Idle);
}

pub fn set_status(app: &AppHandle, status: Status) {
    if let Err(err) = app.emit("status", status) {
        eprintln!("piplo: could not emit status: {err}");
    }
}

/// A sine wave through the real `level` channel, so 1.7's wiring is genuine even
/// though the number isn't. 2.1 swaps the source for cpal's RMS.
fn spawn_fake_levels(app: &AppHandle) {
    let running = app.state::<Levels>().0.clone();
    running.store(true, Ordering::Relaxed);

    let app = app.clone();
    std::thread::spawn(move || {
        let mut frame: u32 = 0;

        while running.load(Ordering::Relaxed) {
            let t = frame as f32 / 30.0;
            let level = (0.30
                + 0.45 * (t * 6.5).sin().abs()
                + 0.20 * (t * 17.0).sin().abs())
            .clamp(0.0, 1.0);

            if app.emit("level", level).is_err() {
                break;
            }

            frame += 1;
            std::thread::sleep(Duration::from_millis(33));
        }

        // Let the bars fall to the floor rather than freezing mid-height.
        let _ = app.emit("level", 0.0_f32);
    });
}
