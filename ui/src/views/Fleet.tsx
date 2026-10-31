import { For, Show, createMemo, createSignal } from "solid-js";
import { Avatar, EmptyState, HoldButton, Meter, Sparkline } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { formatBytes, formatClock, formatNumber, timeAgo } from "../lib/format";
import { api, invoke } from "../lib/api";
import {
  accounts,
  killAll,
  killClient,
  liveClients,
  refresh,
  snapshot,
  toast,
  withBusy,
} from "../lib/store";
import type { LiveClient } from "../lib/types";

type Layout = "grid" | "tile" | "cascade";

export default function Fleet() {
  const [layout, setLayout] = createSignal<Layout>("grid");
  const [columns, setColumns] = createSignal(2);
  const [selectedClient, setSelectedClient] = createSignal<string | null>(null);
  const [autoRefresh, setAutoRefresh] = createSignal(true);

  const accountFor = (client: LiveClient) => snapshot().fleet.accounts[client.account_id];
  const totals = createMemo(() => snapshot().totals);
  const visible = createMemo(() => liveClients().filter((c) => c.state !== "stopped"));

  const applyLayout = async (next: Layout) => {
    setLayout(next);
    await withBusy("Arranging client windows", async () => {
      await invoke("auto_tile", { layout: next });
      toast("Layout applied", next === "cascade" ? "Windows cascaded on the primary monitor." : `Clients arranged in a ${next}.`, "success");
    });
  };

  const focus = async (client: LiveClient) => {
    try {
      await invoke("focus_client", { clientId: client.client_id });
    } catch (error: any) {
      toast("Could not focus window", String(error?.message ?? error), "error");
    }
  };

  const screenshot = async (client: LiveClient) => {
    await withBusy("Capturing client window", async () => {
      await invoke("screenshot_client", { clientId: client.client_id });
      toast("Screenshot saved", "Check the screenshots folder in your data directory.", "success");
    });
  };

  return (
    <div class="page fleet-page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Fleet</h1>
          <p class="subtitle">Live windows, per-client load, and one-click layout control.</p>
        </div>
        <div class="row">
          <div class="segmented" role="group" aria-label="Window layout">
            <button class={layout() === "grid" ? "active" : ""} onClick={() => applyLayout("grid")} title="Grid"><Icon name="grid" size={14} /> Grid</button>
            <button class={layout() === "tile" ? "active" : ""} onClick={() => applyLayout("tile")} title="Tile"><Icon name="layout" size={14} /> Tile</button>
            <button class={layout() === "cascade" ? "active" : ""} onClick={() => applyLayout("cascade")} title="Cascade"><Icon name="card" size={14} /> Cascade</button>
          </div>
          <HoldButton label="Kill all" icon="kill" onConfirm={killAll} disabled={!visible().length} />
        </div>
      </div>

      <div class="stat-strip">
        <div class="stat"><span class="stat-label">Running</span><strong>{visible().length}</strong><span class="stat-note">of {accounts().length} accounts</span></div>
        <div class="stat"><span class="stat-label">CPU</span><strong>{formatNumber(totals().cpu_pct, 1)}%</strong><Sparkline values={[1, 2, 1.6, 3, 2.4, totals().cpu_pct]} width={72} height={18} /></div>
        <div class="stat"><span class="stat-label">Memory</span><strong>{formatBytes(totals().mem_mb)}</strong><span class="stat-note">across all clients</span></div>
        <div class="stat"><span class="stat-label">Restarts</span><strong>{totals().restarts}</strong><span class="stat-note">{totals().crashes} crash{totals().crashes === 1 ? "" : "es"}</span></div>
        <div class="stat"><span class="stat-label">Last sync</span><strong>{timeAgo(snapshot().taken_at)}</strong><span class="stat-note">{autoRefresh() ? "event-driven" : "paused"}</span></div>
      </div>

      <Show
        when={visible().length}
        fallback={
          <EmptyState
            icon="fleet"
            title="No clients running"
            body="Launch one or more accounts from the Board. Windows will appear here with live metrics, thumbnails, and quick actions."
          />
        }
      >
        <div class="fleet-toolbar">
          <div class="row" style={{ gap: "4px" }}>
            <span class="muted small">Thumbnails</span>
            <div class="segmented sm" role="group" aria-label="Thumbnail columns">
              <For each={[1, 2, 3, 4]}>
                {(n) => <button class={columns() === n ? "active" : ""} onClick={() => setColumns(n)}>{n}</button>}
              </For>
            </div>
          </div>
          <div class="grow" />
          <label class="row tiny muted" style={{ gap: "6px" }}>
            <input type="checkbox" checked={autoRefresh()} onChange={(e) => setAutoRefresh(e.currentTarget.checked)} />
            Live refresh
          </label>
          <button class="btn sm ghost" onClick={() => refresh()}><Icon name="refresh" size={13} /> Refresh now</button>
        </div>

        <div class="fleet-grid" style={{ "--cols": String(columns()) }}>
          <For each={visible()}>
            {(client) => {
              const account = () => accountFor(client);
              const health = () => account()?.health.score ?? 0;
              const active = () => selectedClient() === client.client_id;
              return (
                <article
                  class={`fleet-card ${active() ? "active" : ""} state-${client.state}`}
                  onClick={() => setSelectedClient(client.client_id)}
                  aria-label={`${account()?.username ?? "Client"} preview`}
                >
                  <div class="fleet-thumb" style={{ "--hue-a": account()?.color ?? "#6d7bff" }}>
                    <div class="fleet-thumb-fallback">
                      <Show when={account()} fallback={<div class="thumb-name">Unknown account</div>}>
                        {(acct) => (
                          <>
                            <Avatar account={acct()} size={42} presence={false} />
                            <span class="thumb-name">{acct().display_name || acct().username}</span>
                            <span class="thumb-sub">{client.target ? "In experience" : "Roblox"}</span>
                          </>
                        )}
                      </Show>
                    </div>
                    <div class="fleet-thumb-overlay">
                      <span class={`chip status ${client.state}`}>{client.state.replace(/_/g, " ")}</span>
                      <div class="grow" />
                      <Show when={client.hwnd} fallback={<span class="chip ghost">no window</span>}>
                        <span class="chip ghost"><Icon name="monitor" size={11} /> {formatNumber(client.pid)}</span>
                      </Show>
                    </div>
                    <div class="thumb-actions">
                      <button class="icon-btn glass" onClick={(e) => { e.stopPropagation(); focus(client); }} aria-label="Focus window" title="Focus window"><Icon name="external" size={14} /></button>
                      <button class="icon-btn glass" onClick={(e) => { e.stopPropagation(); screenshot(client); }} aria-label="Screenshot" title="Screenshot"><Icon name="camera" size={14} /></button>
                      <button class="icon-btn glass" onClick={(e) => { e.stopPropagation(); killClient(client.client_id); }} aria-label="Stop client" title="Stop client"><Icon name="kill" size={14} /></button>
                    </div>
                  </div>
                  <div class="fleet-meta">
                    <div class="row between">
                      <div class="fleet-name">{account()?.alias || account()?.username || client.account_id.slice(0, 8)}</div>
                      <span class={`health-pill ${health() >= 80 ? "ok" : health() >= 50 ? "warn" : "bad"}`}>{health()}</span>
                    </div>
                    <div class="fleet-stats">
                      <span title="CPU"><Icon name="cpu" size={12} /> {formatNumber(client.cpu_pct, 1)}%</span>
                      <span title="Memory"><Icon name="memory" size={12} /> {formatBytes(client.mem_mb)}</span>
                      <span title="Threads">{formatNumber(client.threads)} thr</span>
                      <span title="Handles">{formatNumber(client.handles)} h</span>
                    </div>
                    <div class="metric-bars">
                      <Meter value={client.mem_mb} max={Math.max(2048, client.mem_mb)} warnAt={1400} dangerAt={2000} />
                    </div>
                    <div class="row between tiny muted">
                      <span><Icon name="clock" size={11} /> {formatClock(client.started_at ? (Date.now() - new Date(client.started_at).getTime()) / 1000 : 0)}</span>
                      <span>{client.restarts} restarts</span>
                    </div>
                  </div>
                </article>
              );
            }}
          </For>
        </div>

        <Show when={selectedClient()}>
          {(id) => {
            const client = () => liveClients().find((c) => c.client_id === id());
            const account = () => (client() ? accountFor(client()!) : undefined);
            return (
              <div class="inspector">
                <div class="inspector-head">
                  <Show when={account()}>{(acct) => <Avatar account={acct()} size="sm" />}</Show>
                  <strong>{account()?.username ?? "Client"}</strong>
                  <span class={`chip status ${client()?.state}`}>{client()?.state.replace(/_/g, " ")}</span>
                  <div class="grow" />
                  <button class="btn sm ghost" onClick={() => setSelectedClient(null)}><Icon name="close" size={13} /></button>
                </div>
                <div class="inspector-grid">
                  <div><span class="muted tiny">PID</span><strong>{client()?.pid ?? "—"}</strong></div>
                  <div><span class="muted tiny">Window</span><strong>{client()?.hwnd ? `0x${client()!.hwnd!.toString(16)}` : "—"}</strong></div>
                  <div><span class="muted tiny">State since</span><strong>{timeAgo(client()?.state_since)}</strong></div>
                  <div><span class="muted tiny">Last reason</span><strong class="truncate">{client()?.last_reason || "—"}</strong></div>
                  <div><span class="muted tiny">Limits</span><strong>{account()?.limits?.cpu_percent ? `${account()!.limits!.cpu_percent}% CPU` : "profile"}</strong></div>
                  <div><span class="muted tiny">Lane</span><strong>{snapshot().fleet.lanes[client()?.lane_id ?? ""]?.name ?? "—"}</strong></div>
                </div>
                <div class="row" style={{ "margin-top": "10px" }}>
                  <button class="btn sm" onClick={() => client() && focus(client()!)}><Icon name="external" size={13} /> Focus</button>
                  <button class="btn sm" onClick={() => client() && screenshot(client()!)}><Icon name="camera" size={13} /> Capture</button>
                  <button class="btn sm" onClick={() => client() && api.validateAccounts([client()!.account_id])}><Icon name="shield" size={13} /> Validate</button>
                  <div class="grow" />
                  <button class="btn sm danger-ghost" onClick={() => client() && killClient(client()!.client_id)}><Icon name="kill" size={13} /> Stop</button>
                </div>
              </div>
            );
          }}
        </Show>
      </Show>

      <Show when={visible().length && snapshot().fleet.settings.performance.thumbnails === false}>
        <p class="hint"><Icon name="info" size={12} /> Thumbnails are disabled in Settings → Performance. Captures are paused while this view is hidden.</p>
      </Show>
    </div>
  );
}
