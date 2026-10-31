import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { Avatar, EmptyState, Modal } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { formatNumber, timeAgo } from "../lib/format";
import { api, invoke } from "../lib/api";
import { accounts, refresh, selectedAccounts, snapshot, toast, withBusy } from "../lib/store";
import type { GameEntry, ServerInstance } from "../lib/types";

type Tab = "discover" | "private" | "library";

export default function Servers() {
  const [tab, setTab] = createSignal<Tab>("discover");
  const [placeInput, setPlaceInput] = createSignal("920587237");
  const [loading, setLoading] = createSignal(false);
  const [servers, setServers] = createSignal<ServerInstance[]>([]);
  const [sort, setSort] = createSignal<"players" | "ping">("players");
  const [region, setRegion] = createSignal("all");
  const [picked, setPicked] = createSignal<string | null>(null);
  const [launchOpen, setLaunchOpen] = createSignal(false);
  const [pick, setPick] = createSignal<string[]>([]);
  const [gameOpen, setGameOpen] = createSignal(false);
  const [privateOpen, setPrivateOpen] = createSignal(false);
  const [form, setForm] = createSignal({ name: "", place_id: "", link: "", notes: "" });
  const [gameForm, setGameForm] = createSignal({ name: "", place_id: "", notes: "" });

  const games = createMemo(() => Object.values(snapshot().fleet.games));
  const privateServers = createMemo(() => Object.values(snapshot().fleet.private_servers));
  const regions = createMemo(() => [...new Set(servers().map((s) => s.region ?? "Unknown"))]);

  const filtered = createMemo(() => {
    const list = servers().filter((s) => region() === "all" || s.region === region());
    return list.sort((a, b) => (sort() === "ping" ? (a.ping ?? 999) - (b.ping ?? 999) : a.playing - b.playing));
  });

  const search = async () => {
    const placeId = Number(placeInput().replace(/\D/g, ""));
    if (!placeId) return toast("Enter a place ID", "Paste a game link or numeric place ID.", "warning");
    setLoading(true);
    try {
      const result = await api.listServers(placeId);
      setServers(result as ServerInstance[]);
      toast("Server list refreshed", `${result.length} public servers found.`, "success");
    } catch (error: any) {
      toast("Could not load servers", String(error?.message ?? error), "error");
    } finally {
      setLoading(false);
    }
  };

  onMount(() => { search(); });

  const openLaunchPicker = (jobId: string | null = picked()) => {
    setPicked(jobId);
    setPick(selectedAccounts().map((a) => a.id));
    setLaunchOpen(true);
  };

  const launchInto = async (accountsToUse: string[], jobId?: string) => {
    const placeId = Number(placeInput().replace(/\D/g, ""));
    if (!accountsToUse.length) return toast("Pick at least one account", "", "warning");
    const target = jobId ? `https://www.roblox.com/games/${placeId}?jobId=${jobId}` : `https://www.roblox.com/games/${placeId}`;
    await withBusy("Launching into server", async () => {
      await api.launch(accountsToUse, target);
      setLaunchOpen(false);
      setPicked(null);
      await refresh();
      toast("Launch queued", `${accountsToUse.length} account${accountsToUse.length === 1 ? "" : "s"} heading to the server.`, "success");
    });
  };

  const saveGame = async () => {
    if (!gameForm().name.trim() || !Number(gameForm().place_id)) return toast("Game needs a name and place ID", "", "warning");
    await withBusy("Saving game", async () => {
      await invoke("save_game", {
        game: {
          name: gameForm().name,
          place_id: Number(gameForm().place_id),
          notes: gameForm().notes,
          tags: [],
          favourite: false,
        },
      });
      setGameOpen(false);
      setGameForm({ name: "", place_id: "", notes: "" });
      await refresh();
      toast("Game saved", "Set a default launch profile from its card.", "success");
    });
  };

  const savePrivate = async () => {
    if (!form().name.trim() || !form().link.trim()) return toast("Add a name and link", "", "warning");
    await withBusy("Saving private server", async () => {
      await invoke("save_private_server", {
        server: {
          name: form().name,
          place_id: Number(form().place_id.replace(/\D/g, "")) || 0,
          link: form().link,
          notes: form().notes,
        },
      });
      setPrivateOpen(false);
      setForm({ name: "", place_id: "", link: "", notes: "" });
      await refresh();
      toast("Private server saved", "", "success");
    });
  };

  const launchGame = (game: GameEntry) => launchInto(selectedAccounts().map((a) => a.id).length ? selectedAccounts().map((a) => a.id) : accounts().slice(0, 1).map((a) => a.id), undefined);

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Servers</h1>
          <p class="subtitle">Find a quiet server, reuse private links, or open a saved game.</p>
        </div>
        <div class="row">
          <button class="btn ghost" onClick={() => setPrivateOpen(true)}><Icon name="link" size={14} /> Add private server</button>
          <button class="btn" onClick={() => setGameOpen(true)}><Icon name="plus" size={14} /> Save game</button>
        </div>
      </div>

      <div class="tabs" role="tablist" aria-label="Server tools">
        <button role="tab" aria-selected={tab() === "discover"} class={tab() === "discover" ? "active" : ""} onClick={() => setTab("discover")}><Icon name="globe" size={14} /> Discover</button>
        <button role="tab" aria-selected={tab() === "private"} class={tab() === "private" ? "active" : ""} onClick={() => setTab("private")}><Icon name="lock" size={14} /> Private servers <span class="tab-count">{privateServers().length}</span></button>
        <button role="tab" aria-selected={tab() === "library"} class={tab() === "library" ? "active" : ""} onClick={() => setTab("library")}><Icon name="database" size={14} /> Game library <span class="tab-count">{games().length}</span></button>
      </div>

      <Show when={tab() === "discover"}>
        <div class="toolbar card">
          <div class="search grow">
            <Icon name="search" size={14} />
            <input value={placeInput()} onInput={(e) => setPlaceInput(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && search()} placeholder="Game link or place ID, e.g. 920587237" aria-label="Place ID" />
          </div>
          <button class="btn" onClick={search} disabled={loading()}><Icon name={loading() ? "rotate" : "refresh"} size={14} /> {loading() ? "Scanning…" : "Find servers"}</button>
        </div>

        <div class="toolbar">
          <span class="muted small">{filtered().length} public servers</span>
          <div class="segmented sm" role="group" aria-label="Sort servers">
            <button class={sort() === "players" ? "active" : ""} onClick={() => setSort("players")}>Fewest players</button>
            <button class={sort() === "ping" ? "active" : ""} onClick={() => setSort("ping")}>Best ping</button>
          </div>
          <select class="input sm" value={region()} onChange={(e) => setRegion(e.currentTarget.value)} aria-label="Region">
            <option value="all">All regions</option>
            <For each={regions()}>{(r) => <option value={r}>{r}</option>}</For>
          </select>
          <div class="grow" />
          <span class="muted tiny"><Icon name="info" size={11} /> Ping is measured from this PC.</span>
        </div>

        <Show when={filtered().length} fallback={<EmptyState icon="servers" title="No servers to show" body="Paste a game link above to scan its public servers by population, ping, and region." />}>
          <div class="server-grid">
            <For each={filtered()}>
              {(server) => (
                <article class={`server-card ${picked() === server.job_id ? "picked" : ""}`} onClick={() => setPicked(server.job_id)}>
                  <div class="row between">
                    <span class="chip ghost mono">{(server.job_id ?? "").slice(0, 8)}</span>
                    <span class={`chip ${server.ping && server.ping < 60 ? "ok" : server.ping && server.ping > 140 ? "warn" : "ghost"}`}>{server.ping ?? "—"} ms</span>
                  </div>
                  <div class="server-pop">
                    <strong>{formatNumber(server.playing)}</strong>
                    <span class="muted">/ {server.max_players} players</span>
                  </div>
                  <div class="players-bar"><span style={{ width: `${(server.playing / Math.max(1, server.max_players)) * 100}%` }} /></div>
                  <div class="row between tiny muted">
                    <span><Icon name="globe" size={11} /> {server.region ?? "Unknown"}</span>
                    <span>{server.fps ?? "—"} FPS</span>
                  </div>
                  <button class="btn sm full" onClick={(e) => { e.stopPropagation(); openLaunchPicker(server.job_id); }} disabled={!accounts().length}>
                    <Icon name="play" size={12} /> Send accounts here
                  </button>
                </article>
              )}
            </For>
          </div>
          <Show when={picked()}>
            <div class="selection-bar">
              <Icon name="check" size={14} /> Server <span class="mono">{picked()!.slice(0, 12)}</span> selected
              <div class="grow" />
              <button class="btn sm" onClick={() => openLaunchPicker()}>Launch into this server</button>
            </div>
          </Show>
        </Show>
      </Show>

      <Show when={tab() === "private"}>
        <Show when={privateServers().length} fallback={<EmptyState icon="lock" title="No private servers yet" body="Store private-server links once and reuse them for any account. Expiry reminders appear in the notification center." action={<button class="btn" onClick={() => setPrivateOpen(true)}>Add your first link</button>} />}>
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>Name</th><th>Place</th><th>Owner</th><th>Expires</th><th /></tr></thead>
              <tbody>
                <For each={privateServers()}>
                  {(server) => (
                    <tr>
                      <td><strong>{server.name}</strong><div class="tiny muted">{server.notes}</div></td>
                      <td class="mono">{server.place_id}</td>
                      <td>{snapshot().fleet.accounts[server.owner_account_id ?? ""]?.username ?? "—"}</td>
                      <td><Show when={server.expires_at} fallback={<span class="muted">Never</span>}>{timeAgo(server.expires_at)}</Show></td>
                      <td class="row-actions">
                        <button class="btn sm" onClick={() => launchInto(selectedAccounts().length ? selectedAccounts().map((a) => a.id) : accounts().slice(0, 1).map((a) => a.id), server.link_code || undefined)}><Icon name="play" size={12} /> Launch</button>
                        <button class="icon-btn" title="Copy link" onClick={() => navigator.clipboard.writeText(server.link).then(() => toast("Link copied", "", "success"))}><Icon name="copy" size={13} /></button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "library"}>
        <Show when={games().length} fallback={<EmptyState icon="database" title="Your game library is empty" body="Save the games you farm most. Each entry remembers notes, tags, a default launch profile, and playtime." action={<button class="btn" onClick={() => setGameOpen(true)}>Save a game</button>} />}>
          <div class="game-grid">
            <For each={games()}>
              {(game) => (
                <article class="game-card">
                  <div class="game-art" style={{ "--hue": `${(game.place_id % 360)}deg` }}>
                    <span class="game-place mono">{game.place_id}</span>
                    <Show when={game.favourite}><span class="game-fav"><Icon name="heart" size={13} /></span></Show>
                  </div>
                  <div class="game-body">
                    <strong>{game.name}</strong>
                    <span class="tiny muted truncate">{game.notes || "No notes yet"}</span>
                    <div class="row tiny muted"><span><Icon name="clock" size={11} /> {formatNumber(game.playtime_seconds / 3600, 1)} h</span><span>·</span><span>{game.tags.join(", ") || "untagged"}</span></div>
                    <div class="row">
                      <button class="btn sm" onClick={() => launchGame(game)}><Icon name="play" size={12} /> Launch</button>
                      <button class="btn sm ghost" onClick={() => { setPlaceInput(String(game.place_id)); setTab("discover"); search(); }}>Servers</button>
                    </div>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={launchOpen()}>
        <Modal title="Launch into server" icon="play" onClose={() => setLaunchOpen(false)} narrow footer={<>
          <button class="btn ghost" onClick={() => setLaunchOpen(false)}>Cancel</button>
          <button class="btn primary" onClick={() => launchInto(pick(), picked() ?? undefined)}>Launch {pick().length || ""} account{pick().length === 1 ? "" : "s"}</button>
        </>}>
          <p class="muted small">Pick accounts. Launch is staggered using each lane's cooldown.</p>
          <AccountPicker value={pick()} onChange={setPick} />
        </Modal>
      </Show>

      <Show when={gameOpen()}>
        <Modal title="Save a game" icon="database" onClose={() => setGameOpen(false)} narrow footer={<><button class="btn ghost" onClick={() => setGameOpen(false)}>Cancel</button><button class="btn primary" onClick={saveGame}>Save game</button></>}>
          <label class="field"><span>Name</span><input class="input" value={gameForm().name} onInput={(e) => setGameForm({ ...gameForm(), name: e.currentTarget.value })} placeholder="Blox Fruits" /></label>
          <label class="field"><span>Place ID or link</span><input class="input mono" value={gameForm().place_id} onInput={(e) => setGameForm({ ...gameForm(), place_id: e.currentTarget.value })} placeholder="2753915549" /></label>
          <label class="field"><span>Notes</span><textarea class="input" rows="3" value={gameForm().notes} onInput={(e) => setGameForm({ ...gameForm(), notes: e.currentTarget.value })} placeholder="Server hop rules, level cap, trade schedule…" /></label>
        </Modal>
      </Show>

      <Show when={privateOpen()}>
        <Modal title="Add private server" icon="link" onClose={() => setPrivateOpen(false)} narrow footer={<><button class="btn ghost" onClick={() => setPrivateOpen(false)}>Cancel</button><button class="btn primary" onClick={savePrivate}>Save link</button></>}>
          <label class="field"><span>Name</span><input class="input" value={form().name} onInput={(e) => setForm({ ...form(), name: e.currentTarget.value })} placeholder="Trade hub" /></label>
          <label class="field"><span>Place ID</span><input class="input mono" value={form().place_id} onInput={(e) => setForm({ ...form(), place_id: e.currentTarget.value })} placeholder="920587237" /></label>
          <label class="field"><span>Private server link</span><input class="input" value={form().link} onInput={(e) => setForm({ ...form(), link: e.currentTarget.value })} placeholder="https://www.roblox.com/games/…?privateServerLinkCode=…" /></label>
          <label class="field"><span>Notes</span><input class="input" value={form().notes} onInput={(e) => setForm({ ...form(), notes: e.currentTarget.value })} placeholder="Owner, billing date…" /></label>
          <p class="hint"><Icon name="shield" size={12} /> Links are stored in your encrypted vault. RoFarm checks access only with the account you choose.</p>
        </Modal>
      </Show>
    </div>
  );
}

function AccountPicker(props: { value: string[]; onChange: (next: string[]) => void }) {
  const toggle = (id: string) => {
    props.onChange(props.value.includes(id) ? props.value.filter((x) => x !== id) : [...props.value, id]);
  };
  return (
    <div class="picker-list">
      <For each={accounts()}>
        {(account) => (
          <label class="picker-row">
            <input type="checkbox" checked={props.value.includes(account.id)} onChange={() => toggle(account.id)} />
            <Avatar account={account} size="sm" />
            <span>{account.alias || account.username}</span>
            <span class="muted tiny">{snapshot().fleet.lanes[account.lane_id]?.name}</span>
          </label>
        )}
      </For>
    </div>
  );
}
