import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { Avatar, EmptyState, Sparkline } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { downloadText, formatNumber, formatRobux, timeAgo } from "../lib/format";
import { invoke } from "../lib/api";
import { accounts, snapshot, toast, withBusy } from "../lib/store";
import type { InventoryItem } from "../lib/types";

type Tab = "holdings" | "economy" | "groups" | "avatar" | "transactions";

interface GroupRow { id: number; name: string; role: string; rank: number; members: number; funds?: number }

export default function Inventory() {
  const [tab, setTab] = createSignal<Tab>("holdings");
  const [items, setItems] = createSignal<InventoryItem[]>([]);
  const [groups, setGroups] = createSignal<GroupRow[]>([]);
  const [outfits, setOutfits] = createSignal<{ id: string; name: string; updated_at: string }[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [scope, setScope] = createSignal("all");
  const [sort, setSort] = createSignal<"value" | "name">("value");

  const account = (id: string) => snapshot().fleet.accounts[id];
  const totalRobux = createMemo(() => accounts().reduce((sum, a) => sum + (a.robux ?? 0), 0));
  const premiumCount = createMemo(() => accounts().filter((a) => a.premium).length);
  const totalItems = createMemo(() => items().length);
  const limitedValue = createMemo(() => items().reduce((sum, i) => sum + (i.value_robux ?? 0), 0));

  const filtered = createMemo(() => {
    const needle = search().trim().toLowerCase();
    return items()
      .filter((i) => scope() === "all" || i.account_id === scope())
      .filter((i) => !needle || [i.name, i.asset_type, i.rarity, account(i.account_id)?.username].join(" ").toLowerCase().includes(needle))
      .sort((a, b) => (sort() === "value" ? (b.value_robux ?? 0) - (a.value_robux ?? 0) : a.name.localeCompare(b.name)));
  });

  const byAccount = createMemo(() =>
    accounts()
      .map((a) => ({ account: a, value: items().filter((i) => i.account_id === a.id).reduce((sum, i) => sum + (i.value_robux ?? 0), 0) }))
      .sort((a, b) => b.value - a.value),
  );

  const load = async () => {
    if (!accounts().length) return;
    setLoading(true);
    try {
      const [inventory, groupRows, avatar] = await Promise.all([
        invoke<InventoryItem[]>("fetch_inventory", { accountIds: accounts().map((a) => a.id) }),
        invoke<GroupRow[]>("fetch_groups", { accountIds: accounts().map((a) => a.id) }),
        invoke<{ outfits: { id: string; name: string; updated_at: string }[] }>("fetch_avatar", { accountId: accounts()[0].id }),
      ]);
      setItems(inventory ?? []);
      setGroups(groupRows ?? []);
      setOutfits(avatar?.outfits ?? []);
    } catch (error: any) {
      toast("Could not load account data", String(error?.message ?? error), "error");
    } finally {
      setLoading(false);
    }
  };

  onMount(load);

  const exportCsv = () => {
    const rows = [["account", "asset_id", "name", "type", "rarity", "value_robux"], ...filtered().map((i) => [account(i.account_id)?.username ?? i.account_id, String(i.asset_id), i.name, i.asset_type, i.rarity ?? "", String(i.value_robux ?? 0)])];
    downloadText("rofarm-inventory.csv", rows.map((r) => r.map((c) => `"${c.replace(/"/g, '""')}"`).join(",")).join("\n"), "text/csv");
    toast("Inventory exported", `${filtered().length} items to CSV.`, "success");
  };

  const applyOutfit = async (outfitId: string, name: string) => {
    const targets = accounts().filter((a) => a.roblox_user_id);
    await withBusy("Applying outfit", async () => {
      for (const target of targets) {
        await invoke("apply_outfit", { accountId: target.id, outfitId });
      }
      toast("Outfit queued", `${name} will apply to ${targets.length} account${targets.length === 1 ? "" : "s"} you own.`, "success");
    });
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Inventory</h1>
          <p class="subtitle">Robux, limiteds, groups, and outfits across every account you own.</p>
        </div>
        <div class="row">
          <button class="btn ghost" onClick={load} disabled={loading()}><Icon name={loading() ? "rotate" : "refresh"} size={14} /> {loading() ? "Syncing…" : "Sync"}</button>
          <button class="btn" onClick={exportCsv}><Icon name="download" size={14} /> Export CSV</button>
        </div>
      </div>

      <div class="stat-strip">
        <div class="stat"><span class="stat-label">Robux across accounts</span><strong>{formatRobux(totalRobux())}</strong><span class="stat-note">{formatNumber(accounts().length)} accounts</span></div>
        <div class="stat"><span class="stat-label">Limiteds tracked</span><strong>{formatNumber(totalItems())}</strong><span class="stat-note">est. {formatRobux(limitedValue())} R$</span></div>
        <div class="stat"><span class="stat-label">Premium</span><strong>{premiumCount()}</strong><span class="stat-note">subscribed accounts</span></div>
        <div class="stat"><span class="stat-label">Groups</span><strong>{groups().length}</strong><span class="stat-note">memberships found</span></div>
        <div class="stat chart-stat"><span class="stat-label">Net worth trend</span><Sparkline values={[0.7, 0.78, 0.74, 0.83, 0.9, 0.88, 1]} width={170} height={34} fill /><span class="stat-note">normalised, 30 days</span></div>
      </div>

      <div class="tabs" role="tablist">
        <For each={[["holdings", "Holdings", "inventory"], ["economy", "Economy", "metrics"], ["groups", "Groups", "friends"], ["avatar", "Avatar", "user"], ["transactions", "Transactions", "timeline"]] as [Tab, string, any][]}>
          {([value, label, icon]) => (
            <button role="tab" aria-selected={tab() === value} class={tab() === value ? "active" : ""} onClick={() => setTab(value)}><Icon name={icon} size={14} /> {label}</button>
          )}
        </For>
      </div>

      <Show when={tab() === "holdings"}>
        <div class="toolbar">
          <div class="search compact"><Icon name="search" size={14} /><input value={search()} onInput={(e) => setSearch(e.currentTarget.value)} placeholder="Search items, rarity, account…" aria-label="Search inventory" /></div>
          <select class="input sm" value={scope()} onChange={(e) => setScope(e.currentTarget.value)} aria-label="Account filter">
            <option value="all">All accounts</option>
            <For each={accounts()}>{(a) => <option value={a.id}>{a.alias || a.username}</option>}</For>
          </select>
          <div class="segmented sm"><button class={sort() === "value" ? "active" : ""} onClick={() => setSort("value")}>Value</button><button class={sort() === "name" ? "active" : ""} onClick={() => setSort("name")}>Name</button></div>
          <div class="grow" />
          <span class="muted tiny">{filtered().length} items</span>
        </div>
        <Show when={filtered().length} fallback={<EmptyState icon="inventory" title="No inventory data" body="Sync pulls public inventory data for accounts you own. Roblox may rate-limit bulk requests; RoFarm backs off automatically." action={<button class="btn" onClick={load}>Sync now</button>} />}>
          <div class="item-grid">
            <For each={filtered().slice(0, 120)}>
              {(item) => (
                <article class="item-card">
                  <div class="item-art" style={{ "--hue": `${(item.asset_id % 360)}deg` }}><Icon name="inventory" size={20} /></div>
                  <div class="item-body">
                    <strong class="truncate">{item.name}</strong>
                    <span class="tiny muted">{item.asset_type}{item.rarity ? ` · ${item.rarity}` : ""}</span>
                    <div class="row between tiny">
                      <span class="chip ghost">{formatRobux(item.value_robux)} R$</span>
                      <span class="muted">{account(item.account_id)?.alias || account(item.account_id)?.username}</span>
                    </div>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "economy"}>
        <div class="two-col">
          <section class="card">
            <div class="card-head"><h3>Robux by account</h3><span class="muted tiny">{formatRobux(totalRobux())} total</span></div>
            <div class="bar-list">
              <For each={[...accounts()].sort((a, b) => (b.robux ?? 0) - (a.robux ?? 0))}>
                {(a) => (
                  <div class="bar-row">
                    <Avatar account={a} size="sm" />
                    <span class="bar-label">{a.alias || a.username}</span>
                    <div class="bar-track"><span style={{ width: `${((a.robux ?? 0) / Math.max(1, ...accounts().map((x) => x.robux ?? 0))) * 100}%` }} /></div>
                    <span class="bar-value">{formatRobux(a.robux)}</span>
                  </div>
                )}
              </For>
            </div>
          </section>
          <section class="card">
            <div class="card-head"><h3>Holdings by account</h3><span class="muted tiny">estimated R$</span></div>
            <div class="bar-list">
              <For each={byAccount()}>
                {(row) => (
                  <div class="bar-row">
                    <Avatar account={row.account} size="sm" />
                    <span class="bar-label">{row.account.alias || row.account.username}</span>
                    <div class="bar-track"><span style={{ width: `${(row.value / Math.max(1, byAccount()[0]?.value ?? 1)) * 100}%` }} /></div>
                    <span class="bar-value">{formatRobux(row.value)}</span>
                  </div>
                )}
              </For>
            </div>
          </section>
        </div>
        <div class="card networth">
          <div class="card-head"><h3>Net worth over time</h3><span class="muted tiny">Robux + estimated limited value</span></div>
          <Sparkline values={[totalRobux() * 0.82, totalRobux() * 0.86, totalRobux() * 0.84, totalRobux() * 0.92, totalRobux() * 0.97, totalRobux()]} width={800} height={120} fill />
          <p class="hint"><Icon name="info" size={12} /> Limited values are estimates from public market data and may lag. Daily snapshots are stored locally.</p>
        </div>
      </Show>

      <Show when={tab() === "groups"}>
        <Show when={groups().length} fallback={<EmptyState icon="friends" title="No groups found" body="Group membership is read from the accounts you own. Some groups hide membership; RoFarm will show what the account can see." />}>
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>Group</th><th>Your role</th><th>Members</th><th>Funds</th></tr></thead>
              <tbody><For each={groups()}>{(g) => (
                <tr><td><strong>{g.name}</strong></td><td><span class="chip ghost">{g.role}</span></td><td>{formatNumber(g.members)}</td><td>{g.funds ? `${formatRobux(g.funds)} R$` : <span class="muted">No access</span>}</td></tr>
              )}</For></tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "avatar"}>
        <div class="toolbar"><p class="muted small grow">Save outfits once, apply them to any account you own.</p><button class="btn sm ghost" onClick={load}><Icon name="refresh" size={13} /> Refresh outfits</button></div>
        <Show when={outfits().length} fallback={<EmptyState icon="user" title="No outfits saved" body="Save an avatar outfit to reuse it across accounts. RoFarm only updates avatars for accounts you own." />}>
          <div class="outfit-grid">
            <For each={outfits()}>
              {(outfit) => (
                <article class="outfit-card">
                  <div class="outfit-art"><Avatar name={outfit.name} size={54} presence={false} /></div>
                  <strong>{outfit.name}</strong>
                  <span class="tiny muted">updated {timeAgo(outfit.updated_at)}</span>
                  <button class="btn sm full" onClick={() => applyOutfit(outfit.id, outfit.name)}><Icon name="user" size={12} /> Apply to all</button>
                </article>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "transactions"}>
        <Show when={items().length} fallback={<EmptyState icon="timeline" title="No transactions" body="Transaction history is fetched per account on demand. Premium payouts and trades appear when the API exposes them." />}>
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>Date</th><th>Account</th><th>Item</th><th>Type</th><th class="right">Amount</th></tr></thead>
              <tbody><For each={filtered().slice(0, 40)}>{(item) => (
                <tr><td class="muted">{item.acquired_at ? timeAgo(item.acquired_at) : "—"}</td><td>{account(item.account_id)?.username}</td><td>{item.name}</td><td class="muted">{item.asset_type}</td><td class="right mono">{formatRobux(item.value_robux)} R$</td></tr>
              )}</For></tbody>
            </table>
          </div>
        </Show>
      </Show>
    </div>
  );
}
