import { invoke } from "@tauri-apps/api/core";

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

let greetInputEl: HTMLInputElement | null;
let greetMsgEl: HTMLElement | null;

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
  pollSnapshot();
  setInterval(pollSnapshot, 1000);
});
