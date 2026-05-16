use super::ForegroundSnapshot;

pub fn snapshot() -> ForegroundSnapshot {
    // M8 will fill this in with NSWorkspace.frontmostApplication,
    // CGEventSourceSecondsSinceLastEventType, and NSWindow style mask checks.
    ForegroundSnapshot::empty()
}
