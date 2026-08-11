mod audio;
mod grammar;
mod groq;
mod history;
mod insert;
mod menu;
mod platform;
mod session;
mod settings;
mod shortcut;
mod tray;
mod widget;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    load_env();

    // Read once, here. The key never reaches the webview. A missing key is a
    // message on the pill, not a crash.
    let api_key = match std::env::var("GROQ_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(key),
        _ => {
            eprintln!("piplo: GROQ_API_KEY is not set — dictation will show an error");
            None
        }
    };

    tauri::Builder::default()
        .manage(session::ApiKey(api_key))
        .manage(session::Active::default())
        .manage(session::Generation::default())
        .manage(shortcut::KeyDown::default())
        .manage(menu::MenuOpen::default())
        .plugin(shortcut::plugin())
        .plugin(tauri_plugin_clipboard_manager::init())
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
        .invoke_handler(tauri::generate_handler![
            widget::widget_set_active,
            session::start_dictation,
            session::finish_dictation,
            session::cancel_dictation,
            settings::get_settings,
            settings::set_settings,
            history::get_history,
            history::clear_history,
            tray::open_home,
            menu::show_widget_menu,
            menu::hide_widget_menu,
            menu::quit_app,
        ])
        .setup(|app| {
            let handle = app.handle();

            // Before the shortcut is registered — the saved accelerator decides
            // what gets bound.
            let saved = settings::load(handle);
            let shortcut_accelerator = saved.shortcut.clone();
            let widget_visible = saved.widget_visible;
            app.manage(settings::Store::new(saved));

            // After the windows exist — the flags need a real HWND.
            for label in [widget::LABEL, menu::LABEL] {
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

            if !widget_visible {
                widget::set_visible(handle, false);
            }

            shortcut::register_initial(handle, &shortcut_accelerator);
            tray::create(handle)?;

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

fn load_env() {
    // dotenv() walks up from the working directory, which finds the repo .env in
    // dev but not from an installed binary. Fall back to the path baked at build
    // time before giving up on the process environment.
    if dotenvy::dotenv().is_err() {
        let _ = dotenvy::from_path(history::repo_root().join(".env"));
    }
}
