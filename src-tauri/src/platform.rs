//! The platform seam.
//!
//! Every function here has an implementation for Windows and for macOS, and a
//! stub for anything else. Nothing above this module branches on the OS — if a
//! `cfg` appears in `session.rs`, something belongs here instead.
//!
//! The two coordinate systems are the thing to keep straight. Windows and Tauri
//! both measure from the **top-left** of the primary display. AppKit measures
//! from the **bottom-left**. Every value crossing this boundary is top-left, so
//! the flip happens in exactly one place: `flip`.

use tauri::WebviewWindow;

#[cfg(windows)]
use tauri::Window;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LBUTTON, VK_LWIN, VK_MENU,
        VK_RBUTTON, VK_RWIN, VK_SHIFT,
    },
    UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    },
};

/// A screen rectangle in physical pixels, top-left origin.
///
/// Deliberately not `RECT`. This type is named in the signatures of
/// `widget::clamp` and `menu::place`, which are cross-platform code — a Windows
/// type there is what made the tree fail to compile for macOS at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Keeps a window out of the application switcher.
///
/// Windows: `skipTaskbar` in the config calls `ITaskbarList::DeleteTab`, which
/// removes the taskbar button but leaves the window in the Alt-Tab list. Only
/// `WS_EX_TOOLWINDOW` takes it out of both.
///
/// macOS: nothing to do. Cmd-Tab lists applications, not windows, so a floating
/// utility window never appears in it on its own.
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
/// dictation would be typed into Piplo. `focus: false` in the config is not
/// enough — it governs the initial show, not later clicks.
///
/// Windows: `WS_EX_NOACTIVATE` delivers clicks without activating.
///
/// macOS: **not yet implemented.** There is no `NSWindow` flag for this; the
/// window has to be converted to an `NSPanel` carrying
/// `NSWindowStyleMaskNonactivatingPanel`, which needs `tauri-nspanel`. Until
/// that lands, clicking the widget on macOS activates Piplo and a dictation
/// started with the mouse would be typed into Piplo itself. The shortcut path
/// is unaffected, because it never touches the window.
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

/// The usable area of the display the window is on: no taskbar on Windows, no
/// Dock or menu bar on macOS.
///
/// The full monitor rectangle would put the chip behind the taskbar on a default
/// Windows setup, and behind the Dock on a default macOS one. Tauri's own
/// `current_monitor()` reports exactly that full rectangle, which is why this
/// exists.
#[cfg(windows)]
pub fn work_area(window: &WebviewWindow) -> Option<Rect> {
    let hwnd = match window.hwnd() {
        Ok(hwnd) => HWND(hwnd.0),
        Err(err) => {
            eprintln!("piplo: no HWND for {}: {err}", window.label());
            return None;
        }
    };

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // The monitor the window is on, not the primary one.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };

    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        eprintln!("piplo: GetMonitorInfoW failed");
        return None;
    }

    let work = info.rcWork;

    Some(Rect {
        left: work.left,
        top: work.top,
        right: work.right,
        bottom: work.bottom,
    })
}

#[cfg(target_os = "macos")]
pub fn work_area(window: &WebviewWindow) -> Option<Rect> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    // NSScreen is main-thread only. Every caller — `setup`, the `Moved` handler,
    // the drag commands — is already on the main thread; anything else gets
    // `None` rather than undefined behaviour.
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);

    if screens.count() == 0 {
        eprintln!("piplo: no screens");
        return None;
    }

    // Screen 0 is the one AppKit measures every other screen from, whether or
    // not it is the screen the widget happens to be on.
    let primary_height = screens.objectAtIndex(0).frame().size.height;

    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;

    // Compared in points, not physical pixels: AppKit's global coordinate space
    // is uniform in points, while physical pixels are not on a desktop mixing a
    // Retina display with an external monitor.
    let centre_x = (position.x as f64 + size.width as f64 / 2.0) / scale;
    let centre_y = (position.y as f64 + size.height as f64 / 2.0) / scale;

    let mut found = None;

    for index in 0..screens.count() {
        let screen = screens.objectAtIndex(index);
        let frame = screen.frame();

        let bounds = flip(
            primary_height,
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        );

        if contains(bounds, centre_x, centre_y) {
            found = Some(screen);
            break;
        }
    }

    // On no screen at all — mid-drag between monitors, or a display was just
    // unplugged. The primary is the safe answer: the caller clamps against it,
    // which puts the widget somewhere the mouse can reach.
    let screen = found.unwrap_or_else(|| screens.objectAtIndex(0));
    let visible = screen.visibleFrame();

    Some(scaled(
        flip(
            primary_height,
            visible.origin.x,
            visible.origin.y,
            visible.size.width,
            visible.size.height,
        ),
        scale,
    ))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn work_area(_window: &WebviewWindow) -> Option<Rect> {
    None
}

