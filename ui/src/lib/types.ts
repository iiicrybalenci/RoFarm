export type Id = string;

export type ViewId =
  | "board"
  | "fleet"
  | "timeline"
  | "servers"
  | "automations"
  | "inventory"
  | "friends"
  | "vault"
  | "metrics"
  | "logs"
  | "settings";

export type Density = "comfortable" | "compact" | "ultra_compact";
export type ClientState = "queued" | "launching" | "in_game" | "disconnected" | "restarting" | "failed" | "stopped";

export interface HealthIssue {
  code: string;
  label: string;
  fix?: string;
}

export interface Account {
  id: Id;
  roblox_user_id?: number | null;
  username: string;
  display_name: string;
  avatar_url?: string | null;
  lane_id: Id;
  alias?: string | null;
  notes: string;
  tags: string[];
  color: string;
  favourite: boolean;
  custom_fields: Record<string, string>;
  created_at: string;
  updated_at: string;
  last_seen?: string | null;
  last_validated?: string | null;
  cookie_expires_at?: string | null;
  cookie_fingerprint?: string | null;
  presence: string;
  robux?: number | null;
  premium: boolean;
  friend_count: number;
  follower_count: number;
  two_step: unknown;
  account_age_days?: number | null;
  health: { score: number; grade: string; issues: HealthIssue[] };
  login: { kind: string; checked_at?: string | null; detail?: string | null };
  playtime_seconds: number;
  sessions: number;
  limits?: ResourceLimits | null;
}

export interface ResourceLimits {
  cpu_percent?: number | null;
  memory_mb?: number | null;
  trim_memory: boolean;
  eco_qos: boolean;
  priority: string;
  io_priority: string;
  gpu: string;
  fps_cap?: number | null;
  graphics: string;
  upload_kbps?: number | null;
  cores: number[];
  affinity_mask?: number | null;
  auto_throttle_memory: boolean;
  background_fps_cap?: number | null;
}

export interface AntiAfk {
  enabled: boolean;
  mode: "foreground" | "background";
  interval_seconds: number;
  jitter_percent: number;
  opted_out_accounts: Id[];
}

export interface LaneRules {
  default_target?: string | null;
  default_profile?: Id | null;
  limits: ResourceLimits;
  alerts: unknown;
  schedule?: unknown;
  rotation?: { keep_running: number; max_session_minutes: number; swap_cooldown_seconds: number; order: string } | null;
  max_running?: number | null;
  anti_afk: AntiAfk;
  stop_on_lock: boolean;
  allowed_to_stay_disconnected: boolean;
  auto_rejoin: boolean;
  restart_on_crash: boolean;
}

export interface Lane {
  id: Id;
  name: string;
  color: string;
  order: number;
  builtin: boolean;
  rules: LaneRules;
  created_at: string;
}

export interface ClientRuntime {
  id: Id;
  account_id: Id;
  lane_id: Id;
  profile_id?: Id | null;
  state: ClientState;
  pid?: number | null;
  hwnd?: number | null;
  launched_at?: string | null;
  state_since: string;
  session_started_at?: string | null;
  playtime_seconds: number;
  restart_count: number;
  crash_count: number;
  consecutive_failures: number;
  exit_code?: number | null;
  last_reason?: string | null;
  server_job_id?: string | null;
  target?: string | null;
  limits: ResourceLimits;
  memory_mb: number;
  cpu_pct: number;
}

export interface LiveClient {
  client_id: Id;
  account_id: Id;
  state: ClientState;
  pid?: number | null;
  hwnd?: number | null;
  cpu_pct: number;
  mem_mb: number;
  threads: number;
  handles: number;
  started_at?: string | null;
  state_since: string;
  target?: string | null;
  restarts: number;
  last_reason?: string | null;
}

export interface SessionRecord {
  client_id: Id;
  account_id: Id;
  started_at: string;
  ended_at: string;
  seconds: number;
  outcome: string;
  target?: string | null;
}

