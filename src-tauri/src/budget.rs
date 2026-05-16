use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::sensing::ForegroundSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub tracked_apps: Vec<String>,
    pub usage_minutes: u32,
    pub break_minutes: u32,
    pub idle_grace_secs: u32,
    pub autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tracked_apps: Vec::new(),
            usage_minutes: 45,
            break_minutes: 5,
            idle_grace_secs: 60,
            autostart: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppState {
    Idle { remaining_secs: u32 },
    Active { remaining_secs: u32 },
    Break { remaining_secs: u32 },
    DeferredBreak,
    Snoozed { until_unix: u64 },
}

impl AppState {
    pub fn kind_label(&self) -> &'static str {
        match self {
            AppState::Idle { .. } => "idle",
            AppState::Active { .. } => "active",
            AppState::Break { .. } => "break",
            AppState::DeferredBreak => "deferred_break",
            AppState::Snoozed { .. } => "snoozed",
        }
    }

    fn same_kind(&self, other: &AppState) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Transition {
    pub from: AppState,
    pub to: AppState,
}

pub struct BudgetMachine {
    pub config: Config,
    pub state: AppState,
    config_path: PathBuf,
    state_path: PathBuf,
    last_tick: Instant,
    last_save: Instant,
}

const SAVE_HEARTBEAT_SECS: u64 = 60;

impl BudgetMachine {
    pub fn load(config_path: PathBuf, state_path: PathBuf) -> Self {
        let config = load_or_init_config(&config_path);
        let state = load_or_init_state(&state_path, &config);
        let now = Instant::now();
        Self {
            config,
            state,
            config_path,
            state_path,
            last_tick: now,
            last_save: now,
        }
    }

    pub fn tick(&mut self, snap: &ForegroundSnapshot) -> (Option<Transition>, bool) {
        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_tick).as_secs() as u32;
        self.last_tick = now;

        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let is_tracked = snap
            .exe
            .as_ref()
            .map(|exe| {
                self.config
                    .tracked_apps
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(exe))
            })
            .unwrap_or(false);
        let is_active = is_tracked && snap.idle_for_secs < u64::from(self.config.idle_grace_secs);

        let prev = self.state.clone();
        self.state = match self.state.clone() {
            AppState::Idle { remaining_secs } => {
                if is_active {
                    AppState::Active { remaining_secs }
                } else {
                    AppState::Idle { remaining_secs }
                }
            }
            AppState::Active { remaining_secs } => {
                if !is_active {
                    AppState::Idle { remaining_secs }
                } else {
                    let next = remaining_secs.saturating_sub(elapsed_secs);
                    if next == 0 {
                        let break_secs = self.config.break_minutes.saturating_mul(60).max(1);
                        if snap.is_fullscreen {
                            AppState::DeferredBreak
                        } else {
                            AppState::Break {
                                remaining_secs: break_secs,
                            }
                        }
                    } else {
                        AppState::Active {
                            remaining_secs: next,
                        }
                    }
                }
            }
            AppState::DeferredBreak => {
                if snap.is_fullscreen {
                    AppState::DeferredBreak
                } else {
                    let break_secs = self.config.break_minutes.saturating_mul(60).max(1);
                    AppState::Break {
                        remaining_secs: break_secs,
                    }
                }
            }
            AppState::Break { remaining_secs } => {
                let next = remaining_secs.saturating_sub(elapsed_secs);
                if next == 0 {
                    AppState::Active {
                        remaining_secs: self.config.usage_minutes.saturating_mul(60).max(1),
                    }
                } else {
                    AppState::Break {
                        remaining_secs: next,
                    }
                }
            }
            AppState::Snoozed { until_unix } => {
                if now_unix >= until_unix {
                    AppState::Active {
                        remaining_secs: self.config.usage_minutes.saturating_mul(60).max(1),
                    }
                } else {
                    AppState::Snoozed { until_unix }
                }
            }
        };

        let transition = if !prev.same_kind(&self.state) {
            Some(Transition {
                from: prev,
                to: self.state.clone(),
            })
        } else {
            None
        };

        let needs_save = transition.is_some()
            || now.duration_since(self.last_save).as_secs() >= SAVE_HEARTBEAT_SECS;
        if needs_save {
            self.last_save = now;
            if let Err(e) = write_json(&self.state_path, &self.state) {
                eprintln!("[pawse] failed to persist state: {e}");
            }
        }

        (transition, needs_save)
    }

    pub fn snooze_for(&mut self, secs: u64) -> Transition {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let prev = self.state.clone();
        self.state = AppState::Snoozed {
            until_unix: now_unix + secs,
        };
        if let Err(e) = write_json(&self.state_path, &self.state) {
            eprintln!("[pawse] failed to persist state after snooze: {e}");
        }
        Transition {
            from: prev,
            to: self.state.clone(),
        }
    }

    pub fn save_config(&self) -> Result<(), std::io::Error> {
        write_json(&self.config_path, &self.config)
    }
}

pub struct BudgetState {
    pub machine: Mutex<BudgetMachine>,
}

impl BudgetState {
    pub fn new(machine: BudgetMachine) -> Self {
        Self {
            machine: Mutex::new(machine),
        }
    }
}

fn load_or_init_config(path: &Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<Config>(&s) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[pawse] config.json parse error ({e}); using defaults");
                let c = Config::default();
                let _ = write_json(path, &c);
                c
            }
        },
        Err(_) => {
            let c = Config::default();
            let _ = write_json(path, &c);
            c
        }
    }
}

fn load_or_init_state(path: &Path, config: &Config) -> AppState {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<AppState>(&s) {
            Ok(state) => apply_launch_rules(state, config),
            Err(e) => {
                eprintln!("[pawse] state.json parse error ({e}); resetting");
                fresh_idle(config)
            }
        },
        Err(_) => fresh_idle(config),
    }
}

fn fresh_idle(config: &Config) -> AppState {
    AppState::Idle {
        remaining_secs: config.usage_minutes.saturating_mul(60).max(1),
    }
}

fn apply_launch_rules(persisted: AppState, _config: &Config) -> AppState {
    match persisted {
        // Forfeit rule: killing the app mid-break does not earn a refilled budget.
        // Next tracked-app focus will immediately trigger another break.
        AppState::Break { .. } | AppState::DeferredBreak => {
            AppState::Idle { remaining_secs: 0 }
        }
        other => other,
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