/// Bottom-left origin (AppKit) to top-left origin (Tauri, Windows), in points.
///
/// Compiled everywhere and tested everywhere on purpose. This is the single
/// easiest thing in the macOS port to get backwards, it compiles perfectly
/// either way, and the symptom is a widget somewhere off screen — so it is
/// worth being provable on the Windows runner too, not only on a Mac.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn flip(primary_height: f64, x: f64, y: f64, width: f64, height: f64) -> [f64; 4] {
    let top = primary_height - (y + height);
    [x, top, x + width, top + height]
}

/// Half-open on the right and bottom, so two adjacent screens cannot both claim
/// a point on their shared edge.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn contains(bounds: [f64; 4], x: f64, y: f64) -> bool {
    x >= bounds[0] && x < bounds[2] && y >= bounds[1] && y < bounds[3]
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn scaled(bounds: [f64; 4], scale: f64) -> Rect {
    Rect {
        left: (bounds[0] * scale).round() as i32,
        top: (bounds[1] * scale).round() as i32,
        right: (bounds[2] * scale).round() as i32,
        bottom: (bounds[3] * scale).round() as i32,
    }
}

/// The cursor, in physical screen pixels, top-left origin.
///
/// `None` means the position could not be read, and callers must treat that as
/// "do not act" rather than as a position.
#[cfg(windows)]
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();

    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }

    Some((point.x, point.y))
}

#[cfg(target_os = "macos")]
pub fn cursor_position() -> Option<(i32, i32)> {
    use objc2_core_graphics::CGEvent;

    // A null event carries the current cursor position, and `location` reports
    // it already flipped to top-left origin — so unlike `work_area` there is no
    // conversion here, and unlike `NSEvent::mouseLocation` it does not need the
    // main thread. That matters: the only caller is the menu's dismissal poll,
    // which runs on its own thread.
    let event = CGEvent::new(None)?;
    // `&*` rather than `&`: the value is a `CFRetained<CGEvent>` and the argument
    // is an `Option<&CGEvent>`, so the deref is written out instead of leaning on
    // coercion inside `Some`.
    let point = CGEvent::location(Some(&*event));

    Some((point.x.round() as i32, point.y.round() as i32))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn cursor_position() -> Option<(i32, i32)> {
    None
}

/// Either mouse button physically down.
#[cfg(windows)]
pub fn mouse_down() -> bool {
    key_down(VK_LBUTTON) || key_down(VK_RBUTTON)
}

#[cfg(target_os = "macos")]
pub fn mouse_down() -> bool {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGMouseButton};

    let state = CGEventSourceStateID::CombinedSessionState;

    CGEventSource::button_state(state, CGMouseButton::Left)
        || CGEventSource::button_state(state, CGMouseButton::Right)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn mouse_down() -> bool {
    false
}

/// Escape physically down.
#[cfg(windows)]
pub fn escape_down() -> bool {
    key_down(VK_ESCAPE)
}

#[cfg(target_os = "macos")]
pub fn escape_down() -> bool {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID};

    /// Virtual keycode for Escape. Apple's keycodes are positional and have not
    /// changed since `Events.h`; there is no named constant to use.
    const ESCAPE: u16 = 53;

    CGEventSource::key_state(CGEventSourceStateID::CombinedSessionState, ESCAPE)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn escape_down() -> bool {
    false
}

/// Whether any chord modifier is still physically held.
///
/// `insert.rs` waits on this before typing. The global-shortcut plugin fires on
/// the first key up, which is almost always the non-modifier key, leaving Ctrl
/// or Cmd down — and typing into a live modifier turns every character into a
/// shortcut firing inside the user's app. That is data loss, not a glitch.
#[cfg(windows)]
pub fn modifiers_held() -> bool {
    const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN];

    MODIFIERS.iter().any(|key| key_down(*key))
}