export interface Notice {
  id: string;
  severity: "info" | "success" | "warning" | "critical";
  title: string;
  body: string;
  at: string;
  read: boolean;
  dismissed: boolean;
  dedupe_key?: string | null;
  action?: { id: string; label: string } | null;
}

export interface LaunchProfile {
  id: Id;
  name: string;
  target: string;
  args: string;
  fps_cap?: number | null;
  graphics: string;
  limits: ResourceLimits;
  window: WindowPlacement;
  server_mode: "any" | "same_server" | "follow_friend";
  follow_username?: string | null;
  stagger_ms: number;
  cooldown_ms: number;
  retry: { max_attempts: number; base_backoff_secs: number; max_backoff_secs: number; retry_on_crash: boolean; retry_on_disconnect: boolean };
  anti_afk_override?: boolean | null;
  created_at: string;
  updated_at: string;
}

export interface WindowPlacement {
  mode: string;
  monitor?: string | null;
  bounds?: { x: number; y: number; width: number; height: number } | null;
  always_on_top: boolean;
  borderless: boolean;
  virtual_desktop?: number | null;
  micro: boolean;
}

export interface Scene {
  id: Id;
  name: string;
  description: string;
  snapshot: unknown;
  hotkey?: string | null;
  created_at: string;
}

export interface Automation {
  id: Id;
  name: string;
  description: string;
  enabled: boolean;
  trigger: Record<string, any>;
  conditions: Record<string, any>[];
  actions: Record<string, any>[];
  cooldown_seconds: number;
  daily_budget: number;
  dry_run: boolean;
  schedule?: unknown;
  last_run?: string | null;
  run_count: number;
  created_at: string;
  updated_at: string;
  stop_on_error: boolean;
}

export interface WindowLayout {
  id: Id;
  name: string;
  kind: string;
  monitor?: string | null;
  slots: Record<string, { x: number; y: number; width: number; height: number }>;
  created_at: string;
}

export interface GameEntry {
  id: Id;
  place_id: number;
  universe_id?: number | null;
  name: string;
  description: string;
  thumbnail_url?: string | null;
  tags: string[];
  notes: string;
  default_profile_id?: Id | null;
  playtime_seconds: number;
  favourite: boolean;
  created_at: string;
  updated_at: string;
}

export interface PrivateServer {
  id: Id;
  place_id: number;
  name: string;
  link_code: string;
  link: string;
  owner_account_id?: Id | null;
  expires_at?: string | null;
  notes: string;
  created_at: string;
}

export interface ServerInstance {
  job_id: string;
  playing: number;
  max_players: number;
  ping?: number | null;
  fps?: number | null;
  region?: string;
}

export interface FriendlyServer {
  id: string;
  place_id: number;
  name: string;
  players: number;
  max_players: number;
  ping_ms: number;
  region: string;
  bookmarked: boolean;
}

export interface Friend {
  id: number;
  username: string;
  display_name: string;
  online: boolean;
  place?: string | null;
  mutual?: boolean;
  accounts: Id[];
}

export interface InventoryItem {
  asset_id: number;
  name: string;
  asset_type: string;
  rarity?: string | null;
  value_robux?: number | null;
  acquired_at?: string | null;
  account_id: Id;
}

export interface ThemeSettings {
  builtin: string;
  custom?: ThemeDefinition | null;
  accent_sync: boolean;
  accent_override?: string | null;
  material: string;
  reduce_transparency: boolean;
  background_dim: number;
}

export interface ThemeDefinition {
  schema_version: number;
  id: string;
  name: string;
  author: string;
  mode: string;
  colors: Record<string, string>;
  radius_px: number;
  font_family?: string | null;
  background_image?: string | null;
  background_dim: number;
  background_blur: number;
}

