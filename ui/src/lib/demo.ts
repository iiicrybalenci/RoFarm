import {
  demoAudit,
  demoBootstrap,
  demoFriends,
  demoInventory,
  demoNetwork,
  demoProfiles,
  demoServers,
  defaultSettings,
  demoSnapshot,
  LANE_IDS,
} from "./fixtures";
import type {
  Account,
  AuditEntry,
  Automation,
  Bootstrap,
  ClientState,
  Lane,
  LaunchProfile,
  Notice,
  ServerInstance,
  Settings,
  Snapshot,
} from "./types";

const clone = <T,>(value: T): T => structuredClone(value);
const now = () => new Date().toISOString();
const uuid = () => (globalThis.crypto?.randomUUID?.() ?? `demo-${Math.random().toString(16).slice(2)}`);

class DemoBackend {
  snapshot: Snapshot;
  bootstrap: Bootstrap;
  settings: Settings;
  vaultPassphrase: string | null = "demo-passphrase";
  audit: AuditEntry[] = clone(demoAudit);
  network = clone(demoNetwork);
  friends = clone(demoFriends);
  inventory = clone(demoInventory);
  undoStack: string[] = [];
  redoStack: string[] = [];
  remoteRunning = false;
  plugins = [
    {
      manifest: {
        id: "discord-presence",
        name: "Discord Rich Presence",
        version: "1.2.0",
        description: "Shows the fleet status in Discord.",
        api_version: 1,
        entry: "plugin.wasm",
        permissions: { notifications: true, network_hosts: ["discord.com"] },
      },
      granted: { notifications: true, network_hosts: ["discord.com"] },
      enabled: true,
      installed_at: now(),
      signed: true,
    },
    {
      manifest: {
        id: "obs-overlay",
        name: "OBS status overlay",
        version: "0.4.1",
        description: "Local browser-source overlay with live fleet counters.",
        api_version: 1,
        entry: "plugin.wasm",
        permissions: { network_hosts: [] },
      },
      granted: {},
      enabled: false,
      installed_at: now(),
      signed: false,
    },
  ];

  constructor() {
    this.snapshot = demoSnapshot();
    this.settings = clone(this.snapshot.fleet.settings);
    this.bootstrap = demoBootstrap();
  }

  private accounts() {
    return this.snapshot.fleet.accounts;
  }
  private lanes() {
    return this.snapshot.fleet.lanes;
  }
  private live() {
    return this.snapshot.live;
  }
  private account(id: string): Account | undefined {
    return this.accounts()[id];
  }
  private saveUndo() {
    this.undoStack.push(JSON.stringify({ snapshot: this.snapshot, settings: this.settings }));
    if (this.undoStack.length > 30) this.undoStack.shift();
    this.redoStack = [];
  }
  private bump() {
    this.snapshot.fleet.revision += 1;
    this.snapshot.taken_at = now();
    this.snapshot.fleet.settings = this.settings;
    this.bootstrap.snapshot = this.snapshot;
    this.bootstrap.settings = this.settings;
  }
  private notice(title: string, body: string, severity: Notice["severity"] = "info") {
    this.snapshot.fleet.notices.unshift({
      id: uuid(),
      severity,
      title,
      body,
      at: now(),
      read: false,
      dismissed: false,
      dedupe_key: null,
      action: null,
    });
  }
  private recalc() {
    const live = this.live();
    this.snapshot.totals = {
      running: live.filter((c) => ["in_game", "launching", "disconnected", "restarting"].includes(c.state)).length,
      cpu_pct: live.reduce((sum, c) => sum + c.cpu_pct, 0),
      mem_mb: live.reduce((sum, c) => sum + c.mem_mb, 0),
      restarts: live.reduce((sum, c) => sum + c.restarts, 0),
      crashes: live.reduce((sum, c) => sum + (c.last_reason?.startsWith("Crashed") ? 1 : 0), 0),
    };
    this.bump();
  }
  private requireVault() {
    if (this.vaultPassphrase === null) {
      const error = new Error("vault is locked");
      (error as any).code = "vault_locked";
      throw error;
    }
  }
  private launchOne(account: Account, target: string, profile?: LaunchProfile) {
    this.requireVault();
    if (this.live().some((c) => c.account_id === account.id && ["launching", "in_game", "restarting"].includes(c.state))) {
      return { account: account.username, error: "Already running" };
    }
    const pid = 23000 + Math.floor(Math.random() * 20000);
    const id = uuid();
    const client = {
      id,
      account_id: account.id,
      lane_id: account.lane_id,
      profile_id: profile?.id ?? null,
      state: "in_game" as ClientState,
      pid,
      hwnd: 3400000 + Math.floor(Math.random() * 9000),
      launched_at: now(),
      state_since: now(),
      session_started_at: now(),
      playtime_seconds: account.playtime_seconds,
      restart_count: 0,
      crash_count: 0,
      consecutive_failures: 0,
      exit_code: null,
      last_reason: null,
      server_job_id: `${Math.random().toString(16).slice(2, 10)}-demo`,
      target,
      limits: profile?.limits ?? defaultSettings() as any,
      memory_mb: 280 + Math.floor(Math.random() * 900),
      cpu_pct: Math.round((2 + Math.random() * 9) * 10) / 10,
    };
    this.snapshot.fleet.clients[id] = client as any;
    this.live().push({
      client_id: id,
      account_id: account.id,
      state: "in_game",
      pid,
      hwnd: client.hwnd,
      cpu_pct: client.cpu_pct,
      mem_mb: client.memory_mb,
      threads: 42,
      handles: 900,
      started_at: now(),
      state_since: now(),
      target,
      restarts: 0,
      last_reason: null,
    });
    this.recalc();
    return { account: account.username, error: null };
  }

