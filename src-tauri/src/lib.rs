#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // GROQ_API_KEY. Absent in a packaged build without a .env, which is fine —
    // the error surfaces on the first dictation, not at startup.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
