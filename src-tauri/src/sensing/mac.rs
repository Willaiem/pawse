use std::ffi::c_void;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::CGDisplay;

use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

use super::{ForegroundSnapshot, MonitorRef, RunningProcess};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFArrayRef;
}

// kCGEventSourceStateCombinedSessionState
const COMBINED_SESSION_STATE: u32 = 0;
// kCGAnyInputEventType — every input event type.
const ANY_INPUT_EVENT: u32 = u32::MAX;
const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const NULL_WINDOW: u32 = 0;

pub fn snapshot() -> ForegroundSnapshot {
    let idle_for_secs = idle_seconds();
    let (exe, pid) = frontmost_app();
    let (monitor_index, monitor) = main_monitor();
    let is_fullscreen = pid
        .map(|p| pid_covers_screen(p))
        .unwrap_or(false);
    ForegroundSnapshot {
        exe,
        idle_for_secs,
        is_fullscreen,
        monitor_index,
        monitor,
    }
}

fn idle_seconds() -> u64 {
    let s = unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) };
    if s.is_finite() && s >= 0.0 {
        s as u64
    } else {
        0
    }
}

fn frontmost_app() -> (Option<String>, Option<i32>) {
    let ws = NSWorkspace::sharedWorkspace();
    let Some(app) = ws.frontmostApplication() else {
        return (None, None);
    };
    let name = app.localizedName().map(|s| s.to_string());
    (name, Some(app.processIdentifier()))
}

fn main_monitor() -> (Option<usize>, Option<MonitorRef>) {
    let bounds = CGDisplay::main().bounds();
    let m = MonitorRef {
        x: bounds.origin.x as i32,
        y: bounds.origin.y as i32,
        width: bounds.size.width as u32,
        height: bounds.size.height as u32,
    };
    (Some(0), Some(m))
}

fn pid_covers_screen(pid: i32) -> bool {
    let main_bounds = CGDisplay::main().bounds();
    let raw = unsafe {
        CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, NULL_WINDOW)
    };
    if raw.is_null() {
        return false;
    }
    let array: CFArray<*const c_void> = unsafe { CFArray::wrap_under_create_rule(raw) };

    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let bounds_key = CFString::from_static_string("kCGWindowBounds");

    for raw_dict in array.iter() {
        let dict_ref = *raw_dict as *const _;
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(dict_ref) };

        let Some(owner) = read_i64(&dict, &pid_key) else { continue };
        if owner != pid as i64 {
            continue;
        }
        let layer = read_i64(&dict, &layer_key).unwrap_or(99);
        if layer != 0 {
            continue;
        }
        let Some(bounds_raw) = dict.find(cf_key(&bounds_key)) else { continue };
        let bounds_dict: CFDictionary = unsafe {
            CFDictionary::wrap_under_get_rule(*bounds_raw as *const _)
        };
        let x = read_f64_key(&bounds_dict, "X").unwrap_or(f64::NAN);
        let y = read_f64_key(&bounds_dict, "Y").unwrap_or(f64::NAN);
        let w = read_f64_key(&bounds_dict, "Width").unwrap_or(0.0);
        let h = read_f64_key(&bounds_dict, "Height").unwrap_or(0.0);
        if (x - main_bounds.origin.x).abs() < 2.0
            && (y - main_bounds.origin.y).abs() < 2.0
            && (w - main_bounds.size.width).abs() < 2.0
            && (h - main_bounds.size.height).abs() < 2.0
        {
            return true;
        }
    }
    false
}

fn cf_key(s: &CFString) -> *const c_void {
    s.as_concrete_TypeRef() as *const c_void
}

fn read_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    let v = dict.find(cf_key(key))?;
    let num: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*v as *const _) };
    num.to_i64()
}

fn read_f64_key(dict: &CFDictionary, key: &str) -> Option<f64> {
    let cf = CFString::new(key);
    let v = dict.find(cf_key(&cf))?;
    let num: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*v as *const _) };
    // CGWindowBounds values are floats; fall back to i64 just in case.
    num.to_f64().or_else(|| num.to_i64().map(|n| n as f64))
}

pub fn list_running_processes() -> Vec<RunningProcess> {
    let self_pid = std::process::id() as i32;
    let mut out: Vec<RunningProcess> = Vec::new();
    let ws = NSWorkspace::sharedWorkspace();
    let apps = ws.runningApplications();
    for app in apps.iter() {
        if app.processIdentifier() == self_pid {
            continue;
        }
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        let Some(name) = app.localizedName().map(|s| s.to_string()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if out.iter().any(|p| p.exe.eq_ignore_ascii_case(&name)) {
            continue;
        }
        out.push(RunningProcess {
            exe: name,
            title: String::new(),
        });
    }
    out.sort_by(|a, b| a.exe.to_lowercase().cmp(&b.exe.to_lowercase()));
    out
}
