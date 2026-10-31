import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { EmptyState } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { downloadText, formatDate } from "../lib/format";
import { api, invoke } from "../lib/api";
import { snapshot, toast, withBusy } from "../lib/store";
import type { DoctorReport } from "../lib/types";

type Tab = "rofarm" | "roblox" | "doctor";
type Level = "all" | "info" | "warn" | "error" | "debug";

interface LogLine { at: string; level: string; target: string; message: string }

const demoLines: LogLine[] = [
  { at: new Date(Date.now() - 40_000).toISOString(), level: "info", target: "rofarm::fleet", message: "snapshot served: 10 accounts, 6 clients (rev 1841)" },
  { at: new Date(Date.now() - 38_000).toISOString(), level: "info", target: "rofarm::win::job", message: "job object created for client 3f27 with cpu cap 25%" },
  { at: new Date(Date.now() - 31_000).toISOString(), level: "debug", target: "rofarm::roblox", message: "GET games.roblox.com/v1/games?universeIds=… 200 in 61 ms" },
  { at: new Date(Date.now() - 27_000).toISOString(), level: "warn", target: "rofarm::fleet", message: "account @TradeOracle validation deferred: upstream 429, backing off 120s" },
  { at: new Date(Date.now() - 19_000).toISOString(), level: "info", target: "rofarm::win::window", message: "SetWinEventHook registered for EVENT_OBJECT_SHOW/DESTROY" },
  { at: new Date(Date.now() - 12_000).toISOString(), level: "error", target: "rofarm::fleet", message: "client 8c11 exited with code 0xC0000005; crash breaker armed (1/3)" },
  { at: new Date(Date.now() - 8_000).toISOString(), level: "info", target: "rofarm::automation", message: "automation 'Crash guard' fired: restart 8c11 (dry_run=false)" },
  { at: new Date(Date.now() - 3_000).toISOString(), level: "info", target: "rofarm::fleet", message: "state diff sent: 1 client changed, 214 bytes" },
];

