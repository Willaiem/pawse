use super::{ForegroundSnapshot, RunningProcess};

pub fn snapshot() -> ForegroundSnapshot {
    // M8 will fill this in with NSWorkspace.frontmostApplication,
    // CGEventSourceSecondsSinceLastEventType, and NSWindow style mask checks.
    ForegroundSnapshot::empty()
}

pub fn list_running_processes() -> Vec<RunningProcess> {
    // M8 will fill this in via NSWorkspace.runningApplications.
    Vec::new()
}
