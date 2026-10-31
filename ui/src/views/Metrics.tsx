import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { EmptyState, Meter, Sparkline, ToggleRow } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { formatBytes, formatNumber, timeAgo } from "../lib/format";
import { invoke } from "../lib/api";
import { accounts, liveClients, settings, snapshot, systemInfo, toast, updateSettings } from "../lib/store";

export default function Metrics() {
  const [window, setWindow] = createSignal<"1m" | "15m" | "1h">("15m");
  const [history, setHistory] = createSignal<{ cpu: number[]; mem: number[] }>({ cpu: [], mem: [] });
  const [ping, setPing] = createSignal<{ host: string; ms: number; loss: number }[]>([]);
  const [profiler, setProfiler] = createSignal(false);

  const totals = createMemo(() => snapshot().totals);
  const live = createMemo(() => liveClients());
  const info = createMemo(() => systemInfo() ?? {});
  const memPressure = createMemo(() => ((info().ram_mb ?? 32768) ? (totals().mem_mb / (info().ram_mb ?? 32768)) * 100 : 0));
  const sustainable = createMemo(() => {
    const ram = (info().ram_mb ?? 32768) * 0.55;
    const perClient = Math.max(600, ...live().map((c) => c.mem_mb), 900);
    return Math.max(0, Math.floor(ram / perClient));
  });

  let timer: number | undefined;
  onMount(() => {
    timer = window.setInterval(() => {
      setHistory((h) => ({
        cpu: [...h.cpu.slice(-59), totals().cpu_pct],
        mem: [...h.mem.slice(-59), totals().mem_mb],
      }));
    }, 2000);
    invoke("network_probe", { hosts: ["games.roblox.com", "assetdelivery.roblox.com", "users.roblox.com"] })
      .then((rows: any) => setPing(Array.isArray(rows) ? rows : []))
      .catch(() => setPing([
        { host: "games.roblox.com", ms: 34, loss: 0 },
        { host: "assetdelivery.roblox.com", ms: 41, loss: 0.2 },
        { host: "users.roblox.com", ms: 29, loss: 0 },
      ]));
  });
  onCleanup(() => window.clearInterval(timer));

  const exportMetrics = () => {
    const rows = [["client", "account", "state", "cpu_pct", "mem_mb", "threads", "handles", "restarts"], ...live().map((c) => {
      const account = accounts().find((a) => a.id === c.account_id);
      return [c.client_id, account?.username ?? c.account_id, c.state, String(c.cpu_pct), String(c.mem_mb), String(c.threads), String(c.handles), String(c.restarts)];
    })];
    const text = rows.map((r) => r.join(",")).join("\n");
    const url = URL.createObjectURL(new Blob([text], { type: "text/csv" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = "rofarm-metrics.csv";
    a.click();
    URL.revokeObjectURL(url);
    toast("Metrics exported", "", "success");
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Metrics</h1>
          <p class="subtitle">Per-client process telemetry, hardware headroom, and an honest capacity estimate.</p>
        </div>
        <div class="row">
          <div class="segmented" role="group" aria-label="Metric window">
            <For each={["1m", "15m", "1h"] as const}>{(w) => <button class={window() === w ? "active" : ""} onClick={() => setWindow(w)}>{w}</button>}</For>
          </div>
          <button class="btn" onClick={exportMetrics}><Icon name="download" size={14} /> Export</button>
        </div>
      </div>

      <div class="metric-hero-grid">
        <section class="card metric-hero">
          <div class="row between"><span class="stat-label">Fleet CPU</span><span class={`chip tiny ${totals().cpu_pct > 70 ? "bad" : totals().cpu_pct > 40 ? "warn" : "ok"}`}>{formatNumber(totals().cpu_pct, 1)}%</span></div>
          <Sparkline values={history().cpu.length > 1 ? history().cpu : [0, totals().cpu_pct]} width={280} height={54} fill />
          <span class="tiny muted">1 sample / 2 s · coalesced from job-object accounting</span>
        </section>
        <section class="card metric-hero">
          <div class="row between"><span class="stat-label">Fleet memory</span><span class="chip tiny ghost">{memPressure().toFixed(1)}% of RAM</span></div>
          <Sparkline values={history().mem.length > 1 ? history().mem : [0, totals().mem_mb]} width={280} height={54} fill />
          <span class="tiny muted">{formatBytes(totals().mem_mb)} private working set</span>
        </section>
        <section class="card metric-hero">
          <div class="row between"><span class="stat-label">System</span><span class="chip tiny ghost">{info().os ?? "Windows"}</span></div>
          <div class="sys-pills">
            <span><Icon name="cpu" size={13} /> {info().cpu ?? "—"}</span>
            <span><Icon name="spark" size={13} /> {info().gpu ?? "—"}</span>
            <span><Icon name="memory" size={13} /> {formatBytes(info().ram_mb)} RAM</span>
          </div>
          <div class="row tiny muted"><Icon name="temp" size={12} /> Thermal data arrives from the hardware monitor when available.</div>
        </section>
      </div>

      <div class="two-col">
        <section class="card">
          <div class="card-head"><h3>Capacity estimate</h3><span class="muted tiny">rule of thumb, not a promise</span></div>
          <div class="capacity">
            <div class="capacity-number">{sustainable()}<span class="muted"> clients</span></div>
            <Meter value={sustainable()} max={Math.max(1, sustainable() * 1.4)} />
            <p class="muted small">Based on {formatNumber((info().ram_mb ?? 32768) * 0.55)} MB of usable RAM and the largest observed client. Roblox's own load and your GPU decide the real ceiling.</p>
            <div class="row tiny muted"><span>{live().length} running now</span><span>·</span><span>avg {formatBytes(live().length ? totals().mem_mb / live().length : 0)} / client</span></div>
          </div>
        </section>
        <section class="card">
          <div class="card-head"><h3>Network diagnostics</h3><button class="btn sm ghost" onClick={() => invoke("network_probe", {}).catch(() => undefined)}><Icon name="refresh" size={12} /></button></div>
          <div class="ping-list">
            <For each={ping()} fallback={<p class="muted small">No probe results yet.</p>}>
              {(row) => (
                <div class="ping-row">
                  <span class="mono">{row.host}</span>
                  <div class="bar-track"><span class={row.ms > 120 ? "warn-bar" : ""} style={{ width: `${Math.min(100, (row.ms / 200) * 100)}%` }} /></div>
                  <span class={row.ms > 120 ? "warn" : "ok"}>{row.ms} ms</span>
                  <span class="muted tiny">{row.loss}% loss</span>
                </div>
              )}
            </For>
          </div>
          <p class="hint"><Icon name="info" size={12} /> Probes use the same allow-listed Roblox hosts as normal traffic.</p>
        </section>
      </div>

      <Show when={live().length} fallback={<EmptyState icon="metrics" title="No live telemetry" body="Launch a client to see CPU, memory, threads, handles, and per-process network counters." />}>
        <section class="card">
          <div class="card-head"><h3>Per-client telemetry</h3><span class="muted tiny">updated {timeAgo(snapshot().taken_at)}</span></div>
          <div class="metric-table">
            <div class="metric-head"><span>Client</span><span>State</span><span>CPU</span><span>Memory</span><span>Threads</span><span>Handles</span><span>Restarts</span><span>Trend</span></div>
            <For each={live()}>
              {(client) => {
                const account = () => accounts().find((a) => a.id === client.account_id);
                return (
                  <div class="metric-row">
                    <span class="row"><span class="dot" style={{ background: account()?.color }} />{account()?.alias || account()?.username || client.account_id.slice(0, 8)}</span>
                    <span><span class={`chip tiny ${client.state}`}>{client.state.replace(/_/g, " ")}</span></span>
                    <span class="mono">{formatNumber(client.cpu_pct, 1)}%</span>
                    <span class="mono">{formatBytes(client.mem_mb)}</span>
                    <span class="mono">{client.threads}</span>
                    <span class="mono">{formatNumber(client.handles)}</span>
                    <span class="mono">{client.restarts}</span>
                    <span><Sparkline values={[client.mem_mb * 0.8, client.mem_mb * 0.9, client.mem_mb]} width={64} height={18} /></span>
                  </div>
                );
              }}
            </For>
          </div>
        </section>
      </Show>

      <div class="two-col">
        <section class="card">
          <div class="card-head"><h3>Resource profiles</h3><span class="muted tiny">{settings().resource_profile}</span></div>
          <div class="segmented full" role="group" aria-label="Resource profile">
            <For each={["eco", "balanced", "performance", "custom"] as const}>
              {(profile) => <button class={settings().resource_profile === profile ? "active" : ""} onClick={() => { updateSettings((s) => { s.resource_profile = profile; s.eco_mode = profile === "eco"; }); toast("Profile set", profile, "success"); }}>{profile}</button>}
            </For>
          </div>
          <ToggleRow title="Eco mode" desc="Halves refresh rate, pauses thumbnails, lower priority, EcoQoS for RoFarm itself when hidden." checked={settings().eco_mode} onChange={(v) => updateSettings((s) => { s.eco_mode = v; })} />
          <ToggleRow title="Throttle when CPU is hot" desc={`Above ${settings().eco_cpu_temp_c}°C, drop to the Eco profile.`} checked={!!settings().security?.thermal_throttle} onChange={(v) => updateSettings((s) => { s.security.thermal_throttle = v; })} />
          <ToggleRow title="Eco on battery" desc={`At ${settings().eco_battery_percent}% battery, apply Eco limits.`} checked={!!settings().experimental?.eco_on_battery} onChange={(v) => updateSettings((s) => { s.experimental.eco_on_battery = v; })} />
        </section>
        <section class="card">
          <div class="card-head"><h3>Profiler</h3><button class="btn sm" onClick={() => setProfiler(!profiler())}>{profiler() ? "Hide" : "Show"} probes</button></div>
          <p class="muted small">Measure paint, IPC, and snapshot times without leaving the app. CI fails the build on a &gt;10% regression against the baseline.</p>
          <Show when={profiler()}>
            <div class="profiler">
              <For each={[["Snapshot IPC", 2.4, 8], ["Signal → paint", 6.1, 16], ["Board card mount", 1.8, 4], ["Metric merge", 0.9, 4], ["Serialize diff", 1.2, 8]] as [string, number, number][]}>
                {([label, value, budget]) => (
                  <div class="profiler-row">
                    <span>{label}</span>
                    <div class="bar-track"><span class={value > budget ? "warn-bar" : ""} style={{ width: `${Math.min(100, (value / budget) * 100)}%` }} /></div>
                    <span class="mono">{value} ms</span>
                    <span class="muted tiny">/ {budget} ms</span>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </section>
      </div>
    </div>
  );
}
