import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type MonitorInfo = {
  index: number;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  is_primary: boolean;
};

type MonitorRef = { x: number; y: number; width: number; height: number };

type ForegroundSnapshot = {
  exe: string | null;
  idle_for_secs: number;
  is_fullscreen: boolean;
  monitor_index: number | null;
  monitor: MonitorRef | null;
};

type AppState =
  | { kind: "idle"; remaining_secs: number }
  | { kind: "active"; remaining_secs: number }
  | { kind: "break"; remaining_secs: number }
  | { kind: "deferred_break" }
  | { kind: "snoozed"; until_unix: number };

type Config = {
  tracked_apps: string[];
  usage_minutes: number;
  break_minutes: number;
  idle_grace_secs: number;
  autostart: boolean;
};

let greetInputEl: HTMLInputElement | null;
let greetMsgEl: HTMLElement | null;
let trackedAppsCache: string[] = [];

async function greet() {
  if (greetMsgEl && greetInputEl) {
    greetMsgEl.textContent = await invoke("greet", { name: greetInputEl.value });
  }
}

async function refreshMonitors() {
  const list = document.querySelector<HTMLDivElement>("#monitor-list");
  if (!list) return;
  try {
    const monitors = await invoke<MonitorInfo[]>("list_monitors");
    list.innerHTML = "";
    if (monitors.length === 0) {
      list.textContent = "No monitors detected.";
      return;
    }
    for (const m of monitors) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "monitor-btn";
      btn.textContent = `Monitor ${m.index} — ${m.width}×${m.height}${m.is_primary ? " · primary" : ""}`;
      btn.addEventListener("click", async () => {
        try {
          await invoke("show_cat", { monitorIndex: m.index });
        } catch (err) {
          console.error("show_cat failed:", err);
        }
      });
      list.appendChild(btn);
    }
  } catch (err) {
    list.textContent = `Failed to list monitors: ${err}`;
  }
}

function formatIdle(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m ${s}s`;
}

function formatRemaining(state: AppState): string {
  switch (state.kind) {
    case "idle":
    case "active":
    case "break":
      return formatIdle(state.remaining_secs);
    case "snoozed": {
      const left = Math.max(0, state.until_unix - Math.floor(Date.now() / 1000));
      return `${formatIdle(left)} left`;
    }
    case "deferred_break":
      return "waiting for fullscreen exit";
  }
}

async function pollSnapshot() {
  const exeEl = document.querySelector<HTMLElement>("#snap-exe");
  const idleEl = document.querySelector<HTMLElement>("#snap-idle");
  const fsEl = document.querySelector<HTMLElement>("#snap-fullscreen");
  const monEl = document.querySelector<HTMLElement>("#snap-monitor");
  try {
    const snap = await invoke<ForegroundSnapshot>("current_snapshot");
    if (exeEl) exeEl.textContent = snap.exe ?? "(none)";
    if (idleEl) idleEl.textContent = formatIdle(snap.idle_for_secs);
    if (fsEl) fsEl.textContent = snap.is_fullscreen ? "yes" : "no";
    if (monEl) {
      monEl.textContent = snap.monitor_index === null
        ? "(unknown)"
        : `#${snap.monitor_index}${snap.monitor ? ` · ${snap.monitor.width}×${snap.monitor.height} @ (${snap.monitor.x},${snap.monitor.y})` : ""}`;
    }
  } catch (err) {
    if (exeEl) exeEl.textContent = `error: ${err}`;
  }
}

function renderState(state: AppState) {
  const kindEl = document.querySelector<HTMLElement>("#state-kind");
  const remEl = document.querySelector<HTMLElement>("#state-remaining");
  if (kindEl) {
    kindEl.textContent = state.kind;
    kindEl.dataset.kind = state.kind;
  }
  if (remEl) remEl.textContent = formatRemaining(state);
}

async function pollAppState() {
  try {
    renderState(await invoke<AppState>("get_app_state"));
  } catch (err) {
    console.error("get_app_state failed:", err);
  }
}

