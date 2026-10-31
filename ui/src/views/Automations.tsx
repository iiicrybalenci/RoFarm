import { For, Show, createMemo, createSignal } from "solid-js";
import { EmptyState, Modal, Switch } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { downloadText, formatDate, timeAgo } from "../lib/format";
import { invoke } from "../lib/api";
import { accounts, closeSceneDialog, openSceneDialog, refresh, sceneDialog, snapshot, toast, withBusy } from "../lib/store";
import type { Automation, Scene } from "../lib/types";

type Tab = "automations" | "scenes" | "history" | "console";

const TRIGGERS = [
  { value: "manual", label: "Manual / run now" },
  { value: "client_crashed", label: "Client crashed" },
  { value: "client_disconnected", label: "Client disconnected" },
  { value: "client_frozen", label: "Client frozen" },
  { value: "memory_above", label: "Memory above threshold" },
  { value: "time_of_day", label: "Time of day (cron)" },
  { value: "account_validation_failed", label: "Account validation failed" },
  { value: "friend_online", label: "Friend comes online" },
  { value: "battery_low", label: "Battery low" },
  { value: "session_locked", label: "Windows session locked" },
  { value: "session_unlocked", label: "Windows session unlocked" },
  { value: "roblox_version_changed", label: "New Roblox version" },
  { value: "webhook", label: "Incoming webhook" },
];

const ACTIONS = [
  { value: "launch", label: "Launch account" },
  { value: "restart", label: "Restart client" },
  { value: "rejoin", label: "Rejoin server" },
  { value: "move_lane", label: "Move account to lane" },
  { value: "notify", label: "Send notification" },
  { value: "kill", label: "Stop client" },
  { value: "kill_all", label: "Stop the whole fleet" },
  { value: "lock_vault", label: "Lock the vault" },
  { value: "switch_profile", label: "Switch resource profile" },
  { value: "set_eco_mode", label: "Set Eco mode" },
  { value: "set_anti_afk", label: "Set anti-AFK" },
  { value: "start_scene", label: "Start a scene" },
  { value: "create_backup", label: "Create encrypted backup" },
];

function blankAutomation(): Automation {
  return {
    id: crypto.randomUUID(),
    name: "",
    description: "",
    enabled: true,
    trigger: { kind: "manual" },
    conditions: [],
    actions: [{ kind: "notify", channel: "native", message: "Hello from RoFarm", severity: 1 }],
    cooldown_seconds: 60,
    daily_budget: 50,
    dry_run: false,
    schedule: null,
    last_run: null,
    run_count: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    stop_on_error: true,
  };
}