export interface Settings {
  schema_version: number;
  density: Density;
  language: string;
  ui_scale: number;
  theme: ThemeSettings;
  shortcuts: Record<string, string>;
  resource_profile: "eco" | "balanced" | "performance" | "custom";
  eco_mode: boolean;
  eco_cpu_temp_c: number;
  eco_battery_percent: number;
  performance: {
    refresh_ms: number;
    thumbnails: boolean;
    thumbnail_fps: number;
    sparklines: boolean;
    metrics_interval_ms: number;
    profiler_visible: boolean;
    pause_when_hidden: boolean;
    reduce_motion: string;
    pause_animations_on_battery: boolean;
  };
  security: Record<string, any>;
  notifications: Record<string, any>;
  privacy: Record<string, any>;
  windows: Record<string, any>;
  backups: Record<string, any>;
  updates: Record<string, any>;
  remote: Record<string, any>;
  integrations: Record<string, any>;
  onboarding: Record<string, any>;
  start_with_windows: boolean;
  auto_shutdown_minutes?: number | null;
  theme_packs: ThemeDefinition[];
  custom_tags: string[];
  experimental: Record<string, boolean>;
  show_network_log: boolean;
  locale_override?: string | null;
  [key: string]: any;
}

export interface VaultStatus {
  path: string;
  exists: boolean;
  unlocked: boolean;
  generation: number;
  secrets: number;
  has_recovery: boolean;
  lockout_seconds?: number | null;
}

export interface AuditEntry {
  seq: number;
  at: string;
  kind: string;
  subject: string;
  detail: string;
  prev_hash: string;
  hash: string;
  severity?: string;
}

export interface NetworkEntry {
  at: string;
  method: string;
  host: string;
  path: string;
  status?: number | null;
  duration_ms: number;
  request_bytes: number;
  response_bytes: number;
  allowed: boolean;
  reason: string;
}

export interface FleetState {
  revision: number;
  accounts: Record<Id, Account>;
  lanes: Record<Id, Lane>;
  clients: Record<Id, ClientRuntime>;
  profiles: Record<Id, LaunchProfile>;
  scenes: Record<Id, Scene>;
  automations: Record<Id, Automation>;
  automation_runs: any[];
  layouts: Record<Id, WindowLayout>;
  games: Record<Id, GameEntry>;
  server_bookmarks: Record<Id, unknown>;
  private_servers: Record<Id, PrivateServer>;
  notices: Notice[];
  settings: Settings;
  economy: unknown;
  sessions: SessionRecord[];
  crash_times: Record<string, string[]>;
  last_event_at?: string | null;
}

export interface Snapshot {
  fleet: FleetState;
  live: LiveClient[];
  totals: { running: number; cpu_pct: number; mem_mb: number; restarts: number; crashes: number };
  vault: VaultStatus;
  roblox_clients: { version: string; path: string }[];
  options: Record<string, any>;
  taken_at: string;
}

export interface Bootstrap {
  snapshot: Snapshot;
  settings: Settings;
  version: string;
  data_dir: string;
  portable: boolean;
  first_run: boolean;
  platform: string;
  demo: boolean;
  shortcuts: Record<string, string>;
}

export interface ThemePalette {
  id: string;
  name: string;
  mode: "dark" | "light";
  accent: string;
  second: string;
  bg: string;
  bg2: string;
  panel: string;
  panel2: string;
  line: string;
  text: string;
  muted: string;
  ok: string;
  warn: string;
  danger: string;
  info: string;
  radius: number;
}

export interface Toast {
  id: number;
  title: string;
  message?: string;
  kind: "info" | "success" | "warning" | "error";
}

export interface DoctorReport {
  at: string;
  checks: { id: string; label: string; status: "pass" | "warn" | "fail"; detail: string; fix: string }[];
  score: number;
}

export interface UpdateStatus {
  channel: string;
  current_version: string;
  available?: { version: string; notes: string; size_bytes: number; published_at: string } | null;
  checked_at?: string | null;
  verified: boolean;
  staged: boolean;
}
