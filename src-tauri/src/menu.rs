//! The widget's right-click menu.
//!
//! A window rather than a native context menu, so it matches the widget's look
//! and can host a toggle switch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::{platform, session, widget};

pub const LABEL: &str = "menu";

/// Gap between the widget and the menu, in logical pixels.
const GAP: f64 = 10.0;

/// The four permanent items, matching `tauri.conf.json`.
const WIDTH: f64 = 236.0;
const HEIGHT: f64 = 172.0;

/// One more row, for the word-fix undo when there is one to offer. The window has
/// to be told: it is frameless and fixed, so the item would otherwise be clipped.
const ROW: f64 = 36.0;

/// Frequent enough to feel instant, rare enough to cost nothing.
const POLL: Duration = Duration::from_millis(40);

#[derive(Default)]
pub struct MenuOpen(AtomicBool);

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        eprintln!("piplo: window '{LABEL}' missing from tauri.conf.json");
        return;
    };

    // Sized before it is placed: the position is measured from the height.
    let height = if session::has_word_fix(app) {
        HEIGHT + ROW
    } else {
        HEIGHT
    };

    if let Err(err) = window.set_size(LogicalSize::new(WIDTH, height)) {
        eprintln!("piplo: could not size the menu: {err}");
    }

    place(app, &window);

    if let Err(err) = window.show() {
        eprintln!("piplo: could not show the menu: {err}");
        return;
    }

    // Read through rather than cached — the settings page is a view of the same
    // file, and a stale switch position is worse than a brief flicker.
    let _ = app.emit_to(LABEL, "menu-open", ());

    let already = app.state::<MenuOpen>().0.swap(true, Ordering::SeqCst);

    if !already {
        watch_for_dismissal(app.clone());
    }
}

pub fn hide(app: &AppHandle) {
    app.state::<MenuOpen>().0.store(false, Ordering::SeqCst);

    if let Some(window) = app.get_webview_window(LABEL) {
        if let Err(err) = window.hide() {
            eprintln!("piplo: could not hide the menu: {err}");
        }
    }
}

/// Adjacent to the widget, then clamped into the work area so it never opens
/// off-screen when the widget is near an edge.
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Some(widget_window) = app.get_webview_window(widget::LABEL) else {
        return;
    };

    let (Ok(anchor), Ok(anchor_size), Ok(size), Ok(scale)) = (
        widget_window.outer_position(),
        widget_window.outer_size(),
        window.outer_size(),
        window.scale_factor(),
    ) else {
        return;
    };

    let Some(work) = platform::work_area(&widget_window) else {
        return;
    };

    let gap = (GAP * scale).round() as i32;
    let width = size.width as i32;
    let height = size.height as i32;

    // The widget lives at the bottom of the screen, so the menu opens upward.
    // Below it would be off-screen or behind the taskbar.
    let mut y = anchor.y - height - gap;

    if y < work.top {
        y = anchor.y + anchor_size.height as i32 + gap;
    }

    let centred = anchor.x + (anchor_size.width as i32 - width) / 2;
    let x = centred.clamp(work.left, (work.right - width).max(work.left));

    let y = y.clamp(work.top, (work.bottom - height).max(work.top));

    if let Err(err) = window.set_position(PhysicalPosition::new(x, y)) {
        eprintln!("piplo: could not position the menu: {err}");
    }
}

/// The menu never receives focus — `WS_EX_NOACTIVATE` on Windows, a
/// non-activating panel on macOS — so there is no blur event and no keyboard
/// input. `Escape` and outside clicks both have to be polled.
///
/// Deliberately not solved with a full-screen transparent overlay to catch the
/// click: that swallows the first click intended for the app underneath, which is
/// the click the user actually wanted.
///
/// Runs on its own thread, which is why every `platform` call it makes is one of
/// the thread-safe ones.
fn watch_for_dismissal(app: AppHandle) {
    std::thread::spawn(move || {
        let open = || app.state::<MenuOpen>().0.load(Ordering::SeqCst);

        // Wait out the click that opened the menu, or it would dismiss itself.
        while platform::mouse_down() && open() {
            std::thread::sleep(POLL);
        }

        while open() {
            if platform::escape_down() {
                break;
            }

            if platform::mouse_down() && !cursor_over_menu(&app) {
                break;
            }

            std::thread::sleep(POLL);
        }

        if open() {
            hide(&app);
        }
    });
}

fn cursor_over_menu(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(LABEL) else {
        return false;
    };

    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return false;
    };

    // An unknown cursor counts as inside, so a failed read cannot dismiss the
    // menu out from under the user.
    let Some((x, y)) = platform::cursor_position() else {
        return true;
    };

    x >= position.x
        && x < position.x + size.width as i32
        && y >= position.y
        && y < position.y + size.height as i32
}

#[tauri::command]
pub fn show_widget_menu(app: AppHandle) {
    show(&app);
}

#[tauri::command]
pub fn hide_widget_menu(app: AppHandle) {
    hide(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