export default function Automations() {
  const [tab, setTab] = createSignal<Tab>("automations");
  const [editor, setEditor] = createSignal<Automation | null>(null);
  const [yamlOpen, setYamlOpen] = createSignal(false);
  const [yaml, setYaml] = createSignal("");
  const [consoleInput, setConsoleInput] = createSignal("count(accounts)");
  const [consoleOut, setConsoleOut] = createSignal<string[]>(["RoFarm script console — Rhai sandbox. Type help() for API surface."]);

  const automations = createMemo(() => Object.values(snapshot().fleet.automations));
  const runs = createMemo(() => snapshot().fleet.automation_runs as any[]);
  const scenes = createMemo(() => Object.values(snapshot().fleet.scenes));
  const enabledCount = createMemo(() => automations().filter((a) => a.enabled).length);

  const save = async (automation: Automation) => {
    if (!automation.name.trim()) return toast("Name the automation", "", "warning");
    if (!automation.actions.length) return toast("Add at least one action", "", "warning");
    await withBusy("Saving automation", async () => {
      await invoke("save_automation", { automation: { ...automation, updated_at: new Date().toISOString() } });
      setEditor(null);
      await refresh();
      toast("Automation saved", automation.name, "success");
    });
  };

  const toggle = async (automation: Automation, enabled: boolean) => {
    await save({ ...automation, enabled });
  };

  const run = async (automation: Automation, dryRun = false) => {
    await withBusy(dryRun ? "Dry-running automation" : "Running automation", async () => {
      const result: any = await invoke("run_automation", { id: automation.id, dryRun });
      await refresh();
      toast(dryRun ? "Dry run finished" : "Automation executed", `${automation.name}: ${result?.status ?? "done"}`, dryRun ? "info" : "success");
    });
  };

  const remove = async (automation: Automation) => {
    await withBusy("Deleting automation", async () => {
      await invoke("delete_automation", { id: automation.id });
      await refresh();
      toast("Automation deleted", automation.name, "warning");
    });
  };

  const openYaml = () => {
    setYaml(automations().map((a) => `# ${a.name}\nid: ${a.id}\nenabled: ${a.enabled}\n`).join("\n"));
    setYamlOpen(true);
  };

  const importYaml = async () => {
    await withBusy("Importing automations", async () => {
      const result: any = await invoke("import_automation_yaml", { yaml: yaml() });
      setYamlOpen(false);
      await refresh();
      toast("Import finished", `${result?.imported ?? 0} automation(s) imported.`, "success");
    });
  };

  const evalConsole = async () => {
    const source = consoleInput();
    if (!source.trim()) return;
    setConsoleOut((prev) => [...prev, `> ${source}`]);
    try {
      const result: any = await invoke("eval_console", { source });
      setConsoleOut((prev) => [...prev, typeof result === "string" ? result : JSON.stringify(result, null, 2)]);
    } catch (error: any) {
      setConsoleOut((prev) => [...prev, `error: ${error?.message ?? error}`]);
    }
  };

  const saveScene = async (name: string) => {
    await withBusy("Capturing scene", async () => {
      const scene = await invoke<Scene>("capture_scene", { name });
      await invoke("save_scene", { scene });
      closeSceneDialog();
      await refresh();
      toast("Scene saved", `${name} — accounts, targets, layouts, and limits captured.`, "success");
    });
  };

  const applyScene = async (scene: Scene) => {
    await withBusy("Applying scene", async () => {
      await invoke("apply_scene", { id: scene.id });
      await refresh();
      toast("Scene applied", scene.name, "success");
    });
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Automations</h1>
          <p class="subtitle">Triggers, conditions, and actions. Dry-run first; everything is local.</p>
        </div>
        <div class="row">
          <button class="btn ghost" onClick={openYaml}><Icon name="code" size={14} /> YAML</button>
          <button class="btn" onClick={() => setEditor(blankAutomation())}><Icon name="plus" size={14} /> New automation</button>
        </div>
      </div>

      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab() === "automations"} class={tab() === "automations" ? "active" : ""} onClick={() => setTab("automations")}><Icon name="automation" size={14} /> Automations <span class="tab-count">{enabledCount()}/{automations().length}</span></button>
        <button role="tab" aria-selected={tab() === "scenes"} class={tab() === "scenes" ? "active" : ""} onClick={() => setTab("scenes")}><Icon name="fleet" size={14} /> Scenes <span class="tab-count">{scenes().length}</span></button>
        <button role="tab" aria-selected={tab() === "history"} class={tab() === "history" ? "active" : ""} onClick={() => setTab("history")}><Icon name="timeline" size={14} /> Run history</button>
        <button role="tab" aria-selected={tab() === "console"} class={tab() === "console" ? "active" : ""} onClick={() => setTab("console")}><Icon name="terminal" size={14} /> Script console</button>
      </div>

      <Show when={tab() === "automations"}>
        <Show when={automations().length} fallback={<EmptyState icon="automation" title="No automations yet" body="Create a rule like “when a client crashes, restart it twice, then notify me on Discord.” Start simple and dry-run it." action={<button class="btn" onClick={() => setEditor(blankAutomation())}>Create an automation</button>} />}>
          <div class="automation-list">
            <For each={automations()}>
              {(automation) => (
                <article class={`automation-card ${automation.enabled ? "" : "disabled"}`}>
                  <div class="automation-icon"><Icon name="automation" size={18} /></div>
                  <div class="automation-body">
                    <div class="row">
                      <strong>{automation.name}</strong>
                      <Show when={automation.dry_run}><span class="chip warn tiny">dry run</span></Show>
                      <Show when={automation.trigger?.kind === "time_of_day"}><span class="chip ghost tiny">schedule</span></Show>
                    </div>
                    <div class="automation-flow">
                      <span class="flow-node trigger"><Icon name="zap" size={11} /> {TRIGGERS.find((t) => t.value === automation.trigger?.kind)?.label ?? String(automation.trigger?.kind ?? "manual")}</span>
                      <Icon name="chevron" size={12} />
                      <span class="flow-node">{automation.actions.length} action{automation.actions.length === 1 ? "" : "s"}</span>
                      <Show when={automation.conditions?.length}><span class="flow-node">{automation.conditions.length} condition{automation.conditions.length === 1 ? "" : "s"}</span></Show>
                      <span class="muted tiny">{automation.run_count} runs · last {timeAgo(automation.last_run)}</span>
                    </div>
                  </div>
                  <div class="automation-actions">
                    <button class="btn sm ghost" onClick={() => run(automation, true)} title="Dry run"><Icon name="eye" size={13} /></button>
                    <button class="btn sm" onClick={() => run(automation)}><Icon name="play" size={12} /> Run</button>
                    <button class="icon-btn" onClick={() => setEditor(structuredClone(automation))} title="Edit" aria-label="Edit automation"><Icon name="edit" size={14} /></button>
                    <button class="icon-btn" onClick={() => remove(automation)} title="Delete" aria-label="Delete automation"><Icon name="trash" size={14} /></button>
                    <Switch checked={automation.enabled} onChange={(next) => toggle(automation, next)} label={`Enable ${automation.name}`} />
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
        <p class="hint"><Icon name="shield" size={12} /> Automations run in-process. Shell scripts and URL actions require explicit confirmation on first use. Nothing is uploaded.</p>
      </Show>

      <Show when={tab() === "scenes"}>
        <div class="toolbar">
          <p class="muted small grow">Scenes capture the live fleet: accounts, targets, lanes, window layouts, and limits.</p>
          <button class="btn" onClick={openSceneDialog}><Icon name="camera" size={14} /> Capture current fleet</button>
        </div>
        <Show when={scenes().length} fallback={<EmptyState icon="fleet" title="No scenes captured" body="Save your current setup once, then start a whole fleet with a single click or hotkey." action={<button class="btn" onClick={openSceneDialog}>Capture a scene</button>} />}>
          <div class="scene-grid">
            <For each={scenes()}>
              {(scene) => (
                <article class="scene-card">
                  <div class="scene-top">
                    <div class="scene-icon"><Icon name="fleet" size={20} /></div>
                    <div>
                      <strong>{scene.name}</strong>
                      <div class="tiny muted">{scene.description || "Fleet snapshot"} · {formatDate(scene.created_at)}</div>
                    </div>
                  </div>
                  <Show when={scene.hotkey}><span class="chip ghost tiny"><Icon name="keyboard" size={11} /> {scene.hotkey}</span></Show>
                  <div class="row">
                    <button class="btn sm primary" onClick={() => applyScene(scene)}><Icon name="play" size={12} /> Start scene</button>
                    <button class="btn sm ghost" onClick={() => downloadText(`${scene.name}.rofarm-scene.json`, JSON.stringify(scene, null, 2), "application/json")}><Icon name="download" size={12} /></button>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
        <Show when={sceneDialog()}>
          <Modal title="Capture scene" icon="camera" onClose={closeSceneDialog} narrow footer={null}>
            <SceneForm onSave={saveScene} />
          </Modal>
        </Show>
      </Show>

      <Show when={tab() === "history"}>
        <Show when={runs().length} fallback={<EmptyState icon="timeline" title="No runs yet" body="Run an automation manually or wait for its trigger. Every run keeps a step-by-step log." />}>
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>When</th><th>Automation</th><th>Trigger</th><th>Status</th><th>Steps</th></tr></thead>
              <tbody>
                <For each={runs()}>
                  {(run) => (
                    <tr>
                      <td class="muted">{formatDate(run.at, true)}</td>
                      <td><strong>{run.automation_name}</strong></td>
                      <td class="muted">{run.trigger_summary}</td>
                      <td><span class={`chip ${run.status === "succeeded" ? "ok" : run.status === "failed" ? "bad" : "ghost"}`}>{String(run.status).replace(/_/g, " ")}</span></td>
                      <td class="muted">{run.steps?.length ?? 0}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "console"}>
        <div class="console card">
          <div class="console-out" role="log" aria-live="polite">
            <For each={consoleOut()}>{(line) => <pre class={line.startsWith(">") ? "cmd" : line.startsWith("error") ? "err" : ""}>{line}</pre>}</For>
          </div>
          <div class="console-input">
            <span class="prompt">›</span>
            <input class="input mono" value={consoleInput()} onInput={(e) => setConsoleInput(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && evalConsole()} placeholder="count(accounts) / launch('Farming') / help()" aria-label="Script console" />
            <button class="btn sm" onClick={evalConsole}><Icon name="play" size={12} /> Run</button>
          </div>
        </div>
        <p class="hint"><Icon name="shield" size={12} /> Sandboxed Rhai with no file, network, or process access unless the automation was granted it. Scripts can only call RoFarm's own API.</p>
      </Show>

      <Show when={editor()}>
        {(automation) => (
          <AutomationEditor
            initial={automation()}
            onClose={() => setEditor(null)}
            onSave={save}
          />
        )}
      </Show>

      <Show when={yamlOpen()}>
        <Modal title="Automations as YAML" icon="code" wide onClose={() => setYamlOpen(false)} footer={<><button class="btn ghost" onClick={() => setYamlOpen(false)}>Cancel</button><button class="btn primary" onClick={importYaml}>Import</button></>}>
          <p class="muted small">Edit or paste automations. Unknown keys are rejected; dangerous actions need confirmation after import.</p>
          <textarea class="input mono code-area" rows="16" value={yaml()} onInput={(e) => setYaml(e.currentTarget.value)} />
        </Modal>
      </Show>
    </div>
  );
}

function AutomationEditor(props: { initial: Automation; onClose: () => void; onSave: (automation: Automation) => void }) {
  const [draft, setDraft] = createSignal<Automation>(structuredClone(props.initial));
  const triggerKind = () => String((draft().trigger as any)?.kind ?? "manual");
  const patch = (fn: (next: Automation) => void) => {
    const next = structuredClone(draft());
    fn(next);
    setDraft(next);
  };
  const setTrigger = (kind: string) => {
    const base: Record<string, any> = { kind };
    if (kind === "time_of_day") base.cron = "0 20 * * *";
    if (kind === "memory_above") base.megabytes = 1500;
    if (kind === "battery_low") base.percent = 20;
    patch((next) => { next.trigger = base; });
  };
  const addAction = () => patch((next) => { next.actions.push({ kind: "notify", channel: "native", message: "", severity: 1 }); });
  const updateAction = (index: number, value: Record<string, any>) => patch((next) => { next.actions[index] = value; });
  const removeAction = (index: number) => patch((next) => { next.actions.splice(index, 1); });
  const addCondition = () => patch((next) => { next.conditions.push({ kind: "on_battery" }); });

  return (
    <Modal
      title={props.initial.name ? "Edit automation" : "New automation"}
      icon="automation"
      wide
      onClose={props.onClose}
      footer={<><button class="btn ghost" onClick={props.onClose}>Cancel</button><button class="btn primary" onClick={() => props.onSave(draft())}><Icon name="save" size={14} /> Save automation</button></>}
    >
      <div class="form-grid two">
        <label class="field"><span>Name</span><input class="input" value={draft().name} onInput={(e) => patch((n) => { n.name = e.currentTarget.value; })} placeholder="Restart crashed farmers" /></label>
        <label class="field"><span>Cooldown (seconds)</span><input class="input" type="number" min="5" value={draft().cooldown_seconds} onInput={(e) => patch((n) => { n.cooldown_seconds = Number(e.currentTarget.value); })} /></label>
      </div>
      <label class="field"><span>Description</span><input class="input" value={draft().description} onInput={(e) => patch((n) => { n.description = e.currentTarget.value; })} placeholder="What this rule protects against" /></label>

      <div class="editor-block">
        <div class="editor-block-head"><span class="step-badge">1</span><strong>Trigger</strong></div>
        <select class="input" value={triggerKind()} onChange={(e) => setTrigger(e.currentTarget.value)}>
          <For each={TRIGGERS}>{(t) => <option value={t.value}>{t.label}</option>}</For>
        </select>
        <Show when={triggerKind() === "time_of_day"}>
          <input class="input mono" value={(draft().trigger as any).cron ?? ""} onInput={(e) => patch((n) => { (n.trigger as any).cron = e.currentTarget.value; })} placeholder="0 20 * * *" aria-label="Cron expression" />
        </Show>
        <Show when={triggerKind() === "memory_above"}>
          <input class="input" type="number" value={(draft().trigger as any).megabytes ?? 1500} onInput={(e) => patch((n) => { (n.trigger as any).megabytes = Number(e.currentTarget.value); })} placeholder="Memory MB" aria-label="Memory threshold" />
        </Show>
        <Show when={triggerKind() === "battery_low"}>
          <input class="input" type="number" value={(draft().trigger as any).percent ?? 20} onInput={(e) => patch((n) => { (n.trigger as any).percent = Number(e.currentTarget.value); })} aria-label="Battery percent" />
        </Show>
      </div>

      <div class="editor-block">
        <div class="editor-block-head"><span class="step-badge">2</span><strong>Conditions</strong><div class="grow" /><button class="btn sm ghost" onClick={addCondition}><Icon name="plus" size={12} /> Condition</button></div>
        <Show when={draft().conditions.length} fallback={<p class="muted small">No conditions — the trigger alone is enough. Add one to scope by lane, battery, time window, or health.</p>}>
          <For each={draft().conditions}>
            {(condition, index) => (
              <div class="condition-row">
                <select class="input sm" value={String(condition.kind)} onChange={(e) => patch((n) => { n.conditions[index()] = { kind: e.currentTarget.value }; })}>
                  <option value="on_battery">On battery</option>
                  <option value="battery_below">Battery below</option>
                  <option value="account_health_below">Account health below</option>
                  <option value="time_window">Time window</option>
                  <option value="script">Script expression</option>
                </select>
                <Show when={condition.kind === "battery_below"}>
                  <input class="input sm" type="number" value={condition.percent ?? 20} onInput={(e) => patch((n) => { n.conditions[index()] = { ...condition, percent: Number(e.currentTarget.value) }; })} aria-label="Percent" />
                </Show>
                <Show when={condition.kind === "account_health_below"}>
                  <input class="input sm" type="number" value={condition.score ?? 50} onInput={(e) => patch((n) => { n.conditions[index()] = { ...condition, score: Number(e.currentTarget.value) }; })} aria-label="Health score" />
                </Show>
                <Show when={condition.kind === "script"}>
                  <input class="input sm mono" value={condition.rhai ?? ""} onInput={(e) => patch((n) => { n.conditions[index()] = { ...condition, rhai: e.currentTarget.value }; })} placeholder="battery_percent() < 30" aria-label="Condition script" />
                </Show>
                <div class="grow" />
                <button class="icon-btn" onClick={() => patch((n) => { n.conditions.splice(index(), 1); })} aria-label="Remove condition"><Icon name="close" size={13} /></button>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class="editor-block">
        <div class="editor-block-head"><span class="step-badge">3</span><strong>Actions</strong><div class="grow" /><button class="btn sm ghost" onClick={addAction}><Icon name="plus" size={12} /> Action</button></div>
        <For each={draft().actions}>
          {(action, index) => (
            <div class="action-row">
              <select class="input sm" value={String(action.kind)} onChange={(e) => updateAction(index(), { kind: e.currentTarget.value })}>
                <For each={ACTIONS}>{(a) => <option value={a.value}>{a.label}</option>}</For>
              </select>
              <Show when={action.kind === "notify"}>
                <select class="input sm" value={action.channel ?? "native"} onChange={(e) => updateAction(index(), { ...action, channel: e.currentTarget.value, message: action.message ?? "", severity: action.severity ?? 1 })}>
                  <For each={["native", "discord", "telegram", "slack", "ntfy", "pushover", "email"]}>{(c) => <option value={c}>{c}</option>}</For>
                </select>
                <input class="input sm grow" value={action.message ?? ""} onInput={(e) => updateAction(index(), { ...action, message: e.currentTarget.value })} placeholder="Message" aria-label="Notification message" />
              </Show>
              <Show when={["launch", "restart", "rejoin", "kill"].includes(String(action.kind))}>
                <select class="input sm" value={action.account ?? ""} onChange={(e) => updateAction(index(), { ...action, account: e.currentTarget.value || null })}>
                  <option value="">All running</option>
                  <For each={accounts()}>{(a) => <option value={a.id}>{a.alias || a.username}</option>}</For>
                </select>
              </Show>
              <Show when={action.kind === "move_lane"}>
                <select class="input sm" value={action.lane ?? ""} onChange={(e) => updateAction(index(), { ...action, lane: e.currentTarget.value })}>
                  <option value="">Choose lane…</option>
                  <For each={Object.values(snapshot().fleet.lanes)}>{(lane) => <option value={lane.id}>{lane.name}</option>}</For>
                </select>
              </Show>
              <Show when={action.kind === "switch_profile"}>
                <select class="input sm" value={action.profile ?? "balanced"} onChange={(e) => updateAction(index(), { ...action, profile: e.currentTarget.value })}>
                  <For each={["eco", "balanced", "performance", "custom"]}>{(p) => <option value={p}>{p}</option>}</For>
                </select>
              </Show>
              <Show when={action.kind === "set_eco_mode" || action.kind === "set_anti_afk"}>
                <label class="row tiny muted"><input type="checkbox" checked={!!action.enabled} onChange={(e) => updateAction(index(), { ...action, enabled: e.currentTarget.checked })} /> enabled</label>
              </Show>
              <div class="grow" />
              <button class="icon-btn" onClick={() => removeAction(index())} aria-label="Remove action"><Icon name="close" size={13} /></button>
            </div>
          )}
        </For>
      </div>

      <div class="form-grid two">
        <label class="field"><span>Daily budget</span><input class="input" type="number" min="1" value={draft().daily_budget} onInput={(e) => patch((n) => { n.daily_budget = Number(e.currentTarget.value); })} /></label>
        <div class="field"><span>Options</span><div class="row" style={{ "flex-wrap": "wrap" }}>
          <label class="row tiny"><input type="checkbox" checked={draft().dry_run} onChange={(e) => patch((n) => { n.dry_run = e.currentTarget.checked; })} /> Dry run</label>
          <label class="row tiny"><input type="checkbox" checked={draft().stop_on_error} onChange={(e) => patch((n) => { n.stop_on_error = e.currentTarget.checked; })} /> Stop on error</label>
          <label class="row tiny"><input type="checkbox" checked={draft().enabled} onChange={(e) => patch((n) => { n.enabled = e.currentTarget.checked; })} /> Enabled</label>
        </div></div>
      </div>
    </Modal>
  );
}

function SceneForm(props: { onSave: (name: string) => void }) {
  const [name, setName] = createSignal("");
  return (
    <>
      <label class="field"><span>Scene name</span><input class="input" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="Evening farming — 8 accounts" /></label>
      <p class="muted small">Captures accounts, targets, window layouts, resource limits, and lane assignment for every running client.</p>
      <button class="btn primary full" onClick={() => name().trim() && props.onSave(name().trim())}><Icon name="save" size={14} /> Save scene</button>
    </>
  );
}
