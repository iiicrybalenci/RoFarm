import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { Icon } from "../components/Icon";
import { Avatar, EmptyState, Modal, Switch } from "../components/Primitives";
import {
  accounts,
  accountStatus,
  clearSelection,
  filteredAccounts,
  lanes,
  liveForAccount,
  moveAccount,
  openAccount,
  openAddAccount,
  openLaunch,
  openSceneDialog,
  refresh,
  selected,
  setSelected,
  snapshot,
  toast,
  toggleSelected,
  undo,
} from "../lib/store";
import { api, invoke } from "../lib/api";
import { formatRobux } from "../lib/format";
import type { Account, Lane } from "../lib/types";
import { t } from "../lib/i18n";

export default function Board() {
  const [transform, setTransform] = createSignal({ x: 34, y: 22, scale: 1 });
  const [panning, setPanning] = createSignal(false);
  const [spaceDown, setSpaceDown] = createSignal(false);
  const [dragging, setDragging] = createSignal<string | null>(null);
  const [dropLane, setDropLane] = createSignal<string | null>(null);
  const [laneModal, setLaneModal] = createSignal<Lane | null>(null);
  const [addLaneOpen, setAddLaneOpen] = createSignal(false);
  const [sceneOpen, setSceneOpen] = createSignal(false);
  const [fileDrag, setFileDrag] = createSignal(false);
  let canvas: HTMLDivElement | undefined;
  let plane: HTMLDivElement | undefined;
  let panStart = { x: 0, y: 0, tx: 0, ty: 0 };

  const zoomBy = (factor: number, center?: { x: number; y: number }) => {
    setTransform((current) => {
      const nextScale = Math.min(1.9, Math.max(0.42, current.scale * factor));
      const point = center ?? { x: (canvas?.clientWidth ?? 900) / 2, y: (canvas?.clientHeight ?? 600) / 2 };
      const worldX = (point.x - current.x) / current.scale;
      const worldY = (point.y - current.y) / current.scale;
      return { scale: nextScale, x: point.x - worldX * nextScale, y: point.y - worldY * nextScale };
    });
  };

  const fit = () => {
    if (!canvas || !plane) return;
    const width = plane.scrollWidth;
    const height = plane.scrollHeight;
    const pad = 36;
    const scale = Math.min(1, (canvas.clientWidth - pad) / Math.max(1, width), (canvas.clientHeight - pad) / Math.max(1, height));
    setTransform({ scale: Math.max(0.42, scale), x: pad / 2, y: pad / 2 });
  };

  const onWheel = (event: WheelEvent) => {
    if (!canvas) return;
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      const rect = canvas.getBoundingClientRect();
      zoomBy(event.deltaY < 0 ? 1.08 : 0.925, { x: event.clientX - rect.left, y: event.clientY - rect.top });
    } else {
      event.preventDefault();
      setTransform((current) => ({ ...current, x: current.x - (event.shiftKey ? 0 : event.deltaX), y: current.y - (event.shiftKey ? event.deltaY : event.deltaY) }));
    }
  };

  onMount(() => {
    canvas?.addEventListener("wheel", onWheel, { passive: false });
    const keydown = (event: KeyboardEvent) => {
      if (event.code === "Space" && !["INPUT", "TEXTAREA", "SELECT"].includes((event.target as HTMLElement)?.tagName)) {
        event.preventDefault();
        setSpaceDown(true);
      }
    };
    const keyup = (event: KeyboardEvent) => {
      if (event.code === "Space") setSpaceDown(false);
    };
    window.addEventListener("keydown", keydown);
    window.addEventListener("keyup", keyup);
    setTimeout(fit, 60);
    onCleanup(() => {
      canvas?.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", keydown);
      window.removeEventListener("keyup", keyup);
    });
  });

  const startPan = (event: PointerEvent) => {
    if (event.button === 1 || spaceDown() || event.target === event.currentTarget || (event.target as HTMLElement).classList.contains("board-plane")) {
      event.preventDefault();
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
      panStart = { x: event.clientX, y: event.clientY, tx: transform().x, ty: transform().y };
      setPanning(true);
    }
  };
  const movePan = (event: PointerEvent) => {
    if (!panning()) return;
    setTransform((current) => ({ ...current, x: panStart.tx + event.clientX - panStart.x, y: panStart.ty + event.clientY - panStart.y }));
  };
  const endPan = () => setPanning(false);

  const clickCard = (event: MouseEvent, account: Account) => {
    if (event.ctrlKey || event.metaKey) toggleSelected(account.id);
    else if (event.shiftKey && selected().size) {
      const ordered = filteredAccounts();
      const anchor = ordered.findIndex((a) => selected().has(a.id));
      const target = ordered.findIndex((a) => a.id === account.id);
      if (anchor >= 0 && target >= 0) {
        const [from, to] = anchor < target ? [anchor, target] : [target, anchor];
        setSelected(new Set([...selected(), ...ordered.slice(from, to + 1).map((a) => a.id)]));
      }
    } else setSelected(new Set([account.id]));
  };

  const dropOnLane = async (laneId: string) => {
    const id = dragging();
    setDropLane(null);
    setDragging(null);
    if (!id) return;
    const account = accounts().find((a) => a.id === id);
    if (!account || account.lane_id === laneId) return;
    await moveAccount(id, laneId);
    const lane = lanes().find((l) => l.id === laneId);
    toast("Moved to " + lane?.name, lane?.rules?.max_running ? `Lane limit ${lane.rules.max_running} applies immediately.` : "Lane rules apply immediately.", "success");
  };

  const handleFileDrop = async (event: DragEvent) => {
    event.preventDefault();
    setFileDrag(false);
    const text = event.dataTransfer?.getData("text/plain");
    const file = event.dataTransfer?.files?.[0];
    try {
      if (file) {
        const contents = await file.text();
        const result: any = await invoke("import_accounts", { text: contents });
        toast("Imported from file", `${result.imported ?? 0} account(s) encrypted.`, "success");
        await refresh();
      } else if (text && /_|WARNING|roblox\.com/.test(text)) {
        const result: any = await invoke("import_accounts", { text });
        toast("Imported", `${result.imported ?? 0} account(s) encrypted.`, "success");
        await refresh();
      } else if (text) {
        window.location.hash = "servers";
      }
    } catch (error: any) {
      toast("Drop import failed", error.message, "error");
    }
  };

  const selectionCount = () => selected().size;

  return (
    <div class="view" style={{ position: "relative" }}>
      <div class="board-shell">
        <div class="board-toolbar">
          <button class="icon-btn" onClick={() => setAddLaneOpen(true)} title="Add lane" aria-label="Add lane"><Icon name="plus" size={15} /></button>
          <button class="icon-btn" onClick={openAddAccount} title="Add account" aria-label="Add account"><Icon name="user" size={15} /></button>
          <div style={{ width: "1px", background: "var(--line)", margin: "4px 2px" }} />
          <button class="icon-btn" onClick={() => setSceneOpen(!sceneOpen())} title="Scenes" aria-label="Scenes"><Icon name="save" size={15} /></button>
          <button class="icon-btn" onClick={() => zoomBy(0.86)} title="Zoom out" aria-label="Zoom out"><Icon name="subtract" size={14} /></button>
          <span class="board-zoom">{Math.round(transform().scale * 100)}%</span>
          <button class="icon-btn" onClick={() => zoomBy(1.16)} title="Zoom in" aria-label="Zoom in"><Icon name="plus" size={14} /></button>
          <button class="icon-btn" onClick={fit} title="Fit lanes" aria-label="Fit lanes"><Icon name="layout" size={14} /></button>
        </div>
        <Show when={sceneOpen()}>
          <div class="modal-backdrop" style={{ background: "transparent" }} onMouseDown={() => setSceneOpen(false)}>
            <div class="modal narrow" style={{ position: "absolute", top: "60px", right: "16px", "max-height": "70vh" }} onMouseDown={(e) => e.stopPropagation()}>
              <div class="modal-head"><h2>Scenes</h2><div class="grow" /><button class="icon-btn" onClick={() => setSceneOpen(false)} aria-label="Close"><Icon name="close" size={14} /></button></div>
              <div class="modal-body">
                <Show when={Object.values(snapshot().fleet.scenes).length} fallback={<p class="muted" style={{ margin: 0, "font-size": "0.78rem" }}>No saved scenes yet. A scene captures accounts, targets, limits and window layout.</p>}>
                  <For each={Object.values(snapshot().fleet.scenes)}>
                    {(scene) => (
                      <div class="row between" style={{ gap: "10px" }}>
                        <div>
                          <div style={{ "font-weight": 600, "font-size": "0.8rem" }}>{scene.name}</div>
                          <div class="tiny muted">{scene.description || "Saved fleet state"}</div>
                        </div>
                        <button class="btn sm primary" onClick={async () => { await invoke("apply_scene", { id: scene.id }); setSceneOpen(false); await refresh(); toast("Scene applied", scene.name, "success"); }}>Apply</button>
                      </div>
                    )}
                  </For>
                </Show>
                <button class="btn" onClick={() => { setSceneOpen(false); openSceneDialog(); }}><Icon name="save" size={13} />Save current fleet as scene</button>
              </div>
            </div>
          </div>
        </Show>
        <div
          class={`board-canvas ${panning() ? "panning" : ""}`}
          ref={canvas}
          onPointerDown={startPan}
          onPointerMove={movePan}
          onPointerUp={endPan}
          onPointerCancel={endPan}
          onDragOver={(event) => { event.preventDefault(); if (event.dataTransfer?.types?.includes("Files")) setFileDrag(true); }}
          onDragLeave={() => setFileDrag(false)}
          onDrop={handleFileDrop}
          style={{ cursor: spaceDown() || panning() ? "grabbing" : undefined }}
        >
          <Show when={fileDrag()}>
            <div style={{ position: "absolute", inset: "16px", "z-index": 20, border: "2px dashed var(--accent)", "border-radius": "16px", background: "color-mix(in srgb, var(--accent) 8%, var(--bg))", display: "grid", "place-items": "center", "pointer-events": "none" }}>
              <div style={{ "text-align": "center" }}><Icon name="upload" size={28} /><div style={{ "margin-top": "6px", "font-weight": 600 }}>Drop cookies or accounts here</div><div class="muted tiny">.txt, .json, or a RoFarm / RAM export</div></div>
            </div>
          </Show>
          <div class="board-plane" ref={plane} style={{ transform: `translate(${transform().x}px, ${transform().y}px) scale(${transform().scale})` }}>
            <For each={lanes()}>
              {(lane) => {
                const laneAccounts = () => filteredAccounts().filter((a) => a.lane_id === lane.id);
                return (
                  <section
                    class="lane-column"
                    classList={{ "drop-target": dropLane() === lane.id }}
                    style={{ "--lane-color": lane.color } as any}
                    aria-label={`${lane.name} lane`}
                    onDragOver={(event) => { event.preventDefault(); if (dragging()) setDropLane(lane.id); }}
                    onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDropLane(null); }}
                    onDrop={(event) => { event.preventDefault(); void dropOnLane(lane.id); }}
                  >
                    <header class="lane-head">
                      <span class="lane-swatch" />
                      <h2>{lane.name}</h2>
                      <span class="lane-count">{laneAccounts().length}</span>
                      <div class="spacer" />
                      <Show when={lane.rules?.anti_afk?.enabled}><span class="lane-limit" title="Anti-AFK enabled">AFK</span></Show>
                      <Show when={lane.rules?.max_running}><span class="lane-limit">max {lane.rules.max_running}</span></Show>
                      <button class="icon-btn" style={{ width: "24px", height: "24px" }} onClick={() => setLaneModal(lane)} aria-label={`${lane.name} lane settings`} title="Lane rules"><Icon name="settings" size={13} /></button>
                      <button class="icon-btn" style={{ width: "24px", height: "24px" }} onClick={() => openLaunch(laneAccounts().map((a) => a.id))} aria-label={`Launch ${lane.name}`} title="Launch this lane" disabled={!laneAccounts().length}><Icon name="play" size={13} /></button>
                    </header>
                    <div class="lane-stack">
                      <Show when={laneAccounts().length} fallback={<div class="lane-empty">{dragging() ? t("board.dropHere") : lane.rules?.default_target ? "No accounts · drop one here" : "No accounts in this lane"}</div>}>
                        <For each={laneAccounts()}>
                          {(account) => <AccountCard account={account} selected={selected().has(account.id)} onSelect={(event) => clickCard(event, account)} onDragStart={() => setDragging(account.id)} onDragEnd={() => { setDragging(null); setDropLane(null); }} />}
                        </For>
                      </Show>
                    </div>
                  </section>
                );
              }}
            </For>
            <button class="add-lane-btn" onClick={() => setAddLaneOpen(true)}><Icon name="plus" size={15} />{t("board.addLane")}</button>
          </div>
          <div class="board-hint">
            <span><kbd>Space</kbd> + drag to pan</span>
            <span><kbd>Ctrl</kbd> + wheel to zoom</span>
            <span>Drag a card between lanes to apply lane rules</span>
          </div>
        </div>
      </div>
      <Show when={selectionCount() > 1}>
        <div class="bulkbar" role="toolbar" aria-label="Bulk actions">
          <span class="pill info">{selectionCount()} selected</span>
          <button class="btn sm primary" onClick={() => openLaunch(Array.from(selected()))}><Icon name="play" size={12} />Launch</button>
          <button class="btn sm" onClick={() => clearSelection()}>Clear</button>
          <div class="segmented">
            <For each={lanes()}>{(lane) => <button onClick={async () => { for (const id of selected()) await api.moveAccount(id, lane.id); clearSelection(); await refresh(); toast("Moved accounts", lane.name, "success"); }}>{lane.name}</button>}</For>
          </div>
        </div>
      </Show>
      <Show when={laneModal()}>
        {(lane) => <LaneRulesModal lane={lane()} onClose={() => setLaneModal(null)} />}
      </Show>
      <Show when={addLaneOpen()}><AddLaneModal onClose={() => setAddLaneOpen(false)} /></Show>
    </div>
  );
}

