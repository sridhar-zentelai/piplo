//! Tray icon: open and quit.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Piplo", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let mut builder = TrayIconBuilder::with_id("piplo")
        .tooltip("Piplo")
        .menu(&menu)
        // Left click opens the home window, so it must not also open the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_home(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_home(tray.app_handle());
            }
        });

    match app.default_window_icon().cloned() {
        Some(icon) => builder = builder.icon(icon),
        // A tray entry with no glyph is still clickable, which beats no tray.
        None => eprintln!("piplo: no default window icon for the tray"),
    }

    builder.build(app)?;
    Ok(())
}

pub fn show_home(app: &AppHandle) {
    let Some(window) = app.get_webview_window("home") else {
        eprintln!("piplo: window 'home' missing from tauri.conf.json");
        return;
    };

    if let Err(err) = window.show() {
        eprintln!("piplo: could not show home: {err}");
        return;
    }

    // Unlike the widget, home is a normal window and is meant to take focus.
    let _ = window.unminimize();
    let _ = window.set_focus();
}
