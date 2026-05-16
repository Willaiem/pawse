mod budget;
mod commands;
mod overlay;
mod sensing;

use std::time::Duration;
use tauri::{Emitter, Manager};

use budget::{AppState, BudgetMachine, BudgetState};
use sensing::SensingState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SensingState::default())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app_data_dir is required");
            std::fs::create_dir_all(&data_dir).expect("create app_data_dir");
            let machine = BudgetMachine::load(
                data_dir.join("config.json"),
                data_dir.join("state.json"),
            );
            eprintln!(
                "[pawse] loaded state: {} (data dir: {})",
                machine.state.kind_label(),
                data_dir.display()
            );
            app.manage(BudgetState::new(machine));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let snap = sensing::snapshot();

                    let sensing_state = handle.state::<SensingState>();
                    *sensing_state.latest.lock().unwrap() = snap.clone();
                    sensing_state.record_foreground(&snap);
                    drop(sensing_state);

                    let budget_state = handle.state::<BudgetState>();
                    let transition = budget_state.machine.lock().unwrap().tick(&snap).0;
                    drop(budget_state);

                    if let Some(t) = transition {
                        eprintln!(
                            "[pawse] {} -> {}",
                            t.from.kind_label(),
                            t.to.kind_label()
                        );
                        let _ = handle.emit("state-changed", &t.to);

                        let entering_break = matches!(t.to, AppState::Break { .. });
                        let leaving_break = matches!(t.from, AppState::Break { .. });

                        if entering_break {
                            let monitor_index = snap.monitor_index.unwrap_or(0);
                            if let Err(e) =
                                overlay::open_cat_window(&handle, monitor_index).await
                            {
                                eprintln!("[pawse] failed to open cat overlay: {e}");
                            }
                        } else if leaving_break {
                            overlay::close_cat_window(&handle);
                        }
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
            commands::recent_foregrounds,
            commands::get_app_state,
            commands::get_config,
            commands::add_tracked_app,
            commands::remove_tracked_app,
            commands::set_usage_minutes,
            commands::set_break_minutes,
            commands::snooze,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
