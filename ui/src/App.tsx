import { ErrorBoundary, Show, Suspense, lazy, onCleanup, onMount } from "solid-js";
import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { FleetBar } from "./components/FleetBar";
import { Toasts } from "./components/Toasts";
import { CommandPalette } from "./components/CommandPalette";
import { CheatSheet } from "./components/CheatSheet";
import { NotificationCenter } from "./components/NotificationCenter";
import { AccountDrawer } from "./components/AccountDrawer";
import { AccountModal } from "./components/AccountModal";
import { LaunchModal } from "./components/LaunchModal";
import { Onboarding } from "./components/Onboarding";
import { ReauthModal } from "./components/ReauthModal";
import { MiniMode } from "./components/MiniMode";
import {
  accountDialog,
  boot,
  cheatSheetOpen,
  closeOverlays,
  launchDialog,
  mini,
  notificationsOpen,
  onboardingOpen,
  openPalette,
  paletteOpen,
  reauthDialog,
  refresh,
  setView,
  view,
  killAll,
  openReauth,
  setCheatSheetOpen,
  settings,
  undo,
  redo,
  snapshot,
} from "./lib/store";
import { isTauri } from "./lib/api";
import type { ViewId } from "./lib/types";

const Board = lazy(() => import("./views/Board"));
const Fleet = lazy(() => import("./views/Fleet"));
const Timeline = lazy(() => import("./views/Timeline"));
const Servers = lazy(() => import("./views/Servers"));
const Automations = lazy(() => import("./views/Automations"));
const Inventory = lazy(() => import("./views/Inventory"));
const Friends = lazy(() => import("./views/Friends"));
const Vault = lazy(() => import("./views/Vault"));
const Metrics = lazy(() => import("./views/Metrics"));
const Logs = lazy(() => import("./views/Logs"));
const Settings = lazy(() => import("./views/Settings"));

const viewComponents: Record<ViewId, any> = {
  board: Board,
  fleet: Fleet,
  timeline: Timeline,
  servers: Servers,
  automations: Automations,
  inventory: Inventory,
  friends: Friends,
  vault: Vault,
  metrics: Metrics,
  logs: Logs,
  settings: Settings,
};

function currentViewFromHash(): ViewId {
  const hash = location.hash.replace("#", "") as ViewId;
  return hash && hash in viewComponents ? hash : "board";
}

function isTyping(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  return ["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName) || element.isContentEditable;
}

export function App() {
  onMount(() => {
    setView(currentViewFromHash());
    void boot();
    const keydown = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
      if (mod && event.key.toLowerCase() === "k") {
        event.preventDefault();
        openPalette();
        return;
      }
      if (event.key === "Escape") {
        closeOverlays();
        return;
      }
      if (isTyping(event.target)) return;
      if (event.key === "?" || (event.shiftKey && event.key === "/")) {
        event.preventDefault();
        setCheatSheetOpen(true);
        return;
      }
      if (mod && event.shiftKey && event.altKey && event.key.toLowerCase() === "k") {
        event.preventDefault();
        void killAll();
        return;
      }
      if (mod && event.key.toLowerCase() === "z") {
        event.preventDefault();
        if (event.shiftKey) void redo();
        else void undo();
        return;
      }
      if (mod && event.key.toLowerCase() === "l") {
        event.preventDefault();
        void import("./lib/api").then(({ api }) => api.lockVault()).then(() => refresh());
        return;
      }
      if (mod && event.key.toLowerCase() === "r") {
        event.preventDefault();
        void refresh();
        return;
      }
      if (mod && ["1", "2", "3", "4", "5"].includes(event.key)) {
        event.preventDefault();
        const map: ViewId[] = ["board", "fleet", "timeline", "automations", "vault"];
        setView(map[Number(event.key) - 1]);
        return;
      }
      if (mod && event.shiftKey && event.key.toLowerCase() === "m") {
        event.preventDefault();
        void import("./lib/api").then(({ invoke, isTauri }) => (isTauri ? invoke("show_mini") : window.open(`${location.pathname}?mini=1`, "_blank", "width=320,height=440")));
      }
    };
    const hashchange = () => setView(currentViewFromHash());
    window.addEventListener("keydown", keydown);
    window.addEventListener("hashchange", hashchange);
    onCleanup(() => {
      window.removeEventListener("keydown", keydown);
      window.removeEventListener("hashchange", hashchange);
    });
  });

  const ViewComponent = () => viewComponents[view()] ?? Board;

  return (
    <Show when={!mini()} fallback={<MiniMode />}>
      <div class="app" classList={{ [`density-${settings().density}`]: true }}>
        <Titlebar />
        <div class="shell">
          <Sidebar />
          <div class="workspace">
            <FleetBar />
            <main class="content" id="main-content">
              <Show when={!isTauri && (snapshot() as any)?.demo === undefined}>
                <></>
              </Show>
              <ErrorBoundary
                fallback={(error, reset) => (
                  <div class="empty">
                    <div class="empty-card">
                      <h2>This view hit a snag</h2>
                      <p>{String(error?.message ?? error)}</p>
                      <button class="btn primary" onClick={reset}>Try again</button>
                    </div>
                  </div>
                )}
              >
                <Suspense fallback={<ViewSkeleton />}>
                  <ViewComponent />
                </Suspense>
              </ErrorBoundary>
            </main>
          </div>
        </div>
        <Show when={paletteOpen()}><CommandPalette /></Show>
        <Show when={cheatSheetOpen()}><CheatSheet /></Show>
        <Show when={accountDialog()}>
          {(dialog) => <AccountModal mode={dialog().mode} id={dialog().id} />}
        </Show>
        <Show when={launchDialog()}>
          {(dialog) => <LaunchModal accounts={dialog().accounts} />}
        </Show>
        <Show when={reauthDialog()}>
          {(dialog) => <ReauthModal action={dialog().action} id={dialog().id} />}
        </Show>
        <Show when={notificationsOpen()}><NotificationCenter /></Show>
        <Show when={onboardingOpen()}><Onboarding /></Show>
        <AccountDrawer />
        <Toasts />
      </div>
    </Show>
  );
}

function ViewSkeleton() {
  return (
    <div class="view" aria-busy="true">
      <div class="view-header">
        <div class="skeleton" style={{ width: "180px", height: "18px" }} />
        <div class="spacer" />
        <div class="skeleton" style={{ width: "90px", height: "26px" }} />
      </div>
      <div class="view-scroll" style={{ display: "grid", "grid-template-columns": "repeat(auto-fill, minmax(280px, 1fr))", gap: "12px", "align-content": "start" }}>
        <div class="skeleton" style={{ height: "150px" }} />
        <div class="skeleton" style={{ height: "150px" }} />
        <div class="skeleton" style={{ height: "150px" }} />
      </div>
    </div>
  );
}
