import { For, Show, createMemo, createSignal } from "solid-js";
import { EmptyState, Sparkline } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { downloadText, formatDate, formatDuration, formatNumber } from "../lib/format";
import { accounts, snapshot, toast } from "../lib/store";
import type { SessionRecord } from "../lib/types";

type Range = "24h" | "7d" | "30d" | "all";

export default function Timeline() {
  const [range, setRange] = createSignal<Range>("7d");
  const [filter, setFilter] = createSignal("");
  const [outcome, setOutcome] = createSignal("all");

  const cutoff = createMemo(() => {
    const ms = { "24h": 86_400_000, "7d": 604_800_000, "30d": 2_592_000_000, all: Number.MAX_SAFE_INTEGER }[range()];
    return Date.now() - ms;
  });

  const sessions = createMemo(() =>
    [...snapshot().fleet.sessions]
      .filter((s) => new Date(s.started_at).getTime() >= cutoff())
      .filter((s) => outcome() === "all" || s.outcome === outcome())
      .filter((s) => {
        const needle = filter().trim().toLowerCase();
        if (!needle) return true;
        const account = snapshot().fleet.accounts[s.account_id];
        return [account?.username, account?.alias, s.target, s.outcome].join(" ").toLowerCase().includes(needle);
      })
      .sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime()),
  );

  const byDay = createMemo(() => {
    const groups = new Map<string, SessionRecord[]>();
    for (const session of sessions()) {
      const key = new Date(session.started_at).toDateString();
      const list = groups.get(key) ?? [];
      list.push(session);
      groups.set(key, list);
    }
    return [...groups.entries()];
  });

  const totalSeconds = createMemo(() => sessions().reduce((sum, s) => sum + s.seconds, 0));
  const crashes = createMemo(() => sessions().filter((s) => s.outcome === "crashed" || s.outcome === "failed").length);
  const longest = createMemo(() => sessions().reduce((max, s) => Math.max(max, s.seconds), 0));
  const uniqueAccounts = createMemo(() => new Set(sessions().map((s) => s.account_id)).size);

  const dailySeries = createMemo(() => {
    const days = Array.from({ length: 14 }).map((_, index) => {
      const date = new Date();
      date.setHours(0, 0, 0, 0);
      date.setDate(date.getDate() - (13 - index));
      const key = date.toDateString();
      const seconds = snapshot().fleet.sessions.filter((s) => new Date(s.started_at).toDateString() === key).reduce((sum, s) => sum + s.seconds, 0);
      return { date, seconds };
    });
    return days;
  });

  const exportCsv = () => {
    const rows = [["account", "started_at", "ended_at", "seconds", "outcome", "target"]];
    for (const session of sessions()) {
      const account = snapshot().fleet.accounts[session.account_id];
      rows.push([account?.username ?? session.account_id, session.started_at, session.ended_at, String(Math.round(session.seconds)), session.outcome, session.target ?? ""]);
    }
    downloadText(`rofarm-sessions-${new Date().toISOString().slice(0, 10)}.csv`, rows.map((r) => r.map((cell) => `"${cell.replace(/"/g, '""')}"`).join(",")).join("\n"), "text/csv");
    toast("Session history exported", `${sessions().length} rows written to CSV.`, "success");
  };

  const exportJson = () => {
    downloadText(`rofarm-sessions-${new Date().toISOString().slice(0, 10)}.json`, JSON.stringify(sessions(), null, 2), "application/json");
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Timeline</h1>
          <p class="subtitle">Every session, restart, and disconnect in one auditable stream.</p>
        </div>
        <div class="row">
          <div class="segmented" role="group" aria-label="Time range">
            <For each={["24h", "7d", "30d", "all"] as Range[]}>
              {(value) => <button class={range() === value ? "active" : ""} onClick={() => setRange(value)}>{value === "all" ? "All" : value}</button>}
            </For>
          </div>
          <button class="btn" onClick={exportCsv}><Icon name="download" size={14} /> Export CSV</button>
          <button class="btn ghost" onClick={exportJson} aria-label="Export JSON"><Icon name="file" size={14} /></button>
        </div>
      </div>

      <div class="stat-strip">
        <div class="stat"><span class="stat-label">Playtime</span><strong>{formatDuration(totalSeconds())}</strong><span class="stat-note">{sessions().length} sessions</span></div>
        <div class="stat"><span class="stat-label">Accounts seen</span><strong>{uniqueAccounts()}</strong><span class="stat-note">of {accounts().length}</span></div>
        <div class="stat"><span class="stat-label">Crashes</span><strong>{crashes()}</strong><span class="stat-note">{sessions().length ? ((crashes() / sessions().length) * 100).toFixed(1) : "0"}% of sessions</span></div>
        <div class="stat"><span class="stat-label">Longest session</span><strong>{formatDuration(longest())}</strong><span class="stat-note">per-session max</span></div>
        <div class="stat chart-stat">
          <span class="stat-label">Last 14 days</span>
          <Sparkline values={dailySeries().map((d) => d.seconds / 3600)} width={190} height={34} fill />
          <span class="stat-note">hours per day</span>
        </div>
      </div>

      <div class="toolbar">
        <div class="search compact">
          <Icon name="search" size={14} />
          <input value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} placeholder="Filter sessions by account, target, outcome…" aria-label="Filter sessions" />
        </div>
        <div class="segmented sm" role="group" aria-label="Outcome">
          <For each={["all", "clean", "crashed", "closed", "failed"]}>
            {(value) => <button class={outcome() === value ? "active" : ""} onClick={() => setOutcome(value)}>{value}</button>}
          </For>
        </div>
        <div class="grow" />
        <span class="muted tiny">{sessions().length} shown</span>
      </div>

      <Show when={byDay().length} fallback={<EmptyState icon="timeline" title="Nothing in this window" body="Sessions appear as soon as a client launches. Adjust the range or start a client from the Board." />}>
        <div class="timeline">
          <For each={byDay()}>
            {([day, items]) => (
              <section class="timeline-day">
                <header>
                  <span class="timeline-dot" />
                  <h3>{day}</h3>
                  <span class="muted small">{items.length} session{items.length === 1 ? "" : "s"}</span>
                  <div class="grow" />
                  <span class="muted small">{formatDuration(items.reduce((sum, s) => sum + s.seconds, 0))}</span>
                </header>
                <div class="timeline-items">
                  <For each={items}>
                    {(session) => {
                      const account = () => snapshot().fleet.accounts[session.account_id];
                      const bad = () => ["crashed", "failed", "killed"].includes(session.outcome);
                      return (
                        <div class="timeline-row">
                          <span class={`timeline-marker ${bad() ? "bad" : session.outcome === "clean" ? "ok" : ""}`} />
                          <div class="timeline-time">
                            <strong>{new Date(session.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</strong>
                            <span class="tiny muted">{formatDate(session.started_at)}</span>
                          </div>
                          <div class="timeline-main">
                            <div class="row"><strong>{account()?.alias || account()?.username || "Deleted account"}</strong><span class="chip tiny ghost">{session.outcome}</span></div>
                            <span class="tiny muted truncate">{session.target || "No target recorded"}</span>
                          </div>
                          <div class="timeline-duration">{formatDuration(session.seconds)}</div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </section>
            )}
          </For>
        </div>
      </Show>

      <p class="hint"><Icon name="shield" size={12} /> Session history is stored locally in your vault workspace. Exporting includes account usernames but never cookies or tokens.</p>
      <span class="hidden">{formatNumber(dailySeries().length)}</span>
    </div>
  );
}
