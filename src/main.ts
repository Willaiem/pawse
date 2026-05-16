import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

type RunningProcess = { exe: string; title: string };

const IS_MAC = navigator.userAgent.toLowerCase().includes("mac os");
const SUGGESTIONS = IS_MAC
  ? ["Discord", "Steam", "Slack"]
  : ["Discord.exe", "Steam.exe", "slack.exe"];

let currentConfig: Config | null = null;

function $(sel: string) {
  return document.querySelector(sel);
}

function formatRemaining(secs: number): string {
  if (secs <= 0) return "0s";
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600);
    const mm = Math.floor((secs % 3600) / 60);
    return mm === 0 ? `${h}h` : `${h}h ${mm}m`;
  }
  return s === 0 ? `${m}m` : `${m}m ${s}s`;
}

function renderState(state: AppState) {
  const kindEl = $("#state-kind") as HTMLElement | null;
  const remEl = $("#state-remaining") as HTMLElement | null;
  if (kindEl) {
    kindEl.textContent = state.kind.replace("_", " ");
    kindEl.dataset.kind = state.kind;
  }
  if (!remEl) return;
  switch (state.kind) {
    case "idle":
    case "active":
    case "break":
      remEl.textContent = formatRemaining(state.remaining_secs);
      break;
    case "snoozed": {
      const left = Math.max(0, state.until_unix - Math.floor(Date.now() / 1000));
      remEl.textContent = `${formatRemaining(left)} left`;
      break;
    }
    case "deferred_break":
      remEl.textContent = "waiting for fullscreen exit";
      break;
  }
}

async function pollState() {
  try {
    renderState(await invoke<AppState>("get_app_state"));
  } catch (err) {
    console.error("get_app_state failed:", err);
  }
}

function avatarLetter(exe: string): string {
  return (exe.replace(/\.(exe|app)$/i, "").charAt(0) || "?").toUpperCase();
}

function renderTrackedApps(cfg: Config) {
  const list = $("#tracked-list") as HTMLUListElement | null;
  if (!list) return;
  list.innerHTML = "";
  if (cfg.tracked_apps.length === 0) {
    const li = document.createElement("li");
    li.className = "tracked-empty";
    li.textContent = "No tracked apps yet — pick one above to start.";
    list.appendChild(li);
    return;
  }
  for (const exe of cfg.tracked_apps) {
    const li = document.createElement("li");
    li.className = "tracked-item";

    const name = document.createElement("div");
    name.className = "tracked-name";
    const avatar = document.createElement("span");
    avatar.className = "tracked-avatar";
    avatar.textContent = avatarLetter(exe);
    const label = document.createElement("span");
    label.textContent = exe;
    name.appendChild(avatar);
    name.appendChild(label);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "btn-danger";
    remove.textContent = "Remove";
    remove.addEventListener("click", async () => {
      try {
        const next = await invoke<Config>("remove_tracked_app", { exe });
        applyConfig(next);
      } catch (err) {
        console.error("remove_tracked_app failed:", err);
      }
    });

    li.appendChild(name);
    li.appendChild(remove);
    list.appendChild(li);
  }
}

function renderSuggestions(cfg: Config) {
  const row = $("#suggestion-row") as HTMLElement | null;
  if (!row) return;
  row.innerHTML = "";
  for (const exe of SUGGESTIONS) {
    const tracked = cfg.tracked_apps.some((t) => t.toLowerCase() === exe.toLowerCase());
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "suggestion-chip";
    chip.setAttribute("aria-pressed", tracked ? "true" : "false");
    chip.textContent = exe.replace(/\.(exe|app)$/i, "");
    chip.title = tracked ? `Remove ${exe}` : `Add ${exe}`;
    chip.addEventListener("click", async () => {
      try {
        const next = tracked
          ? await invoke<Config>("remove_tracked_app", { exe })
          : await invoke<Config>("add_tracked_app", { exe });
        applyConfig(next);
      } catch (err) {
        console.error("toggle suggestion failed:", err);
      }
    });
    row.appendChild(chip);
  }
}

function renderLimits(cfg: Config) {
  const usage = $("#cfg-usage") as HTMLInputElement | null;
  const brk = $("#cfg-break") as HTMLInputElement | null;
  if (usage && document.activeElement !== usage) usage.value = String(cfg.usage_minutes);
  if (brk && document.activeElement !== brk) brk.value = String(cfg.break_minutes);
}

function renderAutostart(cfg: Config) {
  const el = $("#cfg-autostart") as HTMLInputElement | null;
  if (el) el.checked = cfg.autostart;
}

function applyConfig(cfg: Config) {
  currentConfig = cfg;
  renderTrackedApps(cfg);
  renderSuggestions(cfg);
  renderLimits(cfg);
  renderAutostart(cfg);
}

async function refreshConfig() {
  try {
    applyConfig(await invoke<Config>("get_config"));
  } catch (err) {
    console.error("get_config failed:", err);
  }
}

