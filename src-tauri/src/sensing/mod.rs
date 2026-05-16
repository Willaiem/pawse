use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct MonitorRef {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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
}

impl Default for SensingState {
    fn default() -> Self {
        Self {
            latest: Mutex::new(ForegroundSnapshot::empty()),
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
