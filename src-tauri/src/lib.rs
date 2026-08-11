mod platform;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // GROQ_API_KEY. Absent in a packaged build without a .env, which is fine —
    // the error surfaces on the first dictation, not at startup.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Position only. The default flags include VISIBLE, which would restore
        // `home` and `menu` as shown on the next launch even though both are
        // declared hidden.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .build(),
        )
        .setup(|app| {
            // After the windows exist — the flags need a real HWND.
            for label in ["widget", "menu"] {
                match app.get_webview_window(label) {
                    Some(window) => {
                        platform::hide_from_alt_tab(&window);
                        platform::no_activate(&window);
                    }
                    None => eprintln!("piplo: window '{label}' missing from tauri.conf.json"),
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
