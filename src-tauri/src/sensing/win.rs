use super::{ForegroundSnapshot, MonitorRef, RunningProcess};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONULL,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE,
    GW_OWNER, WS_EX_TOOLWINDOW,
};

pub fn snapshot() -> ForegroundSnapshot {
    let idle_for_secs = idle_seconds();
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return ForegroundSnapshot {
            idle_for_secs,
            ..ForegroundSnapshot::empty()
        };
    }
    let exe = foreground_exe(hwnd);
    let (monitor_index, monitor) = monitor_for_window(hwnd);
    let is_fullscreen = window_covers_monitor(hwnd, monitor.as_ref());
    ForegroundSnapshot {
        exe,
        idle_for_secs,
        is_fullscreen,
        monitor_index,
        monitor,
    }
}

fn idle_seconds() -> u64 {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info).as_bool() {
            let tick = GetTickCount();
            u64::from(tick.saturating_sub(info.dwTime)) / 1000
        } else {
            0
        }
    }
}

fn foreground_exe(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if res.is_ok() {
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            Some(
                path.rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(&path)
                    .to_string(),
            )
        } else {
            None
        }
    }
}

fn monitor_for_window(hwnd: HWND) -> (Option<usize>, Option<MonitorRef>) {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        if hmon.0.is_null() {
            return (None, None);
        }
        let monitors = enumerate_monitors();
        let index = monitors.iter().position(|m| m.0 == hmon.0);

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mref = if GetMonitorInfoW(hmon, &mut info).as_bool() {
            Some(MonitorRef {
                x: info.rcMonitor.left,
                y: info.rcMonitor.top,
                width: (info.rcMonitor.right - info.rcMonitor.left) as u32,
                height: (info.rcMonitor.bottom - info.rcMonitor.top) as u32,
            })
        } else {
            None
        };
        (index, mref)
    }
}

fn window_covers_monitor(hwnd: HWND, monitor: Option<&MonitorRef>) -> bool {
    let Some(m) = monitor else { return false };
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        rect.left <= m.x
            && rect.top <= m.y
            && (rect.right - rect.left) as u32 >= m.width
            && (rect.bottom - rect.top) as u32 >= m.height
    }
}

unsafe extern "system" fn enum_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(lparam.0 as *mut Vec<HMONITOR>) };
    monitors.push(hmon);
    BOOL(1)
}

pub fn list_running_processes() -> Vec<RunningProcess> {
    let mut entries: Vec<RunningProcess> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_window_proc),
            LPARAM(&mut entries as *mut _ as isize),
        );
    }

    let self_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
        .unwrap_or_default();

    let mut deduped: Vec<RunningProcess> = Vec::new();
    for entry in entries {
        if !self_exe.is_empty() && entry.exe.eq_ignore_ascii_case(&self_exe) {
            continue;
        }
        if let Some(existing) = deduped
            .iter_mut()
            .find(|e| e.exe.eq_ignore_ascii_case(&entry.exe))
        {
            if existing.title.is_empty() && !entry.title.is_empty() {
                existing.title = entry.title;
            }
        } else {
            deduped.push(entry);
        }
    }
    deduped.sort_by(|a, b| a.exe.to_lowercase().cmp(&b.exe.to_lowercase()));
    deduped
}

unsafe extern "system" fn enum_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let entries = &mut *(lparam.0 as *mut Vec<RunningProcess>);
        if !is_user_window(hwnd) {
            return BOOL(1);
        }
        let Some(exe) = foreground_exe(hwnd) else {
            return BOOL(1);
        };
        let title = window_title(hwnd);
        entries.push(RunningProcess { exe, title });
    }
    BOOL(1)
}

unsafe fn is_user_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        // Skip owned windows (tooltips, dialogs, etc.)
        if !GetWindow(hwnd, GW_OWNER).map(|h| h.0.is_null()).unwrap_or(true) {
            return false;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        // Filter cloaked windows (UWP background processes etc.)
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        true
    }
}

unsafe fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..written as usize])
    }
}

fn enumerate_monitors() -> Vec<HMONITOR> {
    let mut monitors: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_proc),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}
