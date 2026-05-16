use std::time::Duration;

use tauri::{
    webview::Color, AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindowBuilder,
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
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .transparent(true)
        .background_color(Color(0, 0, 0, 0))
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

    #[cfg(target_os = "macos")]
    force_transparent_webview(&window);

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn force_transparent_webview(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let _ = window.with_webview(|wv| unsafe {
        let view = wv.inner() as *mut AnyObject;
        if view.is_null() {
            return;
        }
        let _: () = msg_send![view, setOpaque: false];

        // KVC into WKWebView private `drawsBackground` flag — well-known idiom.
        let no_number = objc2_foundation::NSNumber::new_bool(false);
        let key = objc2_foundation::NSString::from_str("drawsBackground");
        let _: () = msg_send![view, setValue: &*no_number, forKey: &*key];
    });
}

pub fn close_cat_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(CAT_WINDOW_LABEL) {
        let _ = w.close();
    }
}
