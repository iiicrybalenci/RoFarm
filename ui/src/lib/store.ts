import { batch, createMemo, createSignal } from "solid-js";
import { api, invoke, isTauri, onFleetEvent } from "./api";
import { defaultSettings, demoSnapshot } from "./fixtures";
import { applyTheme, paletteFromSettings } from "./themes";
import { setLanguage } from "./i18n";
import type {
  Account,
  Bootstrap,
  Density,
  Lane,
  LiveClient,
  Notice,
  Settings,
  Snapshot,
  Toast,
  ViewId,
} from "./types";

const startedAt = performance.now();

const initialSnapshot = demoSnapshot();
const [snapshot, setSnapshot] = createSignal<Snapshot>(initialSnapshot);
const [settings, setSettings] = createSignal<Settings>(defaultSettings());
const [bootstrap, setBootstrap] = createSignal<Bootstrap | null>(null);
const [view, setViewSignal] = createSignal<ViewId>("board");
const [selected, setSelectedSignal] = createSignal<Set<string>>(new Set());
const [focusedAccountId, setFocusedAccountId] = createSignal<string | null>(null);
const [paletteOpen, setPaletteOpen] = createSignal(false);
const [cheatSheetOpen, setCheatSheetOpen] = createSignal(false);
const [notificationsOpen, setNotificationsOpen] = createSignal(false);
const [onboardingOpen, setOnboardingOpen] = createSignal(false);
const [accountDialog, setAccountDialog] = createSignal<{ mode: "add" | "edit"; id?: string } | null>(null);
const [launchDialog, setLaunchDialog] = createSignal<{ accounts: string[] } | null>(null);
const [sceneDialog, setSceneDialog] = createSignal(false);
const [reauthDialog, setReauthDialog] = createSignal<{ action: "reveal" | "copy" | "backup" | "change"; id?: string } | null>(null);
const [settingsSection, setSettingsSection] = createSignal("appearance");
const [toasts, setToasts] = createSignal<Toast[]>([]);
const [busy, setBusy] = createSignal<string | null>(null);
const [globalFilter, setGlobalFilter] = createSignal("");
const [sidebarCollapsed, setSidebarCollapsed] = createSignal(false);
const [connection, setConnection] = createSignal<"connecting" | "live" | "demo" | "offline">("connecting");
const [lastRefresh, setLastRefresh] = createSignal(Date.now());
const [panel, setPanel] = createSignal<string | null>(null);
const [systemInfo, setSystemInfo] = createSignal<any>(null);
const [notificationsHistory, setNotificationsHistory] = createSignal<Notice[]>([]);
const [quickFilter, setQuickFilter] = createSignal<"all" | "running" | "needs_login" | "favourites">("all");
const [mini, setMini] = createSignal(new URLSearchParams(location.search).get("mini") === "1");

let refreshInFlight: Promise<void> | null = null;
let toastId = 0;
let saveTimer: number | undefined;
let unlisten: (() => void) | null = null;

export {
  snapshot,
  settings,
  bootstrap,
  view,
  selected,
  focusedAccountId,
  paletteOpen,
  cheatSheetOpen,
  notificationsOpen,
  onboardingOpen,
  accountDialog,
  launchDialog,
  sceneDialog,
  reauthDialog,
  settingsSection,
  toasts,
  busy,
  globalFilter,
  sidebarCollapsed,
  connection,
  lastRefresh,
  panel,
  systemInfo,
  notificationsHistory,
  quickFilter,
  mini,
};

export const isDemo = () => bootstrap()?.demo ?? !isTauri;

export function toast(title: string, message = "", kind: Toast["kind"] = "info") {
  const entry: Toast = { id: ++toastId, title, message, kind };
  setToasts((current) => [...current, entry]);
  window.setTimeout(() => setToasts((current) => current.filter((item) => item.id !== entry.id)), 4200);
}

export function dismissToast(id: number) {
  setToasts((current) => current.filter((item) => item.id !== id));
}

export function setView(next: ViewId) {
  setViewSignal(next);
  setPanel(null);
  if (!mini()) history.replaceState(null, "", `#${next}`);
  requestAnimationFrame(() => document.getElementById("view-title")?.focus());
}

export function setSelected(next: Set<string>) {
  setSelectedSignal(next);
}