function renderConfig(cfg: Config) {
  trackedAppsCache = cfg.tracked_apps;
  const usageEl = document.querySelector<HTMLInputElement>("#cfg-usage");
  const breakEl = document.querySelector<HTMLInputElement>("#cfg-break");
  if (usageEl && document.activeElement !== usageEl) usageEl.value = String(cfg.usage_minutes);
  if (breakEl && document.activeElement !== breakEl) breakEl.value = String(cfg.break_minutes);

  const list = document.querySelector<HTMLUListElement>("#tracked-list");
  if (!list) return;
  list.innerHTML = "";
  if (cfg.tracked_apps.length === 0) {
    const li = document.createElement("li");
    li.className = "tracked-empty";
    li.textContent = "No tracked apps yet — add one below.";
    list.appendChild(li);
    return;
  }
  for (const exe of cfg.tracked_apps) {
    const li = document.createElement("li");
    li.className = "tracked-item";
    const label = document.createElement("span");
    label.textContent = exe;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = "Remove";
    btn.addEventListener("click", async () => {
      try {
        const next = await invoke<Config>("remove_tracked_app", { exe });
        renderConfig(next);
      } catch (err) {
        console.error("remove_tracked_app failed:", err);
      }
    });
    li.appendChild(label);
    li.appendChild(btn);
    list.appendChild(li);
  }
}

async function refreshConfig() {
  try {
    renderConfig(await invoke<Config>("get_config"));
  } catch (err) {
    console.error("get_config failed:", err);
  }
}

function renderRecent(exes: string[]) {
  const list = document.querySelector<HTMLDivElement>("#recent-list");
  if (!list) return;
  list.innerHTML = "";
  if (exes.length === 0) {
    const span = document.createElement("span");
    span.className = "recent-empty";
    span.textContent = "focus another app to populate";
    list.appendChild(span);
    return;
  }
  for (const exe of exes) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "recent-chip";
    const isTracked = trackedAppsCache.some((t) => t.toLowerCase() === exe.toLowerCase());
    chip.disabled = isTracked;
    chip.textContent = isTracked ? `${exe} ✓` : exe;
    chip.title = isTracked ? "already tracked" : `Track ${exe}`;
    chip.addEventListener("click", async () => {
      try {
        const next = await invoke<Config>("add_tracked_app", { exe });
        renderConfig(next);
      } catch (err) {
        console.error("add_tracked_app failed:", err);
      }
    });
    list.appendChild(chip);
  }
}

async function pollRecent() {
  try {
    renderRecent(await invoke<string[]>("recent_foregrounds"));
  } catch (err) {
    console.error("recent_foregrounds failed:", err);
  }
}

function wireStateControls() {
  document.querySelector("#cfg-usage")?.addEventListener("change", async (e) => {
    const target = e.target as HTMLInputElement;
    const n = Number(target.value);
    if (!Number.isFinite(n)) return;
    try {
      renderConfig(await invoke<Config>("set_usage_minutes", { minutes: n }));
    } catch (err) {
      console.error("set_usage_minutes failed:", err);
      refreshConfig();
    }
  });
  document.querySelector("#cfg-break")?.addEventListener("change", async (e) => {
    const target = e.target as HTMLInputElement;
    const n = Number(target.value);
    if (!Number.isFinite(n)) return;
    try {
      renderConfig(await invoke<Config>("set_break_minutes", { minutes: n }));
    } catch (err) {
      console.error("set_break_minutes failed:", err);
      refreshConfig();
    }
  });
  document.querySelector("#snooze-btn")?.addEventListener("click", async () => {
    try {
      const next = await invoke<AppState>("snooze", { seconds: 30 * 60 });
      renderState(next);
    } catch (err) {
      console.error("snooze failed:", err);
    }
  });
  document.querySelector("#tracked-add")?.addEventListener("click", async () => {
    const input = document.querySelector<HTMLInputElement>("#tracked-input");
    if (!input) return;
    const exe = input.value.trim();
    if (!exe) return;
    try {
      const next = await invoke<Config>("add_tracked_app", { exe });
      input.value = "";
      renderConfig(next);
    } catch (err) {
      console.error("add_tracked_app failed:", err);
    }
  });
}

window.addEventListener("DOMContentLoaded", () => {
  greetInputEl = document.querySelector("#greet-input");
  greetMsgEl = document.querySelector("#greet-msg");
  document.querySelector("#greet-form")?.addEventListener("submit", (e) => {
    e.preventDefault();
    greet();
  });
  document.querySelector("#hide-cat")?.addEventListener("click", () => {
    invoke("hide_cat").catch((err) => console.error("hide_cat failed:", err));
  });

  refreshMonitors();
  refreshConfig();
  pollSnapshot();
  pollAppState();
  pollRecent();
  setInterval(pollSnapshot, 1000);
  setInterval(pollAppState, 1000);
  setInterval(pollRecent, 1500);

  listen<AppState>("state-changed", (e) => renderState(e.payload));
  listen<Config>("config-changed", (e) => {
    renderConfig(e.payload);
    pollRecent();
  });

  wireStateControls();
});
