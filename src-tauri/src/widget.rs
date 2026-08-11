//! The widget window: where it sits and how big it is.

use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
};

pub const LABEL: &str = "widget";

/// Gap between the widget and the bottom of the work area, in logical pixels.
const BOTTOM_GAP: f64 = 24.0;

/// The window is only ever these two sizes. The chip is as tall as the pill, so
/// only the width changes and nothing can clip mid-morph.
const IDLE_SIZE: LogicalSize<f64> = LogicalSize::new(56.0, 56.0);
const ACTIVE_SIZE: LogicalSize<f64> = LogicalSize::new(260.0, 56.0);

/// Bottom-centre of the work area. One home, and it stays there.
pub fn position_bottom_centre(window: &WebviewWindow) {
    if let Some(target) = target_position(window) {
        if let Err(err) = window.set_position(target) {
            eprintln!("piplo: could not position widget: {err}");
        }
    }
}

/// Widen for the pill, narrow back for the chip. Re-centring after the resize is
/// what keeps the widget's centre fixed, so it grows outward in both directions
/// instead of appearing to slide.
pub fn set_active(window: &WebviewWindow, active: bool) {
    let size = if active { ACTIVE_SIZE } else { IDLE_SIZE };

    if let Err(err) = window.set_size(size) {
        eprintln!("piplo: could not resize widget: {err}");
        return;
    }

    position_bottom_centre(window);
}

/// Re-centres only when the widget has actually drifted, so calling this from a
/// `Moved` handler settles instead of looping.
pub fn recentre_if_moved(window: &WebviewWindow) {
    let Some(target) = target_position(window) else {
        return;
    };

    match window.outer_position() {
        Ok(current) if current == target => {}
        Ok(_) => position_bottom_centre(window),
        Err(err) => eprintln!("piplo: could not read widget position: {err}"),
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

    let scale = window.scale_factor().unwrap_or(1.0);
    let gap = (BOTTOM_GAP * scale).round() as i32;
    let work = work_area(window)?;

    Some(PhysicalPosition::new(
        work.left + ((work.right - work.left) - size.width as i32) / 2,
        work.bottom - size.height as i32 - gap,
    ))
}

/// The monitor's work area — `rcWork`, which excludes the taskbar. `rcMonitor`
/// would put the chip behind it on a default Windows setup.
#[cfg(windows)]
fn work_area(window: &WebviewWindow) -> Option<RECT> {
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
fn work_area(_window: &WebviewWindow) -> Option<RECT> {
    None
}

/// Called by the frontend once the morph animation has finished, so the window
/// does not narrow while the pill is still shrinking through it.
#[tauri::command]
pub fn widget_set_active(window: WebviewWindow, active: bool) {
    set_active(&window, active);
}