  async invoke<T>(command: string, args: any = {}): Promise<T> {
    await new Promise((resolve) => setTimeout(resolve, 8));
    const result = await this.handle(command, args);
    return result as T;
  }

  private async handle(command: string, args: any): Promise<any> {
    switch (command) {
      case "bootstrap":
        return this.bootstrap;
      case "fleet_snapshot":
        return this.snapshot;
      case "save_settings": {
        this.saveUndo();
        this.settings = { ...this.settings, ...args.settings };
        this.bump();
        return this.settings;
      }
      case "add_account": {
        this.requireVault();
        this.saveUndo();
        const input = args.input ?? {};
        const username = String(input.username ?? "").trim();
        if (!/^[A-Za-z0-9_]{3,32}$/.test(username)) throw new Error("Username must be 3–32 letters, numbers or underscores.");
        if (Object.values(this.accounts()).some((a) => a.username.toLowerCase() === username.toLowerCase())) throw new Error("That account is already in the fleet.");
        const id = uuid();
        const lane = input.lane_id && this.lanes()[input.lane_id] ? input.lane_id : this.lanes()[LANE_IDS.farming].id;
        const account: Account = {
          ...clone(Object.values(this.accounts())[0]),
          id,
          username,
          display_name: input.display_name || username,
          lane_id: lane,
          alias: input.alias ?? null,
          notes: input.notes ?? "",
          tags: input.tags ?? [],
          favourite: false,
          robux: 0,
          premium: false,
          friend_count: 0,
          follower_count: 0,
          playtime_seconds: 0,
          sessions: 0,
          health: { score: 70, grade: "fair", issues: [{ code: "unvalidated", label: "Validate the cookie" }] },
          login: { kind: "unknown", checked_at: null, detail: "Added just now" },
          created_at: now(),
          updated_at: now(),
          cookie_fingerprint: "demo-fingerprint",
        };
        this.accounts()[id] = account;
        this.bump();
        return account;
      }
      case "remove_account": {
        this.saveUndo();
        delete this.accounts()[args.id];
        this.recalc();
        return true;
      }
      case "update_account_fields": {
        this.saveUndo();
        const account = this.account(args.id);
        if (!account) throw new Error("Account not found");
        Object.assign(account, args.patch ?? {});
        account.updated_at = now();
        this.bump();
        return account;
      }
      case "move_account": {
        this.saveUndo();
        const account = this.account(args.id);
        if (!account) throw new Error("Account not found");
        if (!this.lanes()[args.to]) throw new Error("Lane not found");
        account.lane_id = args.to;
        account.updated_at = now();
        this.bump();
        return true;
      }
      case "add_lane": {
        this.saveUndo();
        const lane: Lane = {
          id: uuid(),
          name: args.name,
          color: args.color ?? "#6c8cff",
          order: Object.keys(this.lanes()).length,
          builtin: false,
          created_at: now(),
          rules: clone(Object.values(this.lanes())[0].rules),
        };
        this.lanes()[lane.id] = lane;
        this.bump();
        return lane;
      }
      case "set_lane_rules": {
        this.saveUndo();
        const lane = this.lanes()[args.laneId];
        if (!lane) throw new Error("Lane not found");
        lane.rules = args.rules;
        this.bump();
        return true;
      }
      case "remove_lane": {
        this.saveUndo();
        const lane = this.lanes()[args.id];
        if (!lane) throw new Error("Lane not found");
        if (Object.values(this.accounts()).some((a) => a.lane_id === args.id) && !args.moveTo) throw new Error("Move accounts first");
        for (const account of Object.values(this.accounts())) if (account.lane_id === args.id) account.lane_id = args.moveTo;
        delete this.lanes()[args.id];
        this.bump();
        return true;
      }
      case "save_profile": {
        this.saveUndo();
        this.snapshot.fleet.profiles[args.profile.id] = args.profile;
        this.bump();
        return args.profile;
      }
      case "remove_profile": {
        this.saveUndo();
        delete this.snapshot.fleet.profiles[args.id];
        this.bump();
        return true;
      }
      case "parse_target": {
        const raw = String(args.input ?? "").trim();
        let place: number | null = null;
        const match = raw.match(/(?:games|place)\/(\d+)/) ?? raw.match(/^(\d{3,})$/);
        if (match) place = Number(match[1]);
        return {
          raw,
          kind: place ? "place" : raw.startsWith("@") ? "profile" : "raw",
          place_id: place,
          game_instance_id: null,
          private_server_code: null,
          share_code: null,
          user_id: null,
          username: raw.startsWith("@") ? raw.slice(1) : null,
          confidence: place ? 0.95 : 0.4,
          warnings: place ? [] : ["Pick a game link or place ID before launching."],
          needs_resolution: !place,
          summary: place ? `Place ${place}` : "Unrecognized target",
          launchable: !!place,
        };
      }
      case "launch": {
        const target = args.target || args.profile?.target || "";
        const profile = args.profile ?? (args.profileId ? this.snapshot.fleet.profiles[args.profileId] : undefined) ?? Object.values(this.snapshot.fleet.profiles)[0];
        const accountIds: string[] = args.accounts ?? [];
        const outcomes = accountIds.map((id) => {
          const account = this.account(id);
          if (!account) return { account: id, error: "Account not found" };
          return this.launchOne(account, target, profile);
        });
        this.notice("Launch requested", `${outcomes.length} account(s) queued for ${target || "their lane target"}.`, "success");
        return { batch_id: uuid(), outcomes: outcomes.map((o) => ({ account_id: args.accounts[outcomes.indexOf(o)], state: o.error ? "failed" : "in_game", error: o.error })), warnings: [] };
      }
      case "kill_client": {
        this.saveUndo();
        const live = this.live();
        const index = live.findIndex((c) => c.client_id === args.clientId);
        if (index >= 0) {
          const clientId = live[index].client_id;
          live.splice(index, 1);
          const runtime = this.snapshot.fleet.clients[clientId];
          if (runtime) {
            runtime.state = "stopped";
            runtime.state_since = now();
          }
        }
        this.recalc();
        return true;
      }
      case "kill_all": {
        this.saveUndo();
        for (const c of this.live()) {
          const runtime = this.snapshot.fleet.clients[c.client_id];
          if (runtime) {
            runtime.state = "stopped";
            runtime.state_since = now();
          }
        }
        this.snapshot.live = [];
        this.notice("Fleet stopped", "All clients were stopped.", "warning");
        this.recalc();
        return this.snapshot.totals.running;
      }
      case "focus_client":
      case "set_client_limits":
        return true;
      case "validate_accounts": {
        this.saveUndo();
        for (const id of args.ids ?? []) {
          const account = this.account(id);
          if (!account) continue;
          account.login = { kind: "valid", checked_at: now(), detail: null };
          account.last_validated = now();
          account.health = { score: 94, grade: "excellent", issues: [] };
          account.robux = (account.robux ?? 0) + Math.floor(Math.random() * 50);
          account.presence = "Online";
        }
        this.bump();
        return { validated: (args.ids ?? []).length, failed: 0, details: [] };
      }
      case "vault_status":
        return { ...this.snapshot.vault, unlocked: this.vaultPassphrase !== null };
      case "create_vault":
      case "unlock_vault": {
        if (String(args.passphrase ?? "").length < 8) throw new Error("Use at least 8 characters.");
        this.vaultPassphrase = args.passphrase;
        this.snapshot.vault.unlocked = true;
        this.snapshot.vault.exists = true;
        this.audit.push({ seq: this.audit.length + 1, at: now(), kind: "unlock", subject: "vault", detail: "passphrase", prev_hash: "demo", hash: uuid().slice(0, 8), severity: "info" });
        this.bump();
        return this.snapshot.vault;
      }
      case "lock_vault": {
        this.vaultPassphrase = null;
        this.snapshot.vault.unlocked = false;
        this.bump();
        return true;
      }
      case "reauthenticate":
        return args.passphrase === this.vaultPassphrase;
      case "reveal_cookie":
        this.requireVault();
        return "_|WARNING:-DO-NOT-SHARE|_|demo-cookie-not-real-" + "A".repeat(120);
      case "copy_cookie":
        this.requireVault();
        return true;
      case "issue_recovery_code":
        return "ROFARM-DEMO-RECOVERY";
      case "change_passphrase": {
        if (args.current !== this.vaultPassphrase) throw new Error("Current passphrase is incorrect.");
        this.vaultPassphrase = args.next;
        return true;
      }
      case "backup_vault":
        return { bytes: 2_414_000, chunks: 24, files: [{ path: "vault.rofarmvault", size: 2_380_000 }], destination: args.destination };
      case "verify_backup":
        return { ok: true, bytes: 2_414_000, files: 2 };
      case "restore_backup":
        return { restored_bytes: 2_414_000, locked: true };
      case "undo": {
        const previous = this.undoStack.pop();
        if (!previous) return null;
        this.redoStack.push(JSON.stringify({ snapshot: this.snapshot, settings: this.settings }));
        const parsed = JSON.parse(previous);
        this.snapshot = parsed.snapshot;
        this.settings = parsed.settings;
        this.bump();
        return "Undone";
      }
      case "redo": {
        const next = this.redoStack.pop();
        if (!next) return null;
        this.undoStack.push(JSON.stringify({ snapshot: this.snapshot, settings: this.settings }));
        const parsed = JSON.parse(next);
        this.snapshot = parsed.snapshot;
        this.settings = parsed.settings;
        this.bump();
        return "Redone";
      }
      case "undo_state":
        return { can_undo: this.undoStack.length > 0, can_redo: this.redoStack.length > 0, undo_label: this.undoStack.length ? "Last action" : null, redo_label: this.redoStack.length ? "Redo" : null };
      case "save_scene": {
        this.saveUndo();
        this.snapshot.fleet.scenes[args.scene.id] = args.scene;
        this.bump();
        return args.scene;
      }
      case "apply_scene": {
        this.saveUndo();
        this.notice("Scene applied", "Accounts and lanes were restored from the scene.", "success");
        this.bump();
        return 4;
      }
      case "capture_scene": {
        return { id: uuid(), name: args.name ?? "Scene", description: "", snapshot: { accounts: Object.values(this.accounts()).length }, hotkey: null, created_at: now() };
      }
      case "save_automation": {
        this.saveUndo();
        this.snapshot.fleet.automations[args.automation.id] = args.automation;
        this.bump();
        return args.automation;
      }
      case "delete_automation": {
        this.saveUndo();
        delete this.snapshot.fleet.automations[args.id];
        this.bump();
        return true;
      }
      case "run_automation": {
        const automation = this.snapshot.fleet.automations[args.id] as Automation;
        if (!automation) throw new Error("Automation not found");
        const run = {
          id: uuid(),
          automation_id: automation.id,
          automation_name: automation.name,
          at: now(),
          trigger_summary: "Manual",
          status: args.dryRun || automation.dry_run ? "dry_run" : "succeeded",
          steps: automation.actions.map((action) => ({
            action,
            status: args.dryRun || automation.dry_run ? "dry_run" : "succeeded",
            detail: args.dryRun || automation.dry_run ? "Would run" : "Executed",
            duration_ms: 40,
          })),
          dry_run: !!args.dryRun || automation.dry_run,
        };
        this.snapshot.fleet.automation_runs.unshift(run);
        automation.last_run = now();
        automation.run_count += 1;
        this.notice("Automation run", `${automation.name}: ${run.status.replace("_", " ")}`, "info");
        this.bump();
        return run;
      }
      case "automation_yaml":
        return this.snapshot.fleet.automations;
      case "import_automation_yaml":
        return { imported: 0, errors: [] };
      case "eval_console":
        return String(args.source).includes("accounts") ? String(Object.keys(this.accounts()).length) : "ok";
      case "list_servers":
        return demoServers.map((s) => ({ job_id: s.id, playing: s.players, max_players: s.max_players, ping: s.ping_ms, fps: 60, region: s.region }));
      case "fetch_friends":
        return this.friends;
      case "fetch_inventory":
        return this.inventory;
      case "fetch_groups":
        return [
          { id: 1, name: "RoFarm Testers", role: "Member", rank: 1, members: 1200 },
          { id: 2, name: "Adopt Me Traders", role: "Officer", rank: 120, members: 84_000 },
        ];
      case "fetch_avatar":
        return { outfits: [{ id: "o1", name: "Farming fit", updated_at: now() }, { id: "o2", name: "Trade fit", updated_at: now() }] };
      case "apply_outfit":
        return true;
      case "audit_log":
        return this.audit;
      case "network_log":
        return this.network;
      case "remote_status":
        return { running: this.remoteRunning, address: "127.0.0.1:47810", scheme: "https", fingerprint: this.remoteRunning ? "8f:3a:92:cc:01:44:de:91:ab:02:34:fe:10:22:9a:7c" : null, devices: this.remoteRunning ? [{ id: "d1", name: "Phone", created_at: now(), revoked: false }] : [] };
      case "remote_start":
        this.remoteRunning = true;
        return { running: true, address: "127.0.0.1:47810", scheme: "https", fingerprint: "8f:3a:92:cc:01:44:de:91:ab:02:34:fe:10:22:9a:7c", devices: [] };
      case "remote_stop":
        this.remoteRunning = false;
        return { running: false, devices: [] };
      case "plugins_list":
        return this.plugins;
      case "plugin_toggle": {
        const plugin = this.plugins.find((p) => p.manifest.id === args.id);
        if (plugin) plugin.enabled = args.enabled;
        return true;
      }
      case "plugin_grant": {
        const plugin = this.plugins.find((p) => p.manifest.id === args.id);
        if (plugin) plugin.granted = args.permissions;
        return true;
      }
      case "update_status":
        return { channel: this.settings.updates?.channel ?? "stable", current_version: "0.1.0", available: null, checked_at: now(), verified: true, staged: false };
      case "check_updates":
        return { channel: this.settings.updates?.channel ?? "stable", current_version: "0.1.0", available: null, checked_at: now(), verified: true, staged: false };
      case "install_update":
        throw new Error("Updates are only available in the desktop build.");
      case "doctor":
        return {
          at: now(),
          score: 96,
          checks: [
            { id: "data_dir", label: "Data directory", status: "pass", detail: "Writable, ACL restricted.", fix: "" },
            { id: "vault", label: "Vault", status: this.vaultPassphrase ? "pass" : "warn", detail: this.vaultPassphrase ? "Unlocked." : "Locked — normal when idle.", fix: "Unlock in Vault." },
            { id: "event_chain", label: "Event log integrity", status: "pass", detail: "Hash chain verified.", fix: "" },
            { id: "roblox", label: "Roblox client", status: "pass", detail: "version-8f3c2b1a found.", fix: "" },
            { id: "webview2", label: "WebView2", status: "pass", detail: "Evergreen runtime 131.", fix: "" },
            { id: "backup", label: "Last backup", status: "warn", detail: "12 hours ago.", fix: "Create one now." },
          ],
        };
      case "open_external":
        if (typeof window !== "undefined") window.open(args.url, "_blank", "noopener");
        return true;
      case "export_sessions_csv":
        return this.csvSessions();
      case "export_diagnostics":
        return JSON.stringify({ app: "RoFarm", version: "0.1.0", snapshot: this.snapshot, redacted: true }, null, 2);
      case "window_minimize":
      case "window_maximize":
      case "window_close":
      case "show_mini":
      case "show_main":
      case "set_always_on_top":
      case "set_thumbnail_visible":
        return true;
      case "capture_thumbnail":
        return null;
      case "screenshot_client":
        this.notice("Screenshot saved", "Client window captured to the screenshots folder.", "success");
        return { path: "screenshots/demo.png" };
      case "save_game": {
        this.saveUndo();
        this.snapshot.fleet.games[args.game.id] = args.game;
        this.bump();
        return args.game;
      }
      case "remove_game": {
        this.saveUndo();
        delete this.snapshot.fleet.games[args.id];
        this.bump();
        return true;
      }
      case "remove_notice": {
        const notice = this.snapshot.fleet.notices.find((n) => n.id === args.id);
        if (notice) notice.dismissed = true;
        this.bump();
        return true;
      }
      case "mark_notices_read": {
        for (const notice of this.snapshot.fleet.notices) notice.read = true;
        this.bump();
        return true;
      }
      case "save_notifier":
      case "test_notifier":
        return true;
      case "export_settings":
        return JSON.stringify(this.settings, null, 2);
      case "import_settings": {
        this.settings = { ...this.settings, ...JSON.parse(args.json) };
        this.bump();
        return this.settings;
      }
      case "export_theme":
        return JSON.stringify(this.settings.theme, null, 2);
      case "import_theme": {
        const parsed = JSON.parse(args.json);
        this.settings.theme = { ...this.settings.theme, custom: parsed };
        this.bump();
        return this.settings;
      }
      case "list_workspaces":
        return [
          { id: "default", name: "Personal", theme: "obsidian", accounts: Object.keys(this.accounts()).length },
          { id: "trading", name: "Trading desk", theme: "aurora", accounts: 4 },
        ];
      case "switch_workspace":
        return true;
      case "clean_logs":
        this.notice("Logs cleaned", "Rotated RoFarm logs and removed old Roblox log files.", "success");
        return { freed_mb: 128 };
      case "open_logs":
        return true;
      case "apply_limits":
        return true;
      case "save_layout":
        return args.layout;
      case "auto_tile":
        this.notice("Windows tiled", "All client windows arranged on the current monitor.", "info");
        return true;
      case "start_anti_afk":
      case "stop_anti_afk":
        return true;
      case "shutdown_timer":
        return true;
      case "theme_list":
        return [];
      case "system_info":
        return { os: "Windows 11 24H2", cpu: "AMD Ryzen 7 7800X3D", gpu: "NVIDIA RTX 4070", ram_mb: 32768, cores: 16, roblox_installed: true, webview2: "131.0.2903.86" };
      default:
        throw new Error(`Demo bridge does not implement "${command}" yet.`);
    }
  }

  private csvSessions() {
    const header = "account,client,started,ended,minutes,outcome,target";
    const rows = this.snapshot.fleet.sessions.map((session) => {
      const account = this.account(session.account_id);
      return [account?.username ?? session.account_id, session.client_id, session.started_at, session.ended_at, Math.round(session.seconds / 60), session.outcome, session.target ?? ""]
        .map((value) => `"${String(value).replace(/"/g, '""')}"`)
        .join(",");
    });
    return [header, ...rows].join("\n");
  }
}

export const demoBackend = new DemoBackend();
export type { ServerInstance };