function wireLimits() {
  ($("#cfg-usage") as HTMLInputElement | null)?.addEventListener("change", async (e) => {
    const n = Number((e.target as HTMLInputElement).value);
    if (!Number.isFinite(n)) return;
    try {
      applyConfig(await invoke<Config>("set_usage_minutes", { minutes: n }));
    } catch (err) {
      console.error("set_usage_minutes failed:", err);
      refreshConfig();
    }
  });
  ($("#cfg-break") as HTMLInputElement | null)?.addEventListener("change", async (e) => {
    const n = Number((e.target as HTMLInputElement).value);
    if (!Number.isFinite(n)) return;
    try {
      applyConfig(await invoke<Config>("set_break_minutes", { minutes: n }));
    } catch (err) {
      console.error("set_break_minutes failed:", err);
      refreshConfig();
    }
  });
  ($("#cfg-autostart") as HTMLInputElement | null)?.addEventListener("change", async (e) => {
    const enabled = (e.target as HTMLInputElement).checked;
    try {
      applyConfig(await invoke<Config>("set_autostart", { enabled }));
    } catch (err) {
      console.error("set_autostart failed:", err);
      refreshConfig();
    }
  });
}

let pickerProcesses: RunningProcess[] = [];

async function openPicker() {
  const overlay = $("#picker-overlay") as HTMLElement | null;
  const search = $("#picker-search") as HTMLInputElement | null;
  if (!overlay) return;
  overlay.hidden = false;
  if (search) {
    search.value = "";
    queueMicrotask(() => search.focus());
  }
  await loadPickerProcesses();
}

function closePicker() {
  const overlay = $("#picker-overlay") as HTMLElement | null;
  if (overlay) overlay.hidden = true;
}

async function loadPickerProcesses() {
  const list = $("#picker-list") as HTMLUListElement | null;
  if (list) list.innerHTML = '<li class="picker-empty">Loading running apps…</li>';
  try {
    pickerProcesses = await invoke<RunningProcess[]>("list_running_processes");
  } catch (err) {
    console.error("list_running_processes failed:", err);
    if (list) list.innerHTML = '<li class="picker-empty">Failed to load apps.</li>';
    return;
  }
  renderPickerList();
}

function renderPickerList() {
  const list = $("#picker-list") as HTMLUListElement | null;
  const search = $("#picker-search") as HTMLInputElement | null;
  if (!list) return;
  const q = (search?.value ?? "").trim().toLowerCase();
  const tracked = new Set((currentConfig?.tracked_apps ?? []).map((t) => t.toLowerCase()));

  const filtered = pickerProcesses.filter((p) => {
    if (!q) return true;
    return p.exe.toLowerCase().includes(q) || p.title.toLowerCase().includes(q);
  });

  list.innerHTML = "";
  if (filtered.length === 0) {
    const empty = document.createElement("li");
    empty.className = "picker-empty";
    empty.textContent = pickerProcesses.length === 0
      ? "No user-facing apps detected. Try Refresh."
      : "No matches.";
    list.appendChild(empty);
    return;
  }

  for (const p of filtered) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "picker-item";
    const isTracked = tracked.has(p.exe.toLowerCase());
    btn.disabled = isTracked;

    const avatar = document.createElement("span");
    avatar.className = "tracked-avatar";
    avatar.textContent = avatarLetter(p.exe);

    const meta = document.createElement("span");
    meta.className = "picker-meta";
    const exeLine = document.createElement("span");
    exeLine.className = "picker-exe";
    exeLine.textContent = isTracked ? `${p.exe} (already tracked)` : p.exe;
    meta.appendChild(exeLine);
    if (p.title) {
      const titleLine = document.createElement("span");
      titleLine.className = "picker-title-line";
      titleLine.textContent = p.title;
      meta.appendChild(titleLine);
    }

    btn.appendChild(avatar);
    btn.appendChild(meta);
    btn.addEventListener("click", async () => {
      try {
        applyConfig(await invoke<Config>("add_tracked_app", { exe: p.exe }));
        closePicker();
      } catch (err) {
        console.error("add_tracked_app failed:", err);
      }
    });
    li.appendChild(btn);
    list.appendChild(li);
  }
}

function wirePicker() {
  $("#open-picker")?.addEventListener("click", openPicker);
  $("#picker-close")?.addEventListener("click", closePicker);
  $("#picker-refresh")?.addEventListener("click", loadPickerProcesses);
  $("#picker-overlay")?.addEventListener("click", (e) => {
    if (e.target === e.currentTarget) closePicker();
  });
  ($("#picker-search") as HTMLInputElement | null)?.addEventListener("input", renderPickerList);
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      const overlay = $("#picker-overlay") as HTMLElement | null;
      if (overlay && !overlay.hidden) closePicker();
    }
  });
}

window.addEventListener("DOMContentLoaded", () => {
  wireLimits();
  wirePicker();
  refreshConfig();
  pollState();
  setInterval(pollState, 1000);

  listen<AppState>("state-changed", (e) => renderState(e.payload));
  listen<Config>("config-changed", (e) => applyConfig(e.payload));
});
