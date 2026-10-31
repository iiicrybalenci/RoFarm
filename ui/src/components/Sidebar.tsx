import { For, Show } from "solid-js";
import { Icon, type IconName } from "./Icon";
import {
  activeNotices,
  openPalette,
  setSidebarCollapsed,
  setView,
  sidebarCollapsed,
  snapshot,
  view,
  runningClients,
  settings,
} from "../lib/store";
import { t } from "../lib/i18n";
import type { ViewId } from "../lib/types";
import { enterMiniMode } from "./Titlebar";

const primary: { id: ViewId; icon: IconName; badge?: () => number }[] = [
  { id: "board", icon: "board" },
  { id: "fleet", icon: "fleet", badge: () => runningClients().length },
  { id: "timeline", icon: "timeline" },
  { id: "servers", icon: "servers" },
  { id: "automations", icon: "automation" },
];

const tools: { id: ViewId; icon: IconName; badge?: () => number }[] = [
  { id: "inventory", icon: "inventory" },
  { id: "friends", icon: "friends" },
  { id: "vault", icon: "vault" },
  { id: "metrics", icon: "metrics" },
  { id: "logs", icon: "logs", badge: () => activeNotices().filter((n) => !n.read).length },
];

export function Sidebar() {
  const collapsed = () => sidebarCollapsed();
  return (
    <nav class={`sidebar ${collapsed() ? "collapsed" : ""}`} aria-label="Primary">
      <div class="nav-section">
        <Show when={!collapsed()}><div class="nav-label">Workspace</div></Show>
        <For each={primary}>
          {(item) => <NavButton item={item} />}
        </For>
      </div>
      <div class="nav-section">
        <Show when={!collapsed()}><div class="nav-label">Tools</div></Show>
        <For each={tools}>
          {(item) => <NavButton item={item} />}
        </For>
      </div>
      <div class="sidebar-footer">
        <button class="nav-item" onClick={openPalette} title="Command palette (Ctrl+K)">
          <span class="nav-icon"><Icon name="command" /></span>
          <span class="nav-text">Command</span>
          <span class="count">⌘K</span>
        </button>
        <button class="nav-item" onClick={() => setView("settings")} classList={{ active: view() === "settings" }} title="Settings">
          <span class="nav-icon"><Icon name="settings" /></span>
          <span class="nav-text">{t("nav.settings")}</span>
          <Show when={settings().updates?.last_checked}>{ }</Show>
        </button>
        <button class="nav-item" onClick={enterMiniMode} title="Mini mode (Ctrl+Shift+M)">
          <span class="nav-icon"><Icon name="monitor" /></span>
          <span class="nav-text">Mini mode</span>
        </button>
        <button class="nav-item" onClick={() => setSidebarCollapsed(!collapsed())} title={collapsed() ? "Expand sidebar" : "Collapse sidebar"}>
          <span class="nav-icon"><Icon name="sidebar" /></span>
          <span class="nav-text">Collapse</span>
        </button>
        <Show when={!collapsed()}>
          <div class="build">v0.1.0 · {snapshot().fleet.accounts ? Object.keys(snapshot().fleet.accounts).length : 0} accounts</div>
        </Show>
      </div>
    </nav>
  );
}

function NavButton(props: { item: { id: ViewId; icon: IconName; badge?: () => number } }) {
  const badge = () => props.item.badge?.() ?? 0;
  return (
    <button
      class="nav-item"
      classList={{ active: view() === props.item.id }}
      onClick={() => setView(props.item.id)}
      title={t(`nav.${props.item.id}`)}
      aria-current={view() === props.item.id ? "page" : undefined}
    >
      <span class="nav-icon"><Icon name={props.item.icon} /></span>
      <span class="nav-text">{t(`nav.${props.item.id}`)}</span>
      <Show when={badge() > 0}><span class="count">{badge()}</span></Show>
    </button>
  );
}
