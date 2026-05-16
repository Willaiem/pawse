use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::budget::{AppState, BudgetState, Config};
use crate::overlay;
use crate::sensing::{self, ForegroundSnapshot, RunningProcess, SensingState};
use crate::tray;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Serialize)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub is_primary: bool,
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let primary_name = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .and_then(|m| m.name().cloned());

    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let pos = m.position();
            let size = m.size();
            let name = m
                .name()
                .cloned()
                .unwrap_or_else(|| format!("Monitor {}", i + 1));
            let is_primary = primary_name.as_ref() == Some(&name);
            MonitorInfo {
                index: i,
                name,
                width: size.width,
                height: size.height,
                x: pos.x,
                y: pos.y,
                is_primary,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn show_cat(app: AppHandle, monitor_index: usize) -> Result<(), String> {
    overlay::open_cat_window(&app, monitor_index).await
}

#[tauri::command]
pub fn hide_cat(app: AppHandle) -> Result<(), String> {
    overlay::close_cat_window(&app);
    Ok(())
}

#[tauri::command]
pub fn current_snapshot(state: State<'_, SensingState>) -> ForegroundSnapshot {
    state.latest.lock().unwrap().clone()
}

#[tauri::command]
pub fn recent_foregrounds(state: State<'_, SensingState>) -> Vec<String> {
    state.recent.lock().unwrap().clone()
}

#[tauri::command]
pub async fn list_running_processes() -> Vec<RunningProcess> {
    tauri::async_runtime::spawn_blocking(sensing::list_running_processes)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_app_state(state: State<'_, BudgetState>) -> AppState {
    state.machine.lock().unwrap().state.clone()
}

#[tauri::command]
pub fn get_config(state: State<'_, BudgetState>) -> Config {
    state.machine.lock().unwrap().config.clone()
}

#[tauri::command]
pub fn add_tracked_app(
    app: AppHandle,
    state: State<'_, BudgetState>,
    exe: String,
) -> Result<Config, String> {
    let trimmed = exe.trim().to_string();
    if trimmed.is_empty() {
        return Err("exe cannot be empty".into());
    }
    let mut m = state.machine.lock().unwrap();
    if !m
        .config
        .tracked_apps
        .iter()
        .any(|t| t.eq_ignore_ascii_case(&trimmed))
    {
        m.config.tracked_apps.push(trimmed);
        m.save_config().map_err(|e| e.to_string())?;
    }
    let cfg = m.config.clone();
    drop(m);
    let _ = app.emit("config-changed", &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn remove_tracked_app(
    app: AppHandle,
    state: State<'_, BudgetState>,
    exe: String,
) -> Result<Config, String> {
    let mut m = state.machine.lock().unwrap();
    let before = m.config.tracked_apps.len();
    m.config
        .tracked_apps
        .retain(|t| !t.eq_ignore_ascii_case(&exe));
    if m.config.tracked_apps.len() != before {
        m.save_config().map_err(|e| e.to_string())?;
    }
    let cfg = m.config.clone();
    drop(m);
    let _ = app.emit("config-changed", &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn set_usage_minutes(
    app: AppHandle,
    state: State<'_, BudgetState>,
    minutes: u32,
) -> Result<Config, String> {
    if !(1..=600).contains(&minutes) {
        return Err("usage_minutes must be 1..=600".into());
    }
    let mut m = state.machine.lock().unwrap();
    m.config.usage_minutes = minutes;
    m.save_config().map_err(|e| e.to_string())?;
    let cfg = m.config.clone();
    drop(m);
    let _ = app.emit("config-changed", &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn set_break_minutes(
    app: AppHandle,
    state: State<'_, BudgetState>,
    minutes: u32,
) -> Result<Config, String> {
    if !(1..=60).contains(&minutes) {
        return Err("break_minutes must be 1..=60".into());
    }
    let mut m = state.machine.lock().unwrap();
    m.config.break_minutes = minutes;
    m.save_config().map_err(|e| e.to_string())?;
    let cfg = m.config.clone();
    drop(m);
    let _ = app.emit("config-changed", &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn set_autostart(
    app: AppHandle,
    state: State<'_, BudgetState>,
    enabled: bool,
) -> Result<Config, String> {
    let manager = app.autolaunch();
    let currently = manager.is_enabled().unwrap_or(false);
    if enabled && !currently {
        manager.enable().map_err(|e| e.to_string())?;
    } else if !enabled && currently {
        manager.disable().map_err(|e| e.to_string())?;
    }
    let mut m = state.machine.lock().unwrap();
    m.config.autostart = enabled;
    m.save_config().map_err(|e| e.to_string())?;
    let cfg = m.config.clone();
    drop(m);
    let _ = app.emit("config-changed", &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn close_snooze_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(tray::SNOOZE_WINDOW_LABEL) {
        w.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn snooze(
    app: AppHandle,
    state: State<'_, BudgetState>,
    seconds: u64,
) -> Result<AppState, String> {
    if seconds == 0 {
        return Err("seconds must be > 0".into());
    }
    let transition = {
        let mut m = state.machine.lock().unwrap();
        m.snooze_for(seconds)
    };
    eprintln!(
        "[pawse] {} -> {}",
        transition.from.kind_label(),
        transition.to.kind_label()
    );
    let _ = app.emit("state-changed", &transition.to);
    tray::update_tooltip(&app);
    Ok(transition.to)
}
