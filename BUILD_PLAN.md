# pawse — v1 build plan

A tracer-bullet plan: get the riskiest end-to-end path working first, then fill in around it. Every milestone ends in something you can run and feel.

## Stack reminder
- **Tauri 2.x** (Rust backend, TypeScript frontend)
- **Frontend**: vanilla TS + a small component (the cat overlay is just `<video>`; settings is a few inputs — no framework needed, but Svelte/Solid are fine if preferred)
- **Rust crates**: `sysinfo` (process listing), `tauri-plugin-autostart`, `tauri-plugin-store` or plain JSON in `app_data_dir`
- **Platform-specific**: small Rust modules behind `#[cfg(target_os = ...)]` for foreground window, idle time, fullscreen detection
- **Assets**: `neko1.webm` (intro), `neko2.webm` (loop) — bundled in `src-tauri/assets/`

---

## M0 — Scaffold and dev loop (½ day)

Goal: `cargo tauri dev` opens a window, hot-reloads TS, can call Rust commands.

- `npm create tauri-app@latest` → vanilla TS, no router
- Confirm dev server runs on Windows
- Set up `src-tauri/src/` with a `commands.rs` module
- Wire one trivial IPC call (`greet`) end-to-end as a smoke test
- Add `.gitignore` (target/, dist/, node_modules/)

Done when: dev loop is < 2s on save, IPC works both ways.

---

## M1 — Cat overlay, hardcoded (1 day) — RISKIEST, BUILD FIRST

Goal: a Rust command `show_cat(monitor_index)` opens a fullscreen, always-on-top, frameless window on the target monitor playing `neko1.webm` → `neko2.webm` loop, muted, auto-closes after 5s (hardcoded for now). Triggered by a global hotkey or a debug button.

Why first: this is where Tauri's edges live. Validate that cross-monitor, frameless, always-on-top fullscreen overlay actually works on Win + Mac before building anything around it.

- Create the cat window programmatically in Rust (`WebviewWindowBuilder`)
  - `decorations: false`, `always_on_top: true`, `skip_taskbar: true`, `transparent: false`
  - Set position + size to match target monitor's bounds (use `tauri::Monitor` API)
  - `fullscreen: true` on the OS that honors it best, else manual size-to-monitor
- Frontend page `cat.html`: single `<video autoplay muted>` element, swaps `src` from intro to loop on `ended` event, full viewport coverage with `object-fit: cover`
- Bundle webms via Tauri's `tauri.conf.json` → `bundle.resources`
- Trigger from main window button for now

Done when: pressing a button opens the cat on the monitor you pick, plays smoothly, can't be alt-tabbed past in normal nudge fashion (visible, on top). Confirms multi-monitor placement works.

Risk to watch: Windows exclusive-fullscreen games may render above always-on-top windows — that's M6's job to handle, not M1's.

---

## M2 — Process / activity sensing (1 day)

Goal: Rust can answer three questions at any time:
1. What is the currently foreground process? (`String` exe name)
2. Is the user idle? (`Duration` since last input)
3. Is the foreground window fullscreen?

- `src-tauri/src/sensing/mod.rs` with platform sub-modules
- `sensing/win.rs`:
  - Foreground: `GetForegroundWindow` + `GetWindowThreadProcessId` + `OpenProcess` + `QueryFullProcessImageNameW`
  - Idle: `GetLastInputInfo`
  - Fullscreen: window rect vs monitor rect comparison + check for exclusive fullscreen flag
- `sensing/mac.rs`:
  - Foreground: `NSWorkspace.shared.frontmostApplication`
  - Idle: `CGEventSourceSecondsSinceLastEventType`
  - Fullscreen: check window's `NSWindowStyleMask` / space membership
- Expose as a single struct `ForegroundSnapshot { exe, idle_for, is_fullscreen, monitor_index }` polled every 1s from a background Tokio task

Done when: a debug Tauri command returns a live snapshot you can verify by switching windows and going idle.

---

## M3 — Budget state machine + persistence (1 day)

Goal: in-memory state machine that consumes M2's snapshots and decides when the cat should fire.

States:
- `Idle` — no tracked app foreground (or user idle)
- `Active(remaining: Duration)` — tracked app foreground, draining
- `Break(remaining: Duration)` — cat showing
- `Snoozed(until: Instant)` — tracking paused
- `DeferredBreak` — would be in Break but foreground is fullscreen

Transitions:
- `Idle → Active` on tracked-app-foreground + recent-input
- `Active → Idle` on foreground change away, or idle > 60s
- `Active → Break` when remaining hits 0 (unless fullscreen → `DeferredBreak`)
- `DeferredBreak → Break` on fullscreen exit
- `Break → Active(default_usage)` when break duration elapses (refill)
- `* → Snoozed`, `Snoozed → Active` on manual snooze + expiry