function AccountCard(props: { account: Account; selected: boolean; onSelect: (event: MouseEvent) => void; onDragStart: () => void; onDragEnd: () => void }) {
  const account = () => props.account;
  const live = () => liveForAccount(account().id);
  const status = () => accountStatus(account());
  return (
    <article
      class="account-card"
      classList={{ selected: props.selected, running: !!live(), dragging: false }}
      draggable={true}
      onDragStart={(event) => { event.dataTransfer?.setData("text/plain", account().id); event.dataTransfer!.effectAllowed = "move"; props.onDragStart(); }}
      onDragEnd={props.onDragEnd}
      onClick={props.onSelect}
      onDblClick={() => openAccount(account().id)}
      onKeyDown={(event) => { if (event.key === "Enter") openAccount(account().id); if (event.key === " ") { event.preventDefault(); props.onSelect(event as any); } }}
      tabIndex={0}
      aria-label={`${account().username}, ${status().label}`}
      role="button"
    >
      <div class="card-top">
        <Avatar account={account()} />
        <div class="identity">
          <div class="name">
            {account().display_name}
            <Show when={account().favourite}><Icon name="heart" size={10} /></Show>
            <Show when={account().premium}><span class="tag" style={{ "font-size": "0.54rem", padding: "0 4px" }}>P</span></Show>
          </div>
          <div class="handle">@{account().username}</div>
        </div>
        <div class="card-actions" onClick={(event) => event.stopPropagation()}>
          <button class="mini-action" title="Launch" aria-label={`Launch ${account().username}`} onClick={() => openLaunch([account().id])}><Icon name="play" size={11} /></button>
          <button class="mini-action" title="Open details" aria-label="Open details" onClick={() => openAccount(account().id)}><Icon name="more" size={13} /></button>
        </div>
      </div>
      <div class="card-meta">
        <span class={`pill ${status().tone}`} style={{ "font-size": "0.6rem", padding: "1px 5px" }}><span class={`dot ${status().tone}`} />{status().label}</span>
        <span class="robux">{formatRobux(account().robux)} R$</span>
        <div class="card-health" title={`Health ${account().health.score}`}>
          <div class={`health-bar ${account().health.score < 60 ? "danger" : account().health.score < 80 ? "warn" : ""}`}><span style={{ width: `${account().health.score}%` }} /></div>
          <span>{account().health.score}</span>
        </div>
      </div>
      <Show when={account().tags.length}>
        <div class="card-tags">
          <For each={account().tags.slice(0, 3)}>{(tag) => <span class="tag">{tag}</span>}</For>
        </div>
      </Show>
      <Show when={live()}>
        {(client) => (
          <div class="card-meta" style={{ "margin-top": "5px", "font-size": "0.62rem" }}>
            <Icon name="cpu" size={10} />
            <span>{client().cpu_pct.toFixed(1)}%</span>
            <Icon name="memory" size={10} />
            <span>{client().mem_mb} MB</span>
            <span class="grow" />
            <span class="muted">{client().pid ? `pid ${client().pid}` : "starting"}</span>
          </div>
        )}
      </Show>
      <Show when={!live() && account().health.issues.length}>
        <div class="tiny" style={{ "margin-top": "5px", color: "var(--warn)", display: "flex", gap: "4px", "align-items": "center" }}>
          <Icon name="warning" size={10} />{account().health.issues[0].label}
        </div>
      </Show>
    </article>
  );
}