export function toggleSelected(id: string) {
  setSelectedSignal((current) => {
    const next = new Set(current);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  });
}

export function clearSelection() {
  setSelectedSignal(new Set<string>());
}

export function openAccount(id: string) {
  setFocusedAccountId(id);
}

export function closeAccount() {
  setFocusedAccountId(null);
}

export const accounts = createMemo<Account[]>(() => Object.values(snapshot().fleet.accounts));
export const lanes = createMemo<Lane[]>(() => Object.values(snapshot().fleet.lanes).sort((a, b) => a.order - b.order));
export const liveClients = createMemo<LiveClient[]>(() => snapshot().live);
export const selectedAccounts = createMemo<Account[]>(() => accounts().filter((a) => selected().has(a.id)));
export const focusedAccount = createMemo<Account | null>(() => {
  const id = focusedAccountId();
  return id ? snapshot().fleet.accounts[id] ?? null : null;
});
export const runningClients = createMemo(() => liveClients().filter((c) => ["launching", "in_game", "disconnected", "restarting"].includes(c.state)));
export const activeNotices = createMemo(() => snapshot().fleet.notices.filter((n) => !n.dismissed));
export const unreadNotices = createMemo(() => activeNotices().filter((n) => !n.read).length);

export function liveForAccount(accountId: string): LiveClient | undefined {
  return liveClients().find((c) => c.account_id === accountId);
}

export function accountStatus(account: Account): { label: string; tone: string } {
  const live = liveForAccount(account.id);
  if (live) {
    const map: Record<string, string> = {
      in_game: "In game",
      launching: "Launching",
      disconnected: "Disconnected",
      restarting: "Restarting",
      failed: "Failed",
      queued: "Queued",
      stopped: "Stopped",
    };
    return { label: map[live.state] ?? live.state, tone: live.state };
  }
  if (account.login.kind === "expired" || account.login.kind === "invalid") return { label: "Needs login", tone: "needs_login" };
  if (account.presence === "Online") return { label: "Online", tone: "online" };
  return { label: "Idle", tone: "idle" };
}

export function filteredAccounts(list: Account[] = accounts()): Account[] {
  const needle = globalFilter().trim().toLowerCase();
  let result = list;
  const filter = quickFilter();
  if (filter === "running") result = result.filter((a) => !!liveForAccount(a.id));
  if (filter === "needs_login") result = result.filter((a) => ["expired", "invalid"].includes(a.login.kind));
  if (filter === "favourites") result = result.filter((a) => a.favourite);
  if (needle) {
    result = result.filter((a) =>
      [a.username, a.display_name, a.alias ?? "", a.notes, ...a.tags].join(" ").toLowerCase().includes(needle),
    );
  }
  return result;
}

export async function boot() {
  try {
    setConnection("connecting");
    const data = await api.bootstrap();
    batch(() => {
      setBootstrap(data);
      setSnapshot(data.snapshot);
      setSettings(data.settings);
      setNotificationsHistory(data.snapshot.fleet.notices);
      setLanguage(data.settings.language ?? "en");
      setConnection(data.demo ? "demo" : "live");
      if (!data.snapshot.settings.onboarding?.completed) setOnboardingOpen(true);
    });
    applyTheme(data.settings);
    if (!isTauri) setConnection("demo");
    unlisten = await onFleetEvent(async () => {
      if (document.visibilityState !== "visible") return;
      await refresh();
    });
    api.systemInfo().then(setSystemInfo).catch(() => undefined);
  } catch (error: any) {
    setConnection("offline");
    toast("Could not load RoFarm", error.message ?? "Is rofarmd running?", "error");
  }
  requestAnimationFrame(() => document.documentElement.classList.add("ready"));
}

