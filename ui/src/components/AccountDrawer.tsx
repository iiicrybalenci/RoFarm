import { For, Show, createMemo, createSignal } from "solid-js";
import { Drawer } from "./Primitives";
import { Avatar } from "./Primitives";
import { Icon } from "./Icon";
import {
  accountStatus,
  accounts,
  closeAccount,
  focusedAccount,
  killClient,
  lanes,
  liveForAccount,
  moveAccount,
  openEditAccount,
  openLaunch,
  openReauth,
  refresh,
  snapshot,
  toast,
  undo,
} from "../lib/store";
import { api, invoke, openExternal } from "../lib/api";
import { formatDuration, formatRobux, timeAgo } from "../lib/format";

export function AccountDrawer() {
  return (
    <Show when={focusedAccount()}>
      {(account) => <DrawerContent />}
    </Show>
  );
}

function DrawerContent() {
  const account = focusedAccount()!;
  const live = createMemo(() => liveForAccount(account.id));
  const [tab, setTab] = createSignal<"overview" | "notes" | "activity">("overview");
  const [notes, setNotes] = createSignal(account.notes);
  const [lane, setLane] = createSignal(account.lane_id);
  const [confirmRemove, setConfirmRemove] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const status = createMemo(() => accountStatus(account));
  const sessions = createMemo(() => snapshot().fleet.sessions.filter((s) => s.account_id === account.id).slice(-8).reverse());

  const saveNotes = async () => {
    setSaving(true);
    try {
      await api.updateAccount(account.id, { notes: notes() });
      toast("Notes saved", "", "success");
      await refresh();
    } catch (error: any) {
      toast("Could not save notes", error.message, "error");
    } finally {
      setSaving(false);
    }
  };

  const validate = async () => {
    try {
      await api.validateAccounts([account.id]);
      await refresh();
      toast("Account validated", `@${account.username} is signed in.`, "success");
    } catch (error: any) {
      toast("Validation failed", error.message, "error");
    }
  };

  const remove = async () => {
    try {
      await api.removeAccount(account.id);
      closeAccount();
      await refresh();
      toast("Account removed", `@${account.username} was removed. Use Undo to bring it back.`, "warning");
    } catch (error: any) {
      toast("Could not remove account", error.message, "error");
    }
  };

  const changeLane = async (next: string) => {
    setLane(next);
    await moveAccount(account.id, next);
  };

  const favourite = async () => {
    await api.updateAccount(account.id, { favourite: !account.favourite });
    await refresh();
  };

  const profileUrl = account.roblox_user_id ? `https://www.roblox.com/users/${account.roblox_user_id}/profile` : `https://www.roblox.com/search/users?keyword=${encodeURIComponent(account.username)}`;

  return (
    <Drawer onClose={closeAccount} label={`Account ${account.username}`}>
      <div class="drawer-head">
        <Avatar account={account} size="lg" />
        <div class="grow" style={{ "min-width": 0 }}>
          <h2 style={{ display: "flex", "align-items": "center", gap: "6px" }}>
            {account.display_name}
            <Show when={account.premium}><span class="pill warn" style={{ "font-size": "0.58rem" }}>PREMIUM</span></Show>
          </h2>
          <div class="muted" style={{ "font-size": "0.75rem" }}>@{account.username} {account.alias ? `· ${account.alias}` : ""}</div>
          <div class="row" style={{ "margin-top": "5px", gap: "5px" }}>
            <span class={`pill ${status().tone}`}><span class={`dot ${status().tone}`} />{status().label}</span>
            <Show when={account.favourite}><span class="pill warn">Favourite</span></Show>
          </div>
        </div>
        <button class="icon-btn" onClick={() => void favourite()} aria-label="Toggle favourite" title="Favourite" style={account.favourite ? { color: "var(--warn)" } : undefined}>
          <Icon name="heart" size={16} />
        </button>
        <button class="icon-btn" onClick={closeAccount} aria-label="Close"><Icon name="close" size={16} /></button>
      </div>
      <div class="drawer-body">
        <div class="row" style={{ gap: "5px" }}>
          <button class="btn primary grow" onClick={() => openLaunch([account.id])}><Icon name="play" size={13} />Launch</button>
          <Show when={live()} fallback={<button class="btn" onClick={validate}><Icon name="check" size={13} />Validate</button>}>
            {(client) => (
              <>
                <button class="btn" onClick={() => invoke("focus_client", { clientId: client().client_id })}><Icon name="monitor" size={13} />Focus</button>
                <button class="btn danger" onClick={() => killClient(client().client_id)}><Icon name="kill" size={13} />Stop</button>
              </>
            )}
          </Show>
          <button class="icon-btn" onClick={() => openEditAccount(account.id)} aria-label="Edit account" title="Edit account"><Icon name="edit" size={15} /></button>
        </div>
        <div class="tabs" style={{ padding: 0, "margin-top": "-2px" }}>
          <button classList={{ active: tab() === "overview" }} onClick={() => setTab("overview")}>Overview</button>
          <button classList={{ active: tab() === "notes" }} onClick={() => setTab("notes")}>Notes</button>
          <button classList={{ active: tab() === "activity" }} onClick={() => setTab("activity")}>Activity</button>
        </div>
        <Show when={tab() === "overview"}>
          <div class="row" style={{ gap: "8px" }}>
            <div class="section grow" style={{ "min-width": "120px" }}>
              <div class="tiny muted">Robux</div>
              <div style={{ "font-size": "1.25rem", "font-weight": 650, color: "var(--warn)" }}>{formatRobux(account.robux)}</div>
            </div>
            <div class="section grow" style={{ "min-width": "120px" }}>
              <div class="tiny muted">Health</div>
              <div style={{ "font-size": "1.25rem", "font-weight": 650, color: account.health.score >= 80 ? "var(--ok)" : account.health.score >= 60 ? "var(--warn)" : "var(--danger)" }}>{account.health.score}</div>
            </div>
          </div>
          <Show when={live()}>
            {(client) => (
              <div class="section">
                <h3>Live client</h3>
                <div class="kv" style={{ "margin-top": "8px" }}>
                  <dt>State</dt><dd><span class={`pill ${client().state}`}>{client().state.replaceAll("_", " ")}</span></dd>
                  <dt>PID</dt><dd class="mono">{client().pid ?? "—"}</dd>
                  <dt>CPU</dt><dd>{client().cpu_pct.toFixed(1)}%</dd>
                  <dt>Memory</dt><dd>{client().mem_mb} MB</dd>
                  <dt>Session</dt><dd>{client().started_at ? formatDuration((Date.now() - new Date(client().started_at!).getTime()) / 1000) : "—"}</dd>
                  <dt>Restarts</dt><dd>{client().restarts}</dd>
                  <dt>Last reason</dt><dd>{client().last_reason ?? "—"}</dd>
                </div>
              </div>
            )}
          </Show>
          <div class="section">
            <h3>Account</h3>
            <div class="kv" style={{ "margin-top": "8px" }}>
              <dt>Roblox ID</dt><dd class="mono">{account.roblox_user_id ?? "unknown"}</dd>
              <dt>Friends</dt><dd>{account.friend_count}</dd>
              <dt>Followers</dt><dd>{account.follower_count}</dd>
              <dt>Account age</dt><dd>{account.account_age_days ? `${account.account_age_days} days` : "—"}</dd>
              <dt>2-step</dt><dd>{account.two_step ? "enabled" : "not reported"}</dd>
              <dt>Last validated</dt><dd>{timeAgo(account.last_validated)}</dd>
              <dt>Cookie expires</dt><dd>{account.cookie_expires_at ? timeAgo(account.cookie_expires_at) : "unknown"}</dd>
              <dt>Login</dt><dd><span class={`pill ${["valid"].includes(account.login.kind) ? "ok" : "warn"}`}>{account.login.kind}</span></dd>
              <dt>Lane</dt><dd>
                <select class="select" style={{ padding: "3px 6px", width: "auto" }} value={lane()} onChange={(event) => void changeLane(event.currentTarget.value)}>
                  <For each={lanes()}>{(item) => <option value={item.id}>{item.name}</option>}</For>
                </select>
              </dd>
            </div>
            <Show when={account.health.issues.length}>
              <div class="stack" style={{ "margin-top": "10px" }}>
                <For each={account.health.issues}>
                  {(issue) => <div class="banner"><Icon name="warning" size={13} /><span>{issue.label} {issue.fix ? `— ${issue.fix}` : ""}</span></div>}
                </For>
              </div>
            </Show>
            <Show when={account.tags.length}>
              <div class="card-tags" style={{ "margin-top": "9px" }}>
                <For each={account.tags}>{(tag) => <span class="tag">{tag}</span>}</For>
              </div>
            </Show>
          </div>
          <div class="section">
            <h3>Vault</h3>
            <p class="section-desc">Reveal and copy always ask for your passphrase again. Cookies are excluded from screenshots.</p>
            <div class="row">
              <button class="btn sm" onClick={() => openReauth("reveal", account.id)}><Icon name="eye" size={13} />Reveal</button>
              <button class="btn sm" onClick={() => openReauth("copy", account.id)}><Icon name="copy" size={13} />Copy</button>
              <span class="grow" />
              <span class="tiny muted mono">{account.cookie_fingerprint ?? "no cookie"}</span>
            </div>
          </div>
          <button class="btn ghost" onClick={() => openExternal(profileUrl)}><Icon name="external" size={13} />Open profile on Roblox</button>
        </Show>
        <Show when={tab() === "notes"}>
          <div class="field">
            <label class="field-label" for="drawer-notes">Private notes</label>
            <textarea id="drawer-notes" class="textarea" style={{ "min-height": "180px", "font-family": "var(--font)", "font-size": "0.78rem" }} value={notes()} onInput={(event) => setNotes(event.currentTarget.value)} />
          </div>
          <div class="row">
            <button class="btn primary" disabled={saving() || notes() === account.notes} onClick={saveNotes}>{saving() ? "Saving…" : "Save notes"}</button>
            <span class="muted tiny">Markdown is kept as plain text. Notes are stored in the event log and backups.</span>
          </div>
          <div class="section">
            <h3>Custom fields</h3>
            <Show when={Object.keys(account.custom_fields).length} fallback={<p class="section-desc" style={{ margin: 0 }}>No custom fields yet. Add them from Edit account.</p>}>
              <div class="kv">
                <For each={Object.entries(account.custom_fields)}>{([key, value]) => <><dt>{key}</dt><dd>{value}</dd></>}</For>
              </div>
            </Show>
          </div>
        </Show>
        <Show when={tab() === "activity"}>
          <div class="section">
            <h3>Recent sessions</h3>
            <Show when={sessions().length} fallback={<p class="section-desc" style={{ margin: 0 }}>No sessions recorded yet.</p>}>
              <div class="stack" style={{ "margin-top": "8px" }}>
                <For each={sessions()}>
                  {(session) => (
                    <div class="row" style={{ gap: "8px", "font-size": "0.74rem", "align-items": "flex-start" }}>
                      <span class={`dot ${session.outcome.startsWith("Crashed") ? "failed" : session.outcome.includes("Disconnected") ? "launching" : "in_game"}`} style={{ "margin-top": "5px" }} />
                      <div class="grow">
                        <div>{formatDuration(session.seconds)} · {session.outcome}</div>
                        <div class="tiny muted">{timeAgo(session.started_at)} · {session.target ?? "unknown target"}</div>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
          <div class="section">
            <h3>Playtime</h3>
            <div class="kv" style={{ "margin-top": "6px" }}>
              <dt>Total</dt><dd>{formatDuration(account.playtime_seconds)}</dd>
              <dt>Sessions</dt><dd>{account.sessions}</dd>
              <dt>This week</dt><dd>{formatDuration(sessions().reduce((sum, s) => sum + s.seconds, 0))}</dd>
            </div>
          </div>
        </Show>
        <div class="divider" />
        <Show when={confirmRemove()} fallback={
          <button class="btn danger" onClick={() => setConfirmRemove(true)}><Icon name="trash" size={13} />Remove account</button>
        }>
          <div class="banner danger">
            <Icon name="warning" size={15} />
            <div class="grow">
              <strong>Remove @{account.username}?</strong>
              <div class="tiny" style={{ "margin": "3px 0 7px" }}>This deletes the account and crypto-shreds its cookie. Undo restores the record, but a shredded cookie cannot be recovered.</div>
              <div class="row">
                <button class="btn sm danger" onClick={remove}>Remove permanently</button>
                <button class="btn sm" onClick={() => setConfirmRemove(false)}>Cancel</button>
                <button class="btn sm ghost" onClick={() => { setConfirmRemove(false); void undo(); }}>Undo instead</button>
              </div>
            </div>
          </div>
        </Show>
      </div>
    </Drawer>
  );
}