Persistence:
- JSON file in `app_data_dir/state.json`, flushed every 60s and on state transitions
- On launch: read remaining budget, drop any in-progress break (forfeit rule)
- On launch: if config file missing, write defaults

Done when: a debug log line prints state transitions in real time as you use tracked apps, and the budget survives an app restart.

---

## M4 — Wire M3 to M1 (½ day)

Goal: state machine entering `Break` actually fires the cat overlay on the correct monitor; exiting `Break` closes it.

- Subscribe the cat-window controller to state transitions
- On enter `Break`: read snapshot's `monitor_index`, open cat
- On exit `Break`: close cat window
- Set break to 30s during testing, not 5 min

Done when: configure Discord as a tracked app with a 1-min budget, open it, wait, cat appears on Discord's monitor, sits for 30s, closes. You've shipped the core loop.

---

## M5 — Tray icon + menu (½ day)

Goal: pawse runs in tray, not as a visible main window.

- `tauri-plugin-tray` (or built-in `TrayIconBuilder`) with static cat PNG
- Hover tooltip updates every minute: `"pawse — 42 min remaining"` / `"on break — 3 min"` / `"snoozed for 12 min"`
- Right-click menu:
  - `Snooze 30 min` → opens a small confirm dialog window
  - `Open Settings`
  - `Quit pawse`
- Closing the main window hides it (doesn't quit)
- Single-instance lock so double-launching does nothing

Done when: app installs to tray on launch, no main window appears unless you click Open Settings, tooltip reflects state.

---

## M6 — Settings window (1 day)

Goal: the minimal settings surface from the spec.

- Tracked apps list:
  - Shows current tracked apps with icon + display name + Remove button
  - Add button opens a modal listing currently-running processes (deduped by exe), each with icon + window title, click to add
  - Curated suggestions section (Discord, Steam, Slack) shown above with toggle switches — adding/removing them updates the tracked list
- Usage minutes (number input, 1–600)
- Break minutes (number input, 1–60)
- Autostart toggle (calls `tauri-plugin-autostart`)
- Changes save immediately, no Save button

Done when: you can add/remove tracked apps via the picker, change limits, toggle autostart, and the state machine respects the new config without restart.

---

## M7 — Fullscreen deferral + snooze (½ day)

Goal: the two interaction edges actually work.

- Fullscreen deferral: M3 already has `DeferredBreak`; just confirm M2's `is_fullscreen` is reliable for at least one full-screen game and one Zoom screen-share. Add a debug print.
- Snooze flow: tray click → small frameless window with "Pause tracking for 30 minutes?" + Confirm/Cancel → on confirm, state machine enters `Snoozed(now + 30min)`

Done when: starting a fullscreen YouTube video right before the timer expires defers the cat until you exit fullscreen. Snoozing actually pauses the draining.

---

## M8 — macOS pass (1 day, possibly more)

Goal: everything that worked on Windows works on Mac.

- Test M1's cat overlay on Mac — `always_on_top` over fullscreen apps is finicky on macOS; may need `NSWindowLevel` adjustment via `tauri::WebviewWindow::set_always_on_top` + platform-specific shimming
- Verify `sensing/mac.rs` returns correct values for foreground/idle/fullscreen
- Autostart on Mac uses LaunchAgents — plugin handles it, but verify
- First-run on Mac: right-click → Open to bypass Gatekeeper (you're not notarizing for personal use)
- Tray icon needs a template-style monochrome variant on Mac for menu bar legibility (Mac doesn't show colored tray icons well in dark mode)

Done when: same end-to-end test from M4 passes on the Mac.

---

## M9 — Polish (½ day)

- Tray icon variants: think about whether the static cat icon needs a "break" or "snoozed" visual variant (out-of-scope per spec, but you might want it after dogfooding)
- Settings window styling: just enough to not look like a 1995 form
- Logging: structured logs to `app_data_dir/log.txt`, rolling
- Crash recovery test: kill pawse mid-Break, restart, confirm forfeit rule works

---

## What's deliberately NOT in v1

- Browser site tracking (decided out of scope at Q1/Q6)
- Multiple cats / custom uploads / sound / theming
- Statistics / history view
- Daily reset, per-app timers
- Hold-to-skip / in-cat dismiss button
- Code signing, notarization, auto-update, landing page, installer polish
- Microsoft Store / Mac App Store submission

These each have a clear path in v2 if v1 sticks.

---

## Rough total
~7 working days end-to-end, single developer, if Tauri behaves on M1. M1 is the schedule risk — if always-on-top fullscreen overlay turns out to need workarounds (especially on Mac), it can eat an extra day or two.
