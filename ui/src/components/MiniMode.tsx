import { For, Show } from "solid-js";
import { Icon } from "./Icon";
import { Avatar, HoldButton } from "./Primitives";
import { accounts, killAll, liveClients, runningClients, snapshot, killClient } from "../lib/store";
import { invoke, isTauri } from "../lib/api";
import { formatBytes } from "../lib/format";

export function MiniMode() {
  const clientAccounts = () =>
    liveClients().map((client) => ({ client, account: accounts().find((a) => a.id === client.account_id) })).filter((entry) => entry.account);
  const openMain = async () => {
    if (isTauri()) await invoke("show_main").catch(() => undefined);
    else window.close();
  };
  const focusClient = async (clientId: string) => {
    if (isTauri()) await invoke("focus_client", { clientId }).catch(() => undefined);
  };
  return (
    <div class="mini">
      <div class="mini-head">
        <div class="brand-mark">R</div>
        <strong style={{ "font-size": "0.78rem" }}>RoFarm</strong>
        <div class="spacer" />
        <span class="dot in_game" />
        <span class="tiny muted">{runningClients().length} running</span>
        <button class="icon-btn" style={{ width: "24px", height: "24px" }} onClick={openMain} aria-label="Open Command Deck"><Icon name="external" size={13} /></button>
      </div>
      <div class="mini-row">
        <span class="muted">CPU</span>
        <strong>{snapshot().totals.cpu_pct.toFixed(0)}%</strong>
        <span class="muted">RAM</span>
        <strong>{formatBytes(snapshot().totals.mem_mb)}</strong>
        <div class="spacer" style={{ flex: 1 }} />
        <span class="muted">{Object.keys(snapshot().fleet.accounts).length} accts</span>
      </div>
      <div class="mini-clients">
        <For each={clientAccounts()}>
          {({ client, account }) => (
            <div class="mini-client">
              <span class={`dot ${client.state}`} />
              <span style={{ overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap", flex: 1 }}>@{account!.username}</span>
              <span class="muted tiny">{client.cpu_pct.toFixed(0)}%</span>
              <button class="icon-btn" style={{ width: "22px", height: "22px" }} onClick={() => focusClient(client.client_id)} aria-label={`Focus ${account!.username}`}><Icon name="monitor" size={12} /></button>
              <button class="icon-btn" style={{ width: "22px", height: "22px" }} onClick={() => killClient(client.client_id)} aria-label={`Stop ${account!.username}`}><Icon name="kill" size={12} /></button>
            </div>
          )}
        </For>
        <Show when={!clientAccounts().length}><div class="muted tiny" style={{ padding: "12px", "text-align": "center" }}>No clients running.</div></Show>
      </div>
      <div style={{ "margin-top": "auto", display: "flex", gap: "6px" }}>
        <HoldButton onConfirm={killAll} label="Kill all" icon="kill" disabled={!runningClients().length} class="sm" />
        <button class="btn sm grow" onClick={openMain}><Icon name="maximize" size={13} />Open deck</button>
      </div>
    </div>
  );
}
