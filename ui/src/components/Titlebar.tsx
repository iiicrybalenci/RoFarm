import { Show } from "solid-js";
import { Icon } from "./Icon";
import { bootstrap, connection, isDemo, mini, view } from "../lib/store";
import { t } from "../lib/i18n";
import { api } from "../lib/api";

async function windowAction(action: "minimize" | "maximize" | "close") {
  if (isDemo()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    if (action === "minimize") await win.minimize();
    if (action === "maximize") await win.toggleMaximize();
    if (action === "close") await win.close();
  } catch {
  }
}

export function Titlebar() {
  return (
    <header class="titlebar" data-tauri-drag-region onDblClick={() => windowAction("maximize")}>
      <div class="brand" data-tauri-drag-region>
        <div class="brand-mark">R</div>
        <span data-tauri-drag-region>RoFarm</span>
      </div>
      <div class="crumb" data-tauri-drag-region>
        <span>/</span>
        <span>{t("app.tagline")}</span>
        <span>/</span>
        <span>{t(`nav.${view()}`)}</span>
      </div>
      <div class="spacer" data-tauri-drag-region />
      <div class="workspace-pill" title="Local-only workspace. No telemetry.">
        <span class={`dot ${connection() === "offline" ? "failed" : ""}`} />
        <span>{isDemo() ? "Demo workspace" : "Personal"}</span>
        <span style={{ opacity: 0.45 }}>·</span>
        <span>{bootstrap()?.portable ? "Portable" : "Local only"}</span>
      </div>
      <Show when={connection() === "offline"}>
        <span class="pill danger">core offline</span>
      </Show>
      <div class="window-actions">
        <button class="win-btn" aria-label="Minimize" onClick={() => windowAction("minimize")}><Icon name="minimize" size={14} /></button>
        <button class="win-btn" aria-label="Maximize" onClick={() => windowAction("maximize")}><Icon name="maximize" size={13} /></button>
        <button class="win-btn close" aria-label="Close" onClick={() => windowAction("close")}><Icon name="close" size={14} /></button>
      </div>
    </header>
  );
}

export async function enterMiniMode() {
  if (isDemo()) {
    window.open(`${location.pathname}?mini=1`, "_blank", "width=320,height=420");
    return;
  }
  await api.snapshot().catch(() => undefined);
  await import("@tauri-apps/api/core").then(({ invoke }) => invoke("show_mini")).catch(() => undefined);
}

export { mini };
