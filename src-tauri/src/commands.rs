use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder,
};

use crate::budget::{AppState, BudgetState, Config};
use crate::sensing::{ForegroundSnapshot, SensingState};

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
    if let Some(existing) = app.get_webview_window("cat") {
        let _ = existing.close();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }

    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .get(monitor_index)
        .ok_or_else(|| format!("monitor {monitor_index} not found"))?;
    let pos = monitor.position();
    let size = monitor.size();

    let window = WebviewWindowBuilder::new(&app, "cat", WebviewUrl::App("cat.html".into()))
        .title("pawse")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .transparent(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    window
        .set_position(PhysicalPosition::new(pos.x, pos.y))
        .map_err(|e| e.to_string())?;
    window
        .set_size(PhysicalSize::new(size.width, size.height))
        .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    // M1: hardcoded 5s auto-close. M4 hands this off to the state machine.
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if let Some(w) = app_handle.get_webview_window("cat") {
            let _ = w.close();
        }
    });

    Ok(())
}

#[tauri::command]
pub fn hide_cat(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("cat") {
        w.close().map_err(|e| e.to_string())?;
    }
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
    Ok(transition.to)
}