export default function Logs() {
  const [tab, setTab] = createSignal<Tab>("rofarm");
  const [lines, setLines] = createSignal<LogLine[]>(demoLines);
  const [level, setLevel] = createSignal<Level>("all");
  const [search, setSearch] = createSignal("");
  const [follow, setFollow] = createSignal(true);
  const [doctor, setDoctor] = createSignal<DoctorReport | null>(null);
  const [doctorBusy, setDoctorBusy] = createSignal(false);

  const visible = createMemo(() => {
    const needle = search().trim().toLowerCase();
    return lines()
      .filter((l) => level() === "all" || l.level === level())
      .filter((l) => !needle || `${l.target} ${l.message}`.toLowerCase().includes(needle))
      .slice(-500);
  });

  const load = async () => {
    try {
      const result: any = await invoke("logs_tail", { lines: 500 });
      if (Array.isArray(result)) setLines(result);
      else if (typeof result === "string") {
        setLines(result.split("\n").filter(Boolean).map((line) => {
          try { return JSON.parse(line); } catch { return { at: new Date().toISOString(), level: "info", target: "rofarm", message: line }; }
        }));
      }
    } catch {
    }
  };

  const runDoctor = async () => {
    setDoctorBusy(true);
    try {
      setDoctor(await api.doctor());
      toast("Doctor finished", "No changes were made.", "success");
    } catch (error: any) {
      toast("Doctor failed", String(error?.message ?? error), "error");
    } finally {
      setDoctorBusy(false);
    }
  };

  const cleanLogs = async () => {
    await withBusy("Cleaning Roblox logs", async () => {
      const result: any = await invoke("clean_logs", {});
      toast("Roblox logs cleaned", `${result?.removed ?? 0} files removed, ${result?.freed_mb ?? 0} MB freed.`, "success");
    });
  };

  const exportDiagnostics = async () => {
    await withBusy("Building diagnostics bundle", async () => {
      const result: any = await invoke("export_diagnostics", {});
      if (typeof result === "string" && result.startsWith("{")) downloadText("rofarm-diagnostics.json", result, "application/json");
      toast("Diagnostics exported", "Cookies, tokens, and account identifiers are redacted.", "success");
    });
  };

  onMount(() => { load(); runDoctor(); });

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Logs &amp; Doctor</h1>
          <p class="subtitle">Local logs, a self-check, and a redacted support bundle.</p>
        </div>
        <div class="row">
          <button class="btn ghost" onClick={() => invoke("open_logs", {}).then(() => toast("Opened logs folder", "", "success")).catch((e) => toast("Could not open logs", String(e), "error"))}><Icon name="folder" size={14} /> Open folder</button>
          <button class="btn" onClick={exportDiagnostics}><Icon name="download" size={14} /> Diagnostics bundle</button>
        </div>
      </div>

      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab() === "rofarm"} class={tab() === "rofarm" ? "active" : ""} onClick={() => setTab("rofarm")}><Icon name="logs" size={14} /> RoFarm log</button>
        <button role="tab" aria-selected={tab() === "roblox"} class={tab() === "roblox" ? "active" : ""} onClick={() => setTab("roblox")}><Icon name="file" size={14} /> Roblox logs</button>
        <button role="tab" aria-selected={tab() === "doctor"} class={tab() === "doctor" ? "active" : ""} onClick={() => setTab("doctor")}><Icon name="shield" size={14} /> Doctor <Show when={doctor()}>{(d) => <span class={`tab-count ${d().score > 85 ? "ok" : "warn"}`}>{d().score}</span>}</Show></button>
      </div>

      <Show when={tab() === "rofarm"}>
        <div class="toolbar">
          <div class="segmented sm">
            <For each={["all", "info", "warn", "error", "debug"] as Level[]}>{(l) => <button class={level() === l ? "active" : ""} onClick={() => setLevel(l)}>{l}</button>}</For>
          </div>
          <div class="search compact"><Icon name="search" size={14} /><input value={search()} onInput={(e) => setSearch(e.currentTarget.value)} placeholder="Filter target or message…" aria-label="Search logs" /></div>
          <div class="grow" />
          <label class="row tiny muted"><input type="checkbox" checked={follow()} onChange={(e) => setFollow(e.currentTarget.checked)} /> Follow tail</label>
          <button class="btn sm ghost" onClick={load}><Icon name="refresh" size={13} /> Reload</button>
          <button class="btn sm ghost" onClick={() => downloadText("rofarm.log.jsonl", visible().map((l) => JSON.stringify(l)).join("\n"), "application/x-ndjson")}><Icon name="download" size={13} /></button>
        </div>
        <div class="log-view" role="log" aria-live={follow() ? "polite" : "off"}>
          <For each={visible()} fallback={<div class="log-empty">No matching lines.</div>}>
            {(line) => (
              <div class={`log-line ${line.level}`}>
                <span class="log-time">{new Date(line.at).toLocaleTimeString([], { hour12: false })}</span>
                <span class={`log-level ${line.level}`}>{line.level.toUpperCase().padEnd(5)}</span>
                <span class="log-target">{line.target}</span>
                <span class="log-msg">{line.message}</span>
              </div>
            )}
          </For>
        </div>
        <p class="hint"><Icon name="shield" size={12} /> Logs never contain cookies, tokens, or full account identifiers. Structured JSONL on disk, rotated at 10 MB.</p>
      </Show>

      <Show when={tab() === "roblox"}>
        <div class="two-col">
          <section class="card">
            <div class="card-head"><h3>Client logs</h3><span class="muted tiny">read-only</span></div>
            <p class="muted small">RoFarm reads Roblox's own log folder to parse crash reasons, never modifies client memory. Old logs can be cleaned safely.</p>
            <div class="log-paths">
              <code>%LOCALAPPDATA%\Roblox\logs</code>
              <span class="muted tiny">Crash parser: exit code, disconnection reason, and last script error</span>
            </div>
            <div class="row">
              <button class="btn" onClick={cleanLogs}><Icon name="trash" size={14} /> Clean old logs</button>
              <button class="btn ghost" onClick={() => invoke("open_logs", {}).then(() => toast("Opened logs folder", "", "success"))}><Icon name="folder" size={14} /> Open folder</button>
            </div>
          </section>
          <section class="card">
            <div class="card-head"><h3>Recent crash reasons</h3></div>
            <Show when={Object.values(snapshot().fleet.crash_times).length} fallback={<p class="muted small">No crashes recorded for this workspace. That's a good thing.</p>}>
              <div class="crash-list">
                <For each={Object.entries(snapshot().fleet.crash_times).slice(0, 6)}>
                  {([accountId, times]) => (
                    <div class="crash-row">
                      <span>{snapshot().fleet.accounts[accountId]?.username ?? accountId.slice(0, 8)}</span>
                      <div class="grow" />
                      <span class="chip warn tiny">{times.length} in 24h</span>
                      <span class="tiny muted">{times[0] ? formatDate(times[0], true) : ""}</span>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </section>
        </div>
      </Show>

      <Show when={tab() === "doctor"}>
        <div class="toolbar">
          <Show when={doctor()}>
            {(report) => <div class="row"><span class={`doctor-score ${report().score > 85 ? "ok" : "warn"}`}>{report().score}</span><div><strong>Health score</strong><div class="tiny muted">checked {formatDate(report().at, true)}</div></div></div>}
          </Show>
          <div class="grow" />
          <button class="btn" onClick={runDoctor} disabled={doctorBusy()}><Icon name={doctorBusy() ? "rotate" : "shield"} size={14} /> {doctorBusy() ? "Checking…" : "Run check"}</button>
        </div>
        <Show when={doctor()} fallback={<EmptyState icon="shield" title="Doctor not run" body="Run a self-check to verify the WebView2 runtime, vault, data ACLs, Roblox installation, firewall rules, and update channel." />}>
          {(report) => (
            <div class="doctor-list">
              <For each={report().checks}>
                {(check) => (
                  <div class={`doctor-check ${check.status}`}>
                    <span class={`doctor-icon ${check.status}`}><Icon name={check.status === "pass" ? "check" : check.status === "warn" ? "warning" : "error"} size={14} /></span>
                    <div class="grow"><strong>{check.label}</strong><div class="tiny muted">{check.detail}</div></div>
                    <Show when={check.fix}><button class="btn sm ghost" onClick={() => toast("Suggested fix", check.fix, "info")}>How to fix</button></Show>
                  </div>
                )}
              </For>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}
