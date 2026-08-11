//! The widget window: where it sits, how big it is, and where the user put it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
};

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
};

pub const LABEL: &str = "widget";

/// Its own file next to settings.json rather than a fourth field in it: this is
/// the residue of a mouse gesture, not something the user sets.
const POSITION_FILE: &str = "widget.json";

/// Gap between the widget and the bottom of the work area, in logical pixels.
const BOTTOM_GAP: f64 = 24.0;

/// The window is only ever these two sizes. The chip is as tall as the pill, so
/// only the width changes and nothing can clip mid-morph.
const IDLE_SIZE: LogicalSize<f64> = LogicalSize::new(40.0, 40.0);
const ACTIVE_SIZE: LogicalSize<f64> = LogicalSize::new(200.0, 40.0);

/// Where the user dragged the widget: its centre, in physical screen
/// coordinates.
///
/// The centre rather than the corner, so the pill still grows outward in both
/// directions from wherever it was dropped. Physical rather than logical because
/// it is only ever compared against monitor rectangles, and a round trip through
/// the scale factor of whichever monitor the window happens to start on would
/// drift on a mixed-DPI desktop.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Anchor {
    x: i32,
    y: i32,
}

/// `None` means the widget has never been dragged — which is bottom centre, and
/// which keeps following the work area when the resolution changes.
#[derive(Default)]
pub struct Placement(Mutex<Option<Anchor>>);

impl Placement {
    pub fn new(anchor: Option<Anchor>) -> Self {
        Self(Mutex::new(anchor))
    }

    fn lock(&self) -> MutexGuard<'_, Option<Anchor>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Where the window was when the current drag began, or `None` when no drag is
/// in flight. Moves are offsets from this rather than from the live position, so
/// a coalesced or dropped move event cannot make the widget creep away from the
/// cursor.
#[derive(Default)]
pub struct Drag(Mutex<Option<PhysicalPosition<i32>>>);

impl Drag {
    fn lock(&self) -> MutexGuard<'_, Option<PhysicalPosition<i32>>> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn active(&self) -> bool {
        self.lock().is_some()
    }
}

/// Put the widget where it belongs: where it was dropped, or bottom centre if it
/// has never been dragged.
pub fn place(window: &WebviewWindow) {
    if let Some(target) = target_position(window) {
        if let Err(err) = window.set_position(target) {
            eprintln!("piplo: could not position widget: {err}");
        }
    }
}

/// Widen for the pill, narrow back for the chip. Re-placing after the resize is
/// what keeps the widget's centre fixed, so it grows outward in both directions
/// instead of appearing to slide.
pub fn set_active(window: &WebviewWindow, active: bool) {
    let size = if active { ACTIVE_SIZE } else { IDLE_SIZE };

    if let Err(err) = window.set_size(size) {
        eprintln!("piplo: could not resize widget: {err}");
        return;
    }

    place(window);
}

/// Pulls the widget back inside the work area, and only that — so calling this
/// from a `Moved` handler settles instead of looping, and a widget the user has
/// dragged somewhere deliberate stays there.
pub fn keep_on_screen(window: &WebviewWindow) {
    // A drag is the one time the widget is meant to be exactly where it is,
    // including half off an edge on its way to the next monitor.
    if window.state::<Drag>().active() {
        return;
    }

    let (Ok(current), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        eprintln!("piplo: could not read widget geometry");
        return;
    };

    let Some(work) = work_area(window) else {
        return;
    };

    let target = clamp(current, size, work);

    if target != current {
        if let Err(err) = window.set_position(target) {
            eprintln!("piplo: could not position widget: {err}");
        }
    }
}

fn target_position(window: &WebviewWindow) -> Option<PhysicalPosition<i32>> {
    let size = match window.outer_size() {
        Ok(size) => size,
        Err(err) => {
            eprintln!("piplo: could not read widget size: {err}");
            return None;
        }
    };

    let work = work_area(window)?;
    let saved = *window.state::<Placement>().lock();

    let position = match saved {
        Some(anchor) => PhysicalPosition::new(
            anchor.x - size.width as i32 / 2,
            anchor.y - size.height as i32 / 2,
        ),
        None => {
            let scale = window.scale_factor().unwrap_or(1.0);
            let gap = (BOTTOM_GAP * scale).round() as i32;

            PhysicalPosition::new(
                work.left + ((work.right - work.left) - size.width as i32) / 2,
                work.bottom - size.height as i32 - gap,
            )
        }
    };

    Some(clamp(position, size, work))
}

/// Fully inside the work area. A widget that cannot be reached with the mouse
/// cannot be dragged back.
fn clamp(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    work: RECT,
) -> PhysicalPosition<i32> {
    let max_x = (work.right - size.width as i32).max(work.left);
    let max_y = (work.bottom - size.height as i32).max(work.top);

    PhysicalPosition::new(
        position.x.clamp(work.left, max_x),
        position.y.clamp(work.top, max_y),
    )
}

