mod commands;
mod sensing;

use std::time::Duration;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(sensing::SensingState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let snap = sensing::snapshot();
                    let state = handle.state::<sensing::SensingState>();
                    if let Ok(mut latest) = state.latest.lock() {
                        *latest = snap;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::list_monitors,
            commands::show_cat,
            commands::hide_cat,
            commands::current_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
