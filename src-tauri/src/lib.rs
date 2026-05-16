mod budget;
mod commands;
mod overlay;
mod sensing;
mod tray;

use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use budget::{AppState, BudgetMachine, BudgetState};
use sensing::SensingState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                tray::show_main_window(app);
            }))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ));
    }

    builder
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
            let autostart_desired = machine.config.autostart;
            app.manage(BudgetState::new(machine));

            #[cfg(desktop)]
            {
                let autostart = app.autolaunch();
                let currently = autostart.is_enabled().unwrap_or(false);
                let result = if autostart_desired && !currently {
                    autostart.enable()
                } else if !autostart_desired && currently {
                    autostart.disable()
                } else {
                    Ok(())
                };
                if let Err(e) = result {
                    eprintln!("[pawse] failed to sync autostart on launch: {e}");
                }
            }

            tray::build(app.handle())?;
            tray::update_tooltip(app.handle());

            let sensing_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut prev_fullscreen: Option<bool> = None;
                loop {
                    let snap = sensing::snapshot();

                    if prev_fullscreen != Some(snap.is_fullscreen) {
                        eprintln!(
                            "[pawse] fullscreen={} (exe={})",
                            snap.is_fullscreen,
                            snap.exe.as_deref().unwrap_or("?")
                        );
                        prev_fullscreen = Some(snap.is_fullscreen);
                    }

                    let sensing_state = sensing_handle.state::<SensingState>();
                    *sensing_state.latest.lock().unwrap() = snap.clone();
                    sensing_state.record_foreground(&snap);
                    drop(sensing_state);

                    let budget_state = sensing_handle.state::<BudgetState>();
                    let transition = budget_state.machine.lock().unwrap().tick(&snap).0;
                    drop(budget_state);

                    if let Some(t) = transition {
                        eprintln!(
                            "[pawse] {} -> {}",
                            t.from.kind_label(),
                            t.to.kind_label()
                        );
                        let _ = sensing_handle.emit("state-changed", &t.to);
                        tray::update_tooltip(&sensing_handle);

                        let entering_break = matches!(t.to, AppState::Break { .. });
                        let leaving_break = matches!(t.from, AppState::Break { .. });

                        if entering_break {
                            let monitor_index = snap.monitor_index.unwrap_or(0);
                            if let Err(e) =
                                overlay::open_cat_window(&sensing_handle, monitor_index).await
                            {
                                eprintln!("[pawse] failed to open cat overlay: {e}");
                            }
                        } else if leaving_break {
                            overlay::close_cat_window(&sensing_handle);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });

            let tooltip_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    tray::update_tooltip(&tooltip_handle);
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::list_monitors,
            commands::show_cat,
            commands::hide_cat,
            commands::current_snapshot,
            commands::recent_foregrounds,
            commands::list_running_processes,
            commands::get_app_state,
            commands::get_config,
            commands::add_tracked_app,
            commands::remove_tracked_app,
            commands::set_usage_minutes,
            commands::set_break_minutes,
            commands::snooze,
            commands::set_autostart,
            commands::close_snooze_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
