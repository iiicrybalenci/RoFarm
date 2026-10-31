import { Show } from "solid-js";
import { Icon } from "./Icon";
import { HoldButton, Meter } from "./Primitives";
import {
  globalFilter,
  killAll,
  openPalette,
  runningClients,
  setDensity,
  setGlobalFilter,
  setNotificationsOpen,
  setView,
  settings,
  snapshot,
  unreadNotices,
  updateSettings,
} from "../lib/store";
import { formatBytes } from "../lib/format";
import { t } from "../lib/i18n";

export function FleetBar() {
  const totals = () => snapshot().totals;
  const accounts = () => Object.keys(snapshot().fleet.accounts).length;
  const system = () => (snapshot() as any).system ?? null;
  const netDown = () => system()?.net_down_kbps ?? 0;
  const netUp = () => system()?.net_up_kbps ?? 0;
  const gpu = () => system()?.gpu_pct ?? null;
  const cpuPct = () => Math.min(100, totals().cpu_pct);
  const memPct = () => (system()?.mem_total_mb ? (totals().mem_mb / system().mem_total_mb) * 100 : 0);
  const density = () => settings().density;
  return (
    <div class="fleetbar" role="region" aria-label="Fleet status">
      <button class="stat-chip" onClick={() => setView("fleet")} title="Open Fleet view">
        <span class="k">{t("fleet.running")}</span>
        <span class="v">{totals().running}</span>
        <span class="muted tiny">/{accounts()}</span>
      </button>
      <div class="stat-chip hide-md" title={`${totals().cpu_pct.toFixed(1)}% CPU across all clients`}>
        <span class="k">{t("fleet.cpu")}</span>
        <span class="v">{cpuPct().toFixed(1)}%</span>
        <Meter value={cpuPct()} max={100} warnAt={70} dangerAt={90} />
      </div>
      <div class="stat-chip hide-md">
        <span class="k">{t("fleet.memory")}</span>
        <span class="v">{formatBytes(totals().mem_mb)}</span>
        <Show when={system()?.mem_total_mb}><span class="muted tiny">/{formatBytes(system().mem_total_mb)}</span></Show>
        <Meter value={memPct()} max={100} warnAt={75} dangerAt={92} />
      </div>
      <div class="stat-chip hide-md" title="GPU load from the Windows performance counters">
        <span class="k">GPU</span>
        <span class="v">{gpu() === null || gpu() === undefined ? "—" : `${Math.round(gpu())}%`}</span>
      </div>
      <div class="stat-chip hide-md" title="Network throughput across Roblox clients">
        <span class="k">NET</span>
        <span class="v">↓{formatRate(netDown())}</span>
        <span class="muted tiny">↑{formatRate(netUp())}</span>
      </div>
      <div class="spacer" />
      <label class="searchbox" title="Search accounts, notes and tags">
        <Icon name="search" size={14} />
        <input
          value={globalFilter()}
          onInput={(event) => setGlobalFilter(event.currentTarget.value)}
          placeholder="Filter fleet…"
          aria-label="Filter fleet"
        />
        <Show when={!globalFilter()}><kbd>Ctrl K</kbd></Show>
      </label>
      <button class="icon-btn" onClick={openPalette} aria-label="Command palette" title="Command palette (Ctrl+K)">
        <Icon name="command" size={18} />
      </button>
      <div class="segmented" role="group" aria-label="Density">
        <button class={density() === "comfortable" ? "active" : ""} onClick={() => setDensity("comfortable")} title="Comfortable density">Cozy</button>
        <button class={density() === "compact" ? "active" : ""} onClick={() => setDensity("compact")} title="Compact density">Compact</button>
        <button class={density() === "ultra_compact" ? "active" : ""} onClick={() => setDensity("ultra_compact")} title="Ultra-compact table density">Ultra</button>
      </div>
      <button
        class="icon-btn"
        classList={{ "eco-on": settings().eco_mode }}
        aria-label="Eco mode"
        title={settings().eco_mode ? "Eco mode on — click to disable" : "Eco mode — lower refresh, no thumbnails, less CPU"}
        onClick={() => updateSettings((draft) => { draft.eco_mode = !draft.eco_mode; })}
        style={settings().eco_mode ? { color: "var(--ok)" } : undefined}
      >
        <Icon name="spark" size={17} />
      </button>
      <button class="icon-btn" aria-label="Notifications" title="Notification center" onClick={() => setNotificationsOpen(true)}>
        <Icon name="bell" size={17} />
        <Show when={unreadNotices() > 0}><span class="badge">{unreadNotices() > 9 ? "9+" : unreadNotices()}</span></Show>
      </button>
      <HoldButton
        onConfirm={killAll}
        label={runningClients().length ? `Hold to stop ${runningClients().length}` : "Kill all"}
        icon="kill"
        disabled={!runningClients().length}
        class="sm"
      />
    </div>
  );
}

function formatRate(kbps: number): string {
  if (!kbps) return "0";
  if (kbps >= 1024) return `${(kbps / 1024).toFixed(1)} MB/s`;
  return `${Math.round(kbps)} KB/s`;
}
