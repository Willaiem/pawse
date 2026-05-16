mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::list_monitors,
            commands::show_cat,
            commands::hide_cat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
