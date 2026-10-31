import type { Bootstrap, DoctorReport, Settings, Snapshot, UpdateStatus } from "./types";
import { demoBackend } from "./demo";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    try {
      return await tauriInvoke<T>(command, args);
    } catch (error: any) {
      throw new Error(typeof error === "string" ? error : error?.message ?? "Something went wrong.");
    }
  }
  return demoBackend.invoke<T>(command, args);
}

export async function onFleetEvent(handler: (payload: any) => void): Promise<() => void> {
  if (!isTauri) {
    const interval = window.setInterval(() => handler({ demo: true }), 1500);
    return () => window.clearInterval(interval);
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen("fleet:changed", (event) => handler(event.payload));
  return unlisten;
}

export async function onWindowVisibility(handler: (visible: boolean) => void): Promise<() => void> {
  const onVisibility = () => handler(document.visibilityState === "visible");
  document.addEventListener("visibilitychange", onVisibility);
  return () => document.removeEventListener("visibilitychange", onVisibility);
}

export async function pickFile(options: { filters?: { name: string; extensions: string[] }[]; multiple?: boolean } = {}): Promise<string | string[] | null> {
  if (isTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    return open(options as any) as Promise<string | string[] | null>;
  }
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = !!options.multiple;
    if (options.filters?.length) input.accept = options.filters.flatMap((f) => f.extensions).map((e) => `.${e}`).join(",");
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.readAsText(file);
    };
    input.oncancel = () => resolve(null);
    input.click();
  });
}

export async function saveFile(options: { defaultPath?: string; filters?: { name: string; extensions: string[] }[]; contents: string }): Promise<string | null> {
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    const path = await save({ defaultPath: options.defaultPath, filters: options.filters } as any);
    if (!path) return null;
    await writeTextFile(path as string, options.contents);
    return path as string;
  }
  const blob = new Blob([options.contents], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = options.defaultPath?.split(/[\\/]/).pop() ?? "rofarm-export.txt";
  anchor.click();
  URL.revokeObjectURL(url);
  return anchor.download;
}

export async function openExternal(url: string) {
  if (!/^https:\/\/(www\.)?roblox\.com\//.test(url) && !/^https:\/\//.test(url) && !/^roblox-player:/.test(url)) {
    throw new Error("Only https:// and Roblox links can be opened.");
  }
  if (isTauri) return invoke("open_external", { url });
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function copyText(text: string) {
  await navigator.clipboard.writeText(text);
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  snapshot: () => invoke<Snapshot>("fleet_snapshot"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  addAccount: (input: Record<string, unknown>, cookie?: string) => invoke<any>("add_account", { input, cookie }),
  removeAccount: (id: string) => invoke<boolean>("remove_account", { id }),
  updateAccount: (id: string, patch: Record<string, unknown>) => invoke<any>("update_account_fields", { id, patch }),
  moveAccount: (id: string, to: string) => invoke<boolean>("move_account", { id, to }),
  addLane: (name: string, color?: string) => invoke<any>("add_lane", { name, color }),
  setLaneRules: (laneId: string, rules: unknown) => invoke<boolean>("set_lane_rules", { laneId, rules }),
  saveProfile: (profile: unknown) => invoke<any>("save_profile", { profile }),
  removeProfile: (id: string) => invoke<boolean>("remove_profile", { id }),
  parseTarget: (input: string) => invoke<any>("parse_target", { input }),
  launch: (accounts: string[], target: string, profileId?: string) => invoke<any>("launch", { accounts, target, profileId }),
  killClient: (clientId: string) => invoke<boolean>("kill_client", { clientId }),
  killAll: () => invoke<number>("kill_all"),
  validateAccounts: (ids: string[]) => invoke<any>("validate_accounts", { ids }),
  undo: () => invoke<string | null>("undo"),
  redo: () => invoke<string | null>("redo"),
  undoState: () => invoke<{ can_undo: boolean; can_redo: boolean; undo_label?: string | null; redo_label?: string | null }>("undo_state"),
  vaultStatus: () => invoke<Snapshot["vault"]>("vault_status"),
  createVault: (passphrase: string) => invoke<any>("create_vault", { passphrase }),
  unlockVault: (passphrase: string) => invoke<any>("unlock_vault", { passphrase }),
  lockVault: () => invoke<boolean>("lock_vault"),
  reauthenticate: (passphrase: string) => invoke<boolean>("reauthenticate", { passphrase }),
  revealCookie: (id: string) => invoke<string>("reveal_cookie", { id }),
  copyCookie: (id: string) => invoke<boolean>("copy_cookie", { id }),
  auditLog: () => invoke<import("./types").AuditEntry[]>("audit_log"),
  networkLog: () => invoke<import("./types").NetworkEntry[]>("network_log"),
  doctor: () => invoke<DoctorReport>("doctor"),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  checkUpdates: () => invoke<UpdateStatus>("check_updates"),
  remoteStatus: () => invoke<any>("remote_status"),
  remoteStart: (config: Record<string, unknown>) => invoke<any>("remote_start", config),
  remoteStop: () => invoke<any>("remote_stop"),
  plugins: () => invoke<any[]>("plugins_list"),
  systemInfo: () => invoke<any>("system_info"),
  listServers: (placeId: number) => invoke<any[]>("list_servers", { placeId }),
};
