import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { Icon, type IconName } from "./Icon";
import {
  accounts,
  closeOverlays,
  killAll,
  lanes,
  moveAccount,
  openAccount,
  openAddAccount,
  openLaunch,
  openSettings,
  refresh,
  runningClients,
  selected,
  setGlobalFilter,
  setView,
  settings,
  snapshot,
  toggleSelected,
  updateSettings,
  openSceneDialog,
  setCheatSheetOpen,
} from "../lib/store";
import { api, openExternal } from "../lib/api";
import { t } from "../lib/i18n";
import type { ViewId } from "../lib/types";

type Item = {
  id: string;
  group: string;
  label: string;
  hint?: string;
  icon: IconName;
  run: () => void;
};

const viewEntries: { id: ViewId; icon: IconName }[] = [
  { id: "board", icon: "board" },
  { id: "fleet", icon: "fleet" },
  { id: "timeline", icon: "timeline" },
  { id: "servers", icon: "servers" },
  { id: "automations", icon: "automation" },
  { id: "inventory", icon: "inventory" },
  { id: "friends", icon: "friends" },
  { id: "vault", icon: "vault" },
  { id: "metrics", icon: "metrics" },
  { id: "logs", icon: "logs" },
  { id: "settings", icon: "settings" },
];

function score(text: string, query: string): number {
  if (!query) return 1;
  const lower = text.toLowerCase();
  const needle = query.toLowerCase();
  if (lower === needle) return 100;
  if (lower.startsWith(needle)) return 80;
  if (lower.includes(needle)) return 50;
  let qi = 0;
  let gaps = 0;
  for (let i = 0; i < lower.length && qi < needle.length; i++) {
    if (lower[i] === needle[qi]) qi++;
    else gaps++;
  }
  return qi === needle.length ? Math.max(4, 30 - gaps / 4) : 0;
}

