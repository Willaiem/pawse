use super::{ForegroundSnapshot, MonitorRef};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
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
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
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
