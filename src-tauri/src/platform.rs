//! Windows-only window flags. Tauri's config cannot express these.

use tauri::WebviewWindow;

#[cfg(windows)]
use tauri::Window;

#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    },
};

/// Keeps a window out of Alt-Tab.
///
/// `skipTaskbar` in the config calls `ITaskbarList::DeleteTab`, which removes
/// the taskbar button but leaves the window in the Alt-Tab list. Only
/// `WS_EX_TOOLWINDOW` takes it out of both.
pub fn hide_from_alt_tab(window: &WebviewWindow) {
    #[cfg(windows)]
    add_ex_style(window.as_ref().window(), WS_EX_TOOLWINDOW.0 as isize);

    #[cfg(not(windows))]
    let _ = window;
}

/// Stops a window from ever taking focus.
///
/// The load-bearing detail of the whole app. Text is typed into whatever window
/// has focus, so if showing the pill or clicking the mic activated Piplo, the
/// dictation would be typed into Piplo. `WS_EX_NOACTIVATE` delivers clicks
/// without activating.
///
/// `focus: false` in the config is not enough — it governs the initial show, not
/// later clicks.
pub fn no_activate(window: &WebviewWindow) {
    #[cfg(windows)]
    add_ex_style(window.as_ref().window(), WS_EX_NOACTIVATE.0 as isize);

    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
fn add_ex_style(window: Window, bits: isize) {
    let hwnd = match window.hwnd() {
        Ok(hwnd) => HWND(hwnd.0),
        // Degrade rather than panic. Logged because a missing NOACTIVATE is the
        // one failure here worth noticing.
        Err(err) => {
            eprintln!("piplo: no HWND for {}: {err}", window.label());
            return;
        }
    };

    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, current | bits);
    }
}