export function CommandPalette() {
  const [query, setQuery] = createSignal("");
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let list: HTMLDivElement | undefined;

  const items = createMemo<Item[]>(() => {
    const result: Item[] = [];
    for (const account of accounts()) {
      result.push({
        id: `account-${account.id}`,
        group: t("palette.accounts"),
        label: `@${account.username}`,
        hint: `${lanes().find((l) => l.id === account.lane_id)?.name ?? ""} · ${account.robux ?? 0} R$`,
        icon: "user",
        run: () => openAccount(account.id),
      });
    }
    for (const lane of lanes()) {
      result.push({
        id: `lane-${lane.id}`,
        group: "Lanes",
        label: lane.name,
        hint: `${Object.values(snapshot().fleet.accounts).filter((a) => a.lane_id === lane.id).length} accounts`,
        icon: "board",
        run: () => {
          setView("board");
          setGlobalFilter(lane.name);
        },
      });
    }
    const actions: Item[] = [
      { id: "add", group: t("palette.actions"), label: t("board.addAccount"), hint: "A", icon: "plus", run: openAddAccount },
      {
        id: "launch-selected",
        group: t("palette.actions"),
        label: "Launch selected accounts",
        hint: `${selected().size} selected`,
        icon: "play",
        run: () => openLaunch(Array.from(selected())),
      },
      { id: "launch-all", group: t("palette.actions"), label: "Launch all in same lane…", hint: "", icon: "rocket", run: () => setView("fleet") },
      { id: "kill", group: t("palette.actions"), label: "Stop all clients", hint: "Ctrl+Shift+Alt+K", icon: "kill", run: () => void killAll() },
      { id: "scene", group: t("palette.actions"), label: "Save current fleet as scene", hint: "Ctrl+Alt+S", icon: "save", run: openSceneDialog },
      { id: "refresh", group: t("palette.actions"), label: "Refresh fleet", hint: "Ctrl+R", icon: "refresh", run: () => void refresh() },
      { id: "eco", group: t("palette.actions"), label: settings().eco_mode ? "Turn Eco mode off" : "Turn Eco mode on", icon: "spark", run: () => updateSettings((draft) => { draft.eco_mode = !draft.eco_mode; }) },
      { id: "lock", group: t("palette.actions"), label: "Lock vault", hint: "Ctrl+L", icon: "lock", run: () => void api.lockVault().then(refresh) },
      { id: "cheat", group: t("palette.docs"), label: "Keyboard shortcuts", hint: "?", icon: "keyboard", run: () => setCheatSheetOpen(true) },
      { id: "security", group: t("palette.docs"), label: "Docs: security model", icon: "shield", run: () => openExternal("https://github.com/rofarm/rofarm/blob/main/docs/SECURITY.md") },
      { id: "architecture", group: t("palette.docs"), label: "Docs: architecture", icon: "file", run: () => openExternal("https://github.com/rofarm/rofarm/blob/main/docs/ARCHITECTURE.md") },
      { id: "automation-doc", group: t("palette.docs"), label: "Docs: automation syntax", icon: "automation", run: () => openExternal("https://github.com/rofarm/rofarm/blob/main/docs/AUTOMATION.md") },
    ];
    result.push(...actions);
    for (const view of viewEntries) {
      result.push({
        id: `view-${view.id}`,
        group: t("palette.views"),
        label: t(`nav.${view.id}`),
        hint: "Go",
        icon: view.icon,
        run: () => setView(view.id),
      });
    }
    const settingsEntries: [string, string, IconName][] = [
      ["appearance", "Appearance & themes", "palette"],
      ["performance", "Performance & Eco mode", "cpu"],
      ["security", "Vault & security", "shield"],
      ["notifications", "Notifications", "bell"],
      ["remote", "Remote API & pairing", "globe"],
      ["updates", "Updates", "download"],
      ["plugins", "Plugins", "plugin"],
      ["shortcuts", "Shortcuts", "keyboard"],
    ];
    for (const [section, label, icon] of settingsEntries) {
      result.push({
        id: `setting-${section}`,
        group: t("palette.settings"),
        label,
        icon,
        hint: "Settings",
        run: () => openSettings(section),
      });
    }
    const needle = query().trim();
    return result
      .map((item) => ({ item, s: Math.max(score(item.label, needle), score(item.group, needle) * 0.4, score(item.hint ?? "", needle) * 0.25) }))
      .filter((entry) => entry.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((entry) => entry.item)
      .slice(0, needle ? 24 : 18);
  });

  const groups = createMemo(() => {
    const map = new Map<string, Item[]>();
    for (const item of items()) {
      const list = map.get(item.group) ?? [];
      list.push(item);
      map.set(item.group, list);
    }
    return Array.from(map.entries());
  });

  const flat = () => items();

  const run = (item: Item | undefined) => {
    if (!item) return;
    closeOverlays();
    setTimeout(() => item.run(), 10);
  };

  const onKey = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((current) => Math.min(flat().length - 1, current + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => Math.max(0, current - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(flat()[active()]);
    } else if (event.key === "Escape") {
      closeOverlays();
    }
  };

  onMount(() => {
    input?.focus();
    document.addEventListener("keydown", onKey, true);
    onCleanup(() => document.removeEventListener("keydown", onKey, true));
  });

  const activeId = () => flat()[active()]?.id;

  return (
    <div class="palette-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) closeOverlays(); }}>
      <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
        <div class="palette-input">
          <Icon name="search" size={18} />
          <input
            ref={input}
            value={query()}
            onInput={(event) => { setQuery(event.currentTarget.value); setActive(0); }}
            placeholder={t("palette.placeholder")}
            aria-label="Search commands, accounts, lanes and settings"
          />
          <span class="kbd">Esc</span>
        </div>
        <div class="palette-list" ref={list} role="listbox">
          <Show when={flat().length} fallback={<div style={{ padding: "28px", "text-align": "center" }} class="muted">No matches. Try an account name, “launch”, or “theme”.</div>}>
            <For each={groups()}>
              {([group, entries]) => (
                <>
                  <div class="palette-group">{group}</div>
                  <For each={entries}>
                    {(item) => (
                      <button
                        class="palette-item"
                        classList={{ active: activeId() === item.id }}
                        role="option"
                        aria-selected={activeId() === item.id}
                        onMouseEnter={() => setActive(flat().findIndex((entry) => entry.id === item.id))}
                        onClick={() => run(item)}
                      >
                        <Icon name={item.icon} size={15} />
                        <span>{item.label}</span>
                        <Show when={item.hint}><span class="hint">{item.hint}</span></Show>
                      </button>
                    )}
                  </For>
                </>
              )}
            </For>
          </Show>
        </div>
        <div class="palette-foot">
          <span><span class="kbd">↑↓</span> navigate</span>
          <span><span class="kbd">↵</span> run</span>
          <span><span class="kbd">Esc</span> close</span>
          <span style={{ "margin-left": "auto" }}>{flat().length} results</span>
        </div>
      </div>
    </div>
  );
}
