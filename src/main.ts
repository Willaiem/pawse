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
});