/// The monitor's work area — `rcWork`, which excludes the taskbar. `rcMonitor`
/// would put the chip behind it on a default Windows setup.
#[cfg(windows)]
pub fn work_area(window: &WebviewWindow) -> Option<RECT> {
    let hwnd = match window.hwnd() {
        Ok(hwnd) => HWND(hwnd.0),
        Err(err) => {
            eprintln!("piplo: no HWND for widget: {err}");
            return None;
        }
    };

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // The monitor the widget is on, not the primary one.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info) };

    if ok.as_bool() {
        Some(info.rcWork)
    } else {
        eprintln!("piplo: GetMonitorInfoW failed");
        None
    }
}

#[cfg(not(windows))]
pub fn work_area(_window: &WebviewWindow) -> Option<RECT> {
    None
}

/// Read once at startup. A missing or unreadable file just means "never
/// dragged", which is a home the widget can always compute for itself.
pub fn load_anchor(app: &AppHandle) -> Option<Anchor> {
    let path = position_path(app)?;
    let text = std::fs::read_to_string(&path).ok()?;

    match serde_json::from_str(&text) {
        Ok(anchor) => Some(anchor),
        Err(err) => {
            eprintln!(
                "piplo: {} is not readable ({err}); using the default position",
                path.display()
            );
            None
        }
    }
}

fn save_anchor(app: &AppHandle, anchor: Anchor) {
    let Some(path) = position_path(app) else {
        return;
    };

    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("piplo: could not create the config directory: {err}");
            return;
        }
    }

    match serde_json::to_string(&anchor) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                eprintln!("piplo: could not save the widget position: {err}");
            }
        }
        Err(err) => eprintln!("piplo: could not serialise the widget position: {err}"),
    }
}

fn position_path(app: &AppHandle) -> Option<PathBuf> {
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join(POSITION_FILE)),
        Err(err) => {
            eprintln!("piplo: no config directory: {err}");
            None
        }
    }
}

/// Called by the frontend once the morph animation has finished, so the window
/// does not narrow while the pill is still shrinking through it.
#[tauri::command]
pub fn widget_set_active(window: WebviewWindow, active: bool) {
    set_active(&window, active);
}

/// The drag starts from wherever the window already is; every later move is an
/// offset from that, so the widget tracks the cursor exactly.
#[tauri::command]
pub fn widget_drag_start(window: WebviewWindow, drag: State<'_, Drag>) {
    match window.outer_position() {
        Ok(origin) => *drag.lock() = Some(origin),
        Err(err) => eprintln!("piplo: could not read widget position: {err}"),
    }
}

/// `dx`/`dy` are logical pixels moved since the drag began.
///
/// Deliberately not clamped: clamping each move against the current monitor
/// would trap the widget on it, because Windows only hands a window to the next
/// monitor once it is already mostly there. The drop clamps instead.
#[tauri::command]
pub fn widget_drag_to(window: WebviewWindow, drag: State<'_, Drag>, dx: f64, dy: f64) {
    let Some(origin) = *drag.lock() else {
        return;
    };

    let scale = window.scale_factor().unwrap_or(1.0);

    let target = PhysicalPosition::new(
        origin.x + (dx * scale).round() as i32,
        origin.y + (dy * scale).round() as i32,
    );

    if let Err(err) = window.set_position(target) {
        eprintln!("piplo: could not move widget: {err}");
    }
}

/// The drop is where a position becomes the widget's home: pulled back on
/// screen, then written down as a centre.
#[tauri::command]
pub fn widget_drag_end(window: WebviewWindow) {
    // Before `keep_on_screen`, which is a no-op while a drag is in flight.
    *window.state::<Drag>().lock() = None;
    keep_on_screen(&window);

    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        eprintln!("piplo: could not read widget geometry");
        return;
    };

    let anchor = Anchor {
        x: position.x + size.width as i32 / 2,
        y: position.y + size.height as i32 / 2,
    };

    let app = window.app_handle();
    *app.state::<Placement>().lock() = Some(anchor);
    save_anchor(app, anchor);
}

/// The "Show floating widget" setting.
///
/// Turning it off must not turn dictation off — it is a "get out of my screen"
/// control, not a kill switch. The pill still appears for the duration of a
/// session and goes away again afterwards.
pub fn set_visible(app: &tauri::AppHandle, visible: bool) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };

    let result = if visible {
        place(&window);
        window.show()
    } else {
        window.hide()
    };

    if let Err(err) = result {
        eprintln!("piplo: could not set widget visibility: {err}");
    }
}