function LaneRulesModal(props: { lane: Lane; onClose: () => void }) {
  const rules = props.lane.rules;
  const [name, setName] = createSignal(props.lane.name);
  const [maxRunning, setMaxRunning] = createSignal(rules.max_running ?? 0);
  const [target, setTarget] = createSignal(rules.default_target ?? "");
  const [autoRejoin, setAutoRejoin] = createSignal(rules.auto_rejoin);
  const [restartOnCrash, setRestartOnCrash] = createSignal(rules.restart_on_crash);
  const [stayDisconnected, setStayDisconnected] = createSignal(rules.allowed_to_stay_disconnected);
  const [antiAfk, setAntiAfk] = createSignal(rules.anti_afk.enabled);
  const [antiAfkInterval, setAntiAfkInterval] = createSignal(rules.anti_afk.interval_seconds);
  const [rotation, setRotation] = createSignal(rules.rotation?.keep_running ?? 0);
  const [busy, setBusy] = createSignal(false);

  const save = async () => {
    setBusy(true);
    try {
      const nextRules = {
        ...rules,
        default_target: target().trim() || null,
        max_running: maxRunning() > 0 ? maxRunning() : null,
        auto_rejoin: autoRejoin(),
        restart_on_crash: restartOnCrash(),
        allowed_to_stay_disconnected: stayDisconnected(),
        anti_afk: { ...rules.anti_afk, enabled: antiAfk(), interval_seconds: antiAfkInterval() },
        rotation: rotation() > 0 ? { ...(rules.rotation ?? { max_session_minutes: 180, swap_cooldown_seconds: 120, order: "round_robin" }), keep_running: rotation() } : null,
      };
      if (name() !== props.lane.name) {
        await invoke("update_lane", { lane: { ...props.lane, name: name(), rules: nextRules } });
      }
      await api.setLaneRules(props.lane.id, nextRules);
      props.onClose();
      await refresh();
      toast("Lane rules saved", props.lane.name, "success");
    } catch (error: any) {
      toast("Could not save lane rules", error.message, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={`${props.lane.name} lane rules`}
      icon="settings"
      onClose={props.onClose}
      footer={<><button class="btn" onClick={props.onClose}>Cancel</button><button class="btn primary" disabled={busy()} onClick={save}>{busy() ? "Saving…" : "Save rules"}</button></>}
    >
      <div class="row" style={{ "align-items": "flex-start" }}>
        <div class="field grow">
          <label class="field-label" for="lane-name">Lane name</label>
          <input id="lane-name" class="input" value={name()} onInput={(event) => setName(event.currentTarget.value)} />
        </div>
        <div class="field" style={{ width: "130px" }}>
          <label class="field-label" for="lane-max">Max running</label>
          <input id="lane-max" class="input" type="number" min="0" max="100" value={maxRunning()} onInput={(event) => setMaxRunning(Number(event.currentTarget.value))} />
        </div>
      </div>
      <div class="field">
        <label class="field-label" for="lane-target">Default game</label>
        <input id="lane-target" class="input" value={target()} onInput={(event) => setTarget(event.currentTarget.value)} placeholder="Game link or place ID" spellcheck={false} />
        <div class="tiny muted">Used when an account is launched without an explicit target.</div>
      </div>
      <div class="section" style={{ padding: "4px 12px" }}>
        <div class="setting-row"><div class="setting-copy"><div class="setting-title">Auto-rejoin</div><div class="setting-desc">Rejoin the same server after a disconnect.</div></div><div class="setting-control"><Switch checked={autoRejoin()} onChange={setAutoRejoin} label="Auto-rejoin" /></div></div>
        <div class="setting-row"><div class="setting-copy"><div class="setting-title">Restart on crash</div><div class="setting-desc">Subject to the crash breaker and retry budgets.</div></div><div class="setting-control"><Switch checked={restartOnCrash()} onChange={setRestartOnCrash} label="Restart on crash" /></div></div>
        <div class="setting-row"><div class="setting-copy"><div class="setting-title">Allowed to stay disconnected</div><div class="setting-desc">Do not treat a disconnect as a failure.</div></div><div class="setting-control"><Switch checked={stayDisconnected()} onChange={setStayDisconnected} label="Allowed to stay disconnected" /></div></div>
      </div>
      <div class="section" style={{ padding: "12px" }}>
        <div class="row between">
          <div><div class="setting-title">Anti-AFK</div><div class="setting-desc" style={{ "margin": 0 }}>Normal key events only. Off by default.</div></div>
          <Switch checked={antiAfk()} onChange={setAntiAfk} label="Anti-AFK" />
        </div>
        <Show when={antiAfk()}>
          <div class="row" style={{ "margin-top": "10px" }}>
            <span class="tiny muted">Interval (seconds)</span>
            <input class="input" style={{ width: "100px" }} type="number" min="30" max="3600" value={antiAfkInterval()} onInput={(event) => setAntiAfkInterval(Number(event.currentTarget.value))} />
            <span class="tiny muted">Jitter {rules.anti_afk.jitter_percent}%</span>
          </div>
        </Show>
      </div>
      <div class="section" style={{ padding: "12px" }}>
        <div class="row between">
          <div><div class="setting-title">Rotation</div><div class="setting-desc" style={{ margin: 0 }}>Keep N of the lane's accounts running, swapping the least-played.</div></div>
          <input class="input" style={{ width: "80px" }} type="number" min="0" max="50" value={rotation()} onInput={(event) => setRotation(Number(event.currentTarget.value))} />
        </div>
      </div>
      <div class="banner info"><Icon name="info" size={14} /><span>Dragging a card into this lane applies these rules immediately. Per-account limits override lane limits.</span></div>
    </Modal>
  );
}

function AddLaneModal(props: { onClose: () => void }) {
  const [name, setName] = createSignal("");
  const [color, setColor] = createSignal("#6c8cff");
  const [busy, setBusy] = createSignal(false);
  const create = async () => {
    if (!name().trim()) return;
    setBusy(true);
    try {
      await api.addLane(name().trim(), color());
      props.onClose();
      await refresh();
      toast("Lane added", name(), "success");
    } catch (error: any) {
      toast("Could not add lane", error.message, "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title="Add lane" icon="board" narrow onClose={props.onClose} footer={<><button class="btn" onClick={props.onClose}>Cancel</button><button class="btn primary" disabled={busy() || !name().trim()} onClick={create}>Create lane</button></>}>
      <div class="field">
        <label class="field-label" for="new-lane-name">Lane name</label>
        <input id="new-lane-name" class="input" value={name()} onInput={(event) => setName(event.currentTarget.value)} placeholder="e.g. Alt farming" onKeyDown={(event) => { if (event.key === "Enter") void create(); }} />
      </div>
      <div class="field">
        <span class="field-label">Colour</span>
        <input type="color" value={color()} onInput={(event) => setColor(event.currentTarget.value)} style={{ width: "52px", height: "32px", background: "transparent", border: "1px solid var(--line)", "border-radius": "8px" }} />
      </div>
      <div class="banner info"><Icon name="info" size={14} /><span>New lanes start with sensible defaults. Edit the rules from the lane header at any time.</span></div>
    </Modal>
  );
}
