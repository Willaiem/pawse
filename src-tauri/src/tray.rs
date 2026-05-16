use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder, Wry,
};

use crate::budget::{AppState, BudgetState};

pub const TRAY_ID: &str = "pawse-tray";
pub const SNOOZE_WINDOW_LABEL: &str = "snooze-confirm";

const MENU_ID_SNOOZE: &str = "snooze";
const MENU_ID_SETTINGS: &str = "settings";
const MENU_ID_QUIT: &str = "quit";

const SNOOZE_LABEL_OPEN: &str = "Snooze 30 min…";
const SNOOZE_LABEL_CANCEL: &str = "Cancel snooze";

struct TrayMenuState {
    snooze_item: MenuItem<Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let snooze_item =
        MenuItem::with_id(app, MENU_ID_SNOOZE, SNOOZE_LABEL_OPEN, true, None::<&str>)?;
    let settings_item =
        MenuItem::with_id(app, MENU_ID_SETTINGS, "Open Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, MENU_ID_QUIT, "Quit pawse", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&snooze_item, &settings_item, &quit_item])?;
    app.manage(TrayMenuState {
        snooze_item: snooze_item.clone(),
    });

    let icon = app
        .default_window_icon()
        .cloned()
        .unwrap_or_else(|| Image::new(&[], 0, 0));

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("pawse — starting up…")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_ID_SNOOZE => handle_snooze_menu(app),
            MENU_ID_SETTINGS => show_main_window(app),
            MENU_ID_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn open_snooze_window(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window(SNOOZE_WINDOW_LABEL) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }

    let width: u32 = 360;
    let height: u32 = 170;

    let pos = center_on_cursor_monitor(app, width, height);

    let result = WebviewWindowBuilder::new(
        app,
        SNOOZE_WINDOW_LABEL,
        WebviewUrl::App("snooze.html".into()),
    )
    .title("pawse")
    .inner_size(width.into(), height.into())
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .decorations(false)
    .visible(false)
    .build();

    match result {
        Ok(window) => {
            let _ = window.set_position(pos);
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("[pawse] failed to open snooze window: {e}"),
    }
}

fn center_on_cursor_monitor(app: &AppHandle, w: u32, h: u32) -> LogicalPosition<f64> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| {
            app.available_monitors().ok().and_then(|mons| {
                mons.into_iter().find(|m| {
                    let p = m.position();
                    let s = m.size();
                    let x = cursor.x as i32;
                    let y = cursor.y as i32;
                    x >= p.x
                        && x < p.x + s.width as i32
                        && y >= p.y
                        && y < p.y + s.height as i32
                })
            })
        })
        .or_else(|| app.primary_monitor().ok().flatten());

    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let pos = m.position();
        let size = m.size();
        let logical_x = pos.x as f64 / scale;
        let logical_y = pos.y as f64 / scale;
        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let x = logical_x + ((logical_w - w as f64) / 2.0).max(0.0);
        let y = logical_y + ((logical_h - h as f64) / 2.0).max(0.0);
        LogicalPosition::new(x, y)
    } else {
        LogicalPosition::new(100.0, 100.0)
    }
}

pub fn update_tooltip(app: &AppHandle) {
    let Some(budget) = app.try_state::<BudgetState>() else {
        return;
    };
    let state = budget.machine.lock().unwrap().state.clone();
    let tip = tooltip_for(&state);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tip));
    }
    if let Some(tray_state) = app.try_state::<TrayMenuState>() {
        let label = if matches!(state, AppState::Snoozed { .. }) {
            SNOOZE_LABEL_CANCEL
        } else {
            SNOOZE_LABEL_OPEN
        };
        let _ = tray_state.snooze_item.set_text(label);
    }
}

fn handle_snooze_menu(app: &AppHandle) {
    let Some(budget) = app.try_state::<BudgetState>() else {
        open_snooze_window(app);
        return;
    };
    let snoozed = matches!(
        budget.machine.lock().unwrap().state,
        AppState::Snoozed { .. }
    );
    if snoozed {
        let transition = budget.machine.lock().unwrap().cancel_snooze();
        if let Some(t) = transition {
            eprintln!("[pawse] {} -> {}", t.from.kind_label(), t.to.kind_label());
            let _ = tauri::Emitter::emit(app, "state-changed", &t.to);
        }
        update_tooltip(app);
    } else {
        open_snooze_window(app);
    }
}

fn tooltip_for(state: &AppState) -> String {
    match state {
        AppState::Idle { remaining_secs } => {
            format!("pawse — {} remaining", format_minutes(*remaining_secs))
        }
        AppState::Active { remaining_secs } => {
            format!("pawse — {} remaining", format_minutes(*remaining_secs))
        }
        AppState::Break { remaining_secs } => {
            format!("on break — {}", format_minutes(*remaining_secs))
        }
        AppState::DeferredBreak => "break pending — fullscreen".to_string(),
        AppState::Snoozed { until_unix } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let left = until_unix.saturating_sub(now) as u32;
            format!("snoozed — {} left", format_minutes(left))
        }
    }
}

fn format_minutes(secs: u32) -> String {
    if secs >= 3600 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{h} h")
        } else {
            format!("{h} h {m} min")
        }
    } else if secs >= 60 {
        format!("{} min", secs / 60)
    } else {
        format!("{secs} s")
    }
}
