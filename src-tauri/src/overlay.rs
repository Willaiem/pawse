use std::time::Duration;

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

pub const CAT_WINDOW_LABEL: &str = "cat";

pub async fn open_cat_window(app: &AppHandle, monitor_index: usize) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(CAT_WINDOW_LABEL) {
        let _ = existing.close();
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .get(monitor_index)
        .or_else(|| monitors.first())
        .ok_or_else(|| "no monitors available".to_string())?;
    let pos = monitor.position();
    let size = monitor.size();

    let window = WebviewWindowBuilder::new(app, CAT_WINDOW_LABEL, WebviewUrl::App("cat.html".into()))
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
    Ok(())
}

pub fn close_cat_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(CAT_WINDOW_LABEL) {
        let _ = w.close();
    }
}
