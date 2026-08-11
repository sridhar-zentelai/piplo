mod audio;
mod platform;
mod session;
mod shortcut;
mod tray;
mod widget;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // GROQ_API_KEY. Absent in a packaged build without a .env, which is fine —
    // the error surfaces on the first dictation, not at startup.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .manage(shortcut::KeyDown::default())
        .manage(session::Active::default())
        .plugin(shortcut::plugin())
        // Position only, and only for `home`. The default flags include VISIBLE,
        // which would restore `home` and `menu` as shown even though both are
        // declared hidden; and the widget computes its own place, so a restored
        // position would only fight it.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .with_denylist(&["widget", "menu"])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![widget::widget_set_active])
        .setup(|app| {
            // After the windows exist — the flags need a real HWND.
            for label in [widget::LABEL, "menu"] {
                match app.get_webview_window(label) {
                    Some(window) => {
                        platform::hide_from_alt_tab(&window);
                        platform::no_activate(&window);
                    }
                    None => eprintln!("piplo: window '{label}' missing from tauri.conf.json"),
                }
            }

            if let Some(window) = app.get_webview_window(widget::LABEL) {
                widget::position_bottom_centre(&window);
            }

            shortcut::register(app.handle());
            tray::create(app.handle())?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing home hides it. Piplo lives in the tray.
            WindowEvent::CloseRequested { api, .. } if window.label() == "home" => {
                api.prevent_close();
                let _ = window.hide();
            }
            // A resolution change makes Windows move the widget, and it does not
            // reach Tauri as anything but the resulting `Moved`.
            WindowEvent::Moved(_) if window.label() == widget::LABEL => {
                if let Some(widget) = window.get_webview_window(widget::LABEL) {
                    widget::recentre_if_moved(&widget);
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