#[cfg(target_os = "macos")]
pub fn modifiers_held() -> bool {
    use objc2_core_graphics::{CGEventFlags, CGEventSource, CGEventSourceStateID};

    // Caps Lock and Fn are deliberately absent: neither turns a character into a
    // shortcut, and waiting for Caps Lock to be released would hang for anyone
    // who types with it on.
    let chord = CGEventFlags::MaskCommand
        | CGEventFlags::MaskControl
        | CGEventFlags::MaskAlternate
        | CGEventFlags::MaskShift;

    let flags = CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState);

    !(flags & chord).is_empty()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn modifiers_held() -> bool {
    false
}

#[cfg(windows)]
fn key_down(key: VIRTUAL_KEY) -> bool {
    // The high bit is "physically down". The low bit is "pressed since the last
    // call", which would be a false positive here.
    unsafe { (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1440-tall primary screen with a 25pt menu bar and a 78pt Dock: AppKit
    /// reports the visible frame as sitting 78 up from the bottom.
    #[test]
    fn flips_the_primary_screen() {
        let bounds = flip(1440.0, 0.0, 78.0, 2560.0, 1337.0);

        // Top is the menu bar, bottom is where the Dock starts.
        assert_eq!(bounds, [0.0, 25.0, 2560.0, 1362.0]);
    }

    /// The whole screen, no menu bar or Dock subtracted, must round-trip to the
    /// full rectangle — top 0 and bottom equal to the height.
    #[test]
    fn a_full_screen_starts_at_the_top() {
        assert_eq!(flip(1080.0, 0.0, 0.0, 1920.0, 1080.0), [0.0, 0.0, 1920.0, 1080.0]);
    }

    /// A monitor placed *above* the primary is at positive AppKit y and must come
    /// out as negative top — the sign error this test exists to catch.
    #[test]
    fn a_screen_above_the_primary_is_negative() {
        let bounds = flip(1080.0, 0.0, 1080.0, 1920.0, 1080.0);

        assert_eq!(bounds, [0.0, -1080.0, 1920.0, 0.0]);
    }

    /// And one below is at negative AppKit y, landing past the primary's bottom.
    #[test]
    fn a_screen_below_the_primary_is_past_the_bottom() {
        let bounds = flip(1080.0, 0.0, -1080.0, 1920.0, 1080.0);

        assert_eq!(bounds, [0.0, 1080.0, 1920.0, 2160.0]);
    }

    /// A Dock on the left narrows the visible frame from the left edge, which
    /// must not move the top.
    #[test]
    fn a_left_dock_only_moves_the_left_edge() {
        let bounds = flip(1080.0, 80.0, 0.0, 1840.0, 1055.0);

        assert_eq!(bounds, [80.0, 25.0, 1920.0, 1080.0]);
    }

    #[test]
    fn scales_to_physical_pixels() {
        let rect = scaled([0.0, 25.0, 1440.0, 900.0], 2.0);

        assert_eq!(
            rect,
            Rect { left: 0, top: 50, right: 2880, bottom: 1800 }
        );
    }

    #[test]
    fn a_one_times_display_is_unchanged() {
        let rect = scaled([10.0, 20.0, 30.0, 40.0], 1.0);

        assert_eq!(rect, Rect { left: 10, top: 20, right: 30, bottom: 40 });
    }

    #[test]
    fn contains_is_half_open() {
        let bounds = [0.0, 0.0, 100.0, 100.0];

        assert!(contains(bounds, 0.0, 0.0));
        assert!(contains(bounds, 99.9, 99.9));
        // The far edges belong to the next screen along, not this one.
        assert!(!contains(bounds, 100.0, 50.0));
        assert!(!contains(bounds, 50.0, 100.0));
        assert!(!contains(bounds, -0.1, 50.0));
    }

    /// Two side-by-side 1080p screens: a point on the shared edge must resolve to
    /// exactly one of them.
    #[test]
    fn adjacent_screens_do_not_both_claim_the_seam() {
        let left = flip(1080.0, 0.0, 0.0, 1920.0, 1080.0);
        let right = flip(1080.0, 1920.0, 0.0, 1920.0, 1080.0);

        assert!(contains(left, 1919.0, 500.0));
        assert!(!contains(right, 1919.0, 500.0));

        assert!(!contains(left, 1920.0, 500.0));
        assert!(contains(right, 1920.0, 500.0));
    }
}