export async function refresh() {
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = (async () => {
    try {
      const next = await api.snapshot();
      setSnapshot(next);
      setLastRefresh(Date.now());
      setConnection(isDemo() ? "demo" : "live");
      if (next.fleet.notices.length) {
        const unread = next.fleet.notices.filter((n) => !n.read);
        if (unread.length > notificationsHistory().filter((n) => !n.read).length) {
          const newest = unread[0];
          if (newest) toast(newest.title, newest.body, newest.severity === "critical" ? "error" : newest.severity === "warning" ? "warning" : "info");
        }
        setNotificationsHistory(next.fleet.notices);
      }
    } catch (error: any) {
      if (isTauri) setConnection("offline");
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

export function updateSettings(mutator: (draft: Settings) => void, persist = true) {
  const next = structuredClone(settings());
  mutator(next);
  setSettings(next);
  applyTheme(next);
  if (next.language) setLanguage(next.language);
  if (persist) scheduleSettingsSave();
}

function scheduleSettingsSave() {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    try {
      await api.saveSettings(settings());
    } catch (error: any) {
      toast("Settings were not saved", error.message, "error");
    }
  }, 350);
}

export async function saveSettingsNow() {
  window.clearTimeout(saveTimer);
  try {
    await api.saveSettings(settings());
    toast("Settings saved", "", "success");
  } catch (error: any) {
    toast("Settings were not saved", error.message, "error");
  }
}

export async function withBusy<T>(label: string, action: () => Promise<T>): Promise<T | undefined> {
  setBusy(label);
  try {
    return await action();
  } catch (error: any) {
    const message = typeof error === "string" ? error : error?.message ?? "Something went wrong.";
    toast(label, message, "error");
    return undefined;
  } finally {
    setBusy(null);
  }
}

export async function killAll() {
  await withBusy("Stopping every client", async () => {
    const killed = await api.killAll();
    toast("Fleet stopped", `${killed} client${killed === 1 ? "" : "s"} stopped.`, "warning");
    await refresh();
  });
}

export async function killClient(clientId: string) {
  await withBusy("Stopping client", async () => {
    await api.killClient(clientId);
    await refresh();
  });
}

export async function moveAccount(accountId: string, laneId: string) {
  const previous = snapshot();
  setSnapshot((current) => {
    const account = current.fleet.accounts[accountId];
    if (!account) return current;
    return {
      ...current,
      fleet: {
        ...current.fleet,
        accounts: { ...current.fleet.accounts, [accountId]: { ...account, lane_id: laneId } },
      },
    };
  });
  try {
    await api.moveAccount(accountId, laneId);
    await refresh();
  } catch (error: any) {
    setSnapshot(previous);
    toast("Could not move account", error.message, "error");
  }
}

export async function undo() {
  const label = await api.undo();
  await refresh();
  if (label) toast("Undone", String(label), "info");
}

export async function redo() {
  const label = await api.redo();
  await refresh();
  if (label) toast("Redone", String(label), "info");
}

export function setDensity(density: Density) {
  updateSettings((draft) => {
    draft.density = density;
  });
}

export function openPalette() {
  setPaletteOpen(true);
}

export function closeOverlays() {
  setPaletteOpen(false);
  setCheatSheetOpen(false);
  setNotificationsOpen(false);
  setAccountDialog(null);
  setLaunchDialog(null);
  setSceneDialog(false);
  setReauthDialog(null);
}

export function openLaunch(accountIds: string[]) {
  setLaunchDialog({ accounts: accountIds });
}

export function openAddAccount() {
  setAccountDialog({ mode: "add" });
}

export function openEditAccount(id: string) {
  setAccountDialog({ mode: "edit", id });
}

export function closeAccountDialog() {
  setAccountDialog(null);
}

export function openSettings(section = "appearance") {
  setSettingsSection(section);
  setView("settings");
}

export function closeLaunch() {
  setLaunchDialog(null);
}

export function closeReauth() {
  setReauthDialog(null);
}

export function openNotifications() {
  setNotificationsOpen(true);
}

export function closeNotifications() {
  setNotificationsOpen(false);
}

export function openOnboarding() {
  setOnboardingOpen(true);
}

export function closeOnboarding() {
  setOnboardingOpen(false);
}

export function openSceneDialog() {
  setSceneDialog(true);
}

export function closeSceneDialog() {
  setSceneDialog(false);
}

export function openReauth(action: "reveal" | "copy" | "backup" | "change", id?: string) {
  setReauthDialog({ action, id });
}

export const accentColor = createMemo(() => paletteFromSettings(settings()).accent);
export const uptimeMs = () => performance.now() - startedAt;

export function disposeStore() {
  unlisten?.();
  window.clearTimeout(saveTimer);
}
