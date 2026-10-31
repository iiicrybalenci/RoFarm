import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { Avatar, EmptyState } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { invoke } from "../lib/api";
import { accounts, refresh, snapshot, toast, withBusy } from "../lib/store";
import type { Friend } from "../lib/types";

type Tab = "all" | "online" | "mutual" | "requests";

export default function Friends() {
  const [friends, setFriends] = createSignal<Friend[]>([]);
  const [tab, setTab] = createSignal<Tab>("all");
  const [search, setSearch] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [selected, setSelected] = createSignal<Set<number>>(new Set());
  const [requests, setRequests] = createSignal([
    { id: 9001, username: "TradeOracle", display_name: "TradeOracle", accounts: 2, at: new Date(Date.now() - 2 * 86_400_000).toISOString() },
    { id: 9002, username: "QuietFarmer", display_name: "Quiet Farmer", accounts: 1, at: new Date(Date.now() - 5 * 86_400_000).toISOString() },
  ]);

  const account = (id: string) => snapshot().fleet.accounts[id];
  const online = createMemo(() => friends().filter((f) => f.online).length);
  const mutuals = createMemo(() => friends().filter((f) => (f.accounts?.length ?? 0) > 1).length);
  const visible = createMemo(() => {
    const needle = search().trim().toLowerCase();
    return friends()
      .filter((f) => tab() === "all" || (tab() === "online" && f.online) || (tab() === "mutual" && (f.accounts?.length ?? 0) > 1))
      .filter((f) => !needle || f.username.toLowerCase().includes(needle) || f.display_name.toLowerCase().includes(needle))
      .sort((a, b) => Number(b.online) - Number(a.online) || a.username.localeCompare(b.username));
  });

  const load = async () => {
    setLoading(true);
    try {
      const rows = await invoke<Friend[]>("fetch_friends", { accountIds: accounts().map((a) => a.id) });
      setFriends(rows ?? []);
    } catch (error: any) {
      toast("Could not load friends", String(error?.message ?? error), "error");
    } finally {
      setLoading(false);
    }
  };

  onMount(load);

  const toggle = (id: number) => {
    setSelected((current) => {
      const next = new Set(current);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  const bulk = async (action: "follow" | "unfollow") => {
    const ids = [...selected()];
    if (!ids.length) return toast("Select friends first", "", "warning");
    await withBusy(action === "follow" ? "Following friends" : "Unfollowing friends", async () => {
      for (const id of ids) await invoke(action === "follow" ? "friend_follow" : "friend_unfollow", { friendId: id });
      setSelected(new Set<number>());
      toast("Done", `${ids.length} friend${ids.length === 1 ? "" : "s"} ${action === "follow" ? "followed" : "unfollowed"} from all accounts.`, "success");
    });
  };

  const handleRequest = async (id: number, accept: boolean) => {
    await withBusy(accept ? "Accepting request" : "Declining request", async () => {
      await invoke("friend_request", { requestId: id, accept });
      setRequests((current) => current.filter((r) => r.id !== id));
      toast(accept ? "Request accepted" : "Request declined", "", "success");
      await load();
    });
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Friends</h1>
          <p class="subtitle">Every friend across every account, with mutuals and bulk follow.</p>
        </div>
        <div class="row">
          <button class="btn ghost" onClick={load} disabled={loading()}><Icon name={loading() ? "rotate" : "refresh"} size={14} /> {loading() ? "Syncing…" : "Sync"}</button>
          <button class="btn primary" onClick={() => bulk("follow")} disabled={!selected().size}><Icon name="plus" size={14} /> Follow selected ({selected().size})</button>
        </div>
      </div>

      <div class="stat-strip">
        <div class="stat"><span class="stat-label">Known friends</span><strong>{friends().length}</strong><span class="stat-note">across {accounts().length} accounts</span></div>
        <div class="stat"><span class="stat-label">Online now</span><strong>{online()}</strong><span class="stat-note">presence refreshed live</span></div>
        <div class="stat"><span class="stat-label">Mutuals</span><strong>{mutuals()}</strong><span class="stat-note">on 2+ accounts</span></div>
        <div class="stat"><span class="stat-label">Pending requests</span><strong>{requests().length}</strong><span class="stat-note">waiting for you</span></div>
      </div>

      <div class="tabs" role="tablist">
        <For each={[["all", "All friends"], ["online", "Online"], ["mutual", "Mutuals"], ["requests", "Requests"]] as [Tab, string][]}>
          {([value, label]) => <button role="tab" aria-selected={tab() === value} class={tab() === value ? "active" : ""} onClick={() => setTab(value)}>{label}<Show when={value === "requests" && requests().length}><span class="tab-count">{requests().length}</span></Show></button>}
        </For>
      </div>

      <Show when={tab() !== "requests"}>
        <div class="toolbar">
          <div class="search compact"><Icon name="search" size={14} /><input value={search()} onInput={(e) => setSearch(e.currentTarget.value)} placeholder="Search usernames…" aria-label="Search friends" /></div>
          <div class="grow" />
          <button class="btn sm ghost" onClick={() => { setSelected(new Set(visible().map((f) => f.id))); }}>Select all</button>
          <button class="btn sm ghost" onClick={() => bulk("unfollow")} disabled={!selected().size}>Unfollow selected</button>
        </div>
        <Show when={visible().length} fallback={<EmptyState icon="friends" title="No friends to show" body="Sync friends from accounts you own. Mutuals show where several of your accounts share the same friend." action={<button class="btn" onClick={load}>Sync now</button>} />}>
          <div class="friend-grid">
            <For each={visible()}>
              {(friend) => (
                <article class={`friend-card ${selected().has(friend.id) ? "picked" : ""}`} onClick={() => toggle(friend.id)}>
                  <div class="row">
                    <span class="friend-presence"><span class={`dot ${friend.online ? "online" : ""}`} /><Avatar name={friend.display_name} size={38} presence={false} /></span>
                    <div class="grow">
                      <div class="row"><strong>{friend.display_name}</strong><Show when={friend.mutual || (friend.accounts?.length ?? 0) > 1}><span class="chip tiny accent">mutual</span></Show></div>
                      <span class="tiny muted">@{friend.username}</span>
                    </div>
                    <input type="checkbox" checked={selected().has(friend.id)} onChange={() => toggle(friend.id)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${friend.username}`} />
                  </div>
                  <Show when={friend.place}><div class="friend-place"><Icon name="play" size={11} /> {friend.place}</div></Show>
                  <div class="row tiny muted">
                    <span>{friend.accounts?.length ?? 1} of your accounts</span>
                    <div class="grow" />
                    <Show when={friend.online}><button class="btn sm ghost" onClick={(e) => { e.stopPropagation(); invoke("launch", { accounts: accounts().map((a) => a.id).slice(0, 1), target: "@" + friend.username }); toast("Follow launch queued", `Looking for ${friend.username}'s server…`, "success"); }}>Join</button></Show>
                  </div>
                  <div class="friend-accounts">
                    <For each={(friend.accounts ?? []).slice(0, 3)}>{(id) => <Show when={account(id)}>{(a) => <span class="mini-account" title={a().username}>{a().alias || a().username}</span>}</Show>}</For>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "requests"}>
        <Show when={requests().length} fallback={<EmptyState icon="check" title="No pending requests" body="Friend requests from all accounts land here once synced. Accepting or declining applies to the account that received it." />}>
          <div class="request-list">
            <For each={requests()}>
              {(request) => (
                <div class="request-row">
                  <Avatar name={request.display_name} size={34} presence={false} />
                  <div class="grow"><strong>{request.display_name}</strong><div class="tiny muted">@{request.username} · received by {request.accounts} account(s)</div></div>
                  <button class="btn sm primary" onClick={() => handleRequest(request.id, true)}><Icon name="check" size={12} /> Accept</button>
                  <button class="btn sm ghost" onClick={() => handleRequest(request.id, false)}>Decline</button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <p class="hint"><Icon name="shield" size={12} /> Friend actions use Roblox's public web API and respect rate limits. Bulk actions are staggered; nothing is automated inside a game.</p>
      <button class="hidden" onClick={() => refresh()}>refresh</button>
    </div>
  );
}
