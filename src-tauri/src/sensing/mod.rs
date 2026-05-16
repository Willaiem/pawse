use serde::Serialize;
use std::sync::{Mutex, OnceLock};

const RECENT_CAP: usize = 5;

fn self_exe_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .unwrap_or_else(|| String::from("pawse.exe"))
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorRef {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningProcess {
    pub exe: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForegroundSnapshot {
    pub exe: Option<String>,
    pub idle_for_secs: u64,
    pub is_fullscreen: bool,
    pub monitor_index: Option<usize>,
    pub monitor: Option<MonitorRef>,
}

impl ForegroundSnapshot {
    pub const fn empty() -> Self {
        Self {
            exe: None,
            idle_for_secs: 0,
            is_fullscreen: false,
            monitor_index: None,
            monitor: None,
        }
    }
}

pub struct SensingState {
    pub latest: Mutex<ForegroundSnapshot>,
    pub recent: Mutex<Vec<String>>,
}

impl Default for SensingState {
    fn default() -> Self {
        Self {
            latest: Mutex::new(ForegroundSnapshot::empty()),
            recent: Mutex::new(Vec::new()),
        }
    }
}

impl SensingState {
    pub fn record_foreground(&self, snap: &ForegroundSnapshot) {
        let Some(exe) = snap.exe.as_deref() else {
            return;
        };
        if exe.eq_ignore_ascii_case(self_exe_name()) {
            return;
        }
        let mut recent = self.recent.lock().unwrap();
        if let Some(pos) = recent.iter().position(|e| e.eq_ignore_ascii_case(exe)) {
            recent.remove(pos);
        }
        recent.insert(0, exe.to_string());
        if recent.len() > RECENT_CAP {
            recent.truncate(RECENT_CAP);
        }
    }
}

#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "windows")]
pub fn snapshot() -> ForegroundSnapshot {
    win::snapshot()
}

#[cfg(target_os = "macos")]
pub fn snapshot() -> ForegroundSnapshot {
    mac::snapshot()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn snapshot() -> ForegroundSnapshot {
    ForegroundSnapshot::empty()
}

#[cfg(target_os = "windows")]
pub fn list_running_processes() -> Vec<RunningProcess> {
    win::list_running_processes()
}

#[cfg(target_os = "macos")]
pub fn list_running_processes() -> Vec<RunningProcess> {
    mac::list_running_processes()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn list_running_processes() -> Vec<RunningProcess> {
    Vec::new()
}
