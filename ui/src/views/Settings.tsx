import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { Modal, Section, Segmented, Switch, ToggleRow } from "../components/Primitives";
import { Icon, type IconName } from "../components/Icon";
import { palettes } from "../lib/themes";
import { languages } from "../lib/i18n";
import { downloadText, formatDate, formatDuration } from "../lib/format";
import { api, invoke, pickFile, saveFile } from "../lib/api";
import { bootstrap, refresh, settings, settingsSection, toast, updateSettings } from "../lib/store";
import type { UpdateStatus } from "../lib/types";

type SectionId = "appearance" | "performance" | "notifications" | "integrations" | "windows" | "data" | "privacy" | "updates" | "shortcuts" | "plugins" | "about";

const SECTIONS: { id: SectionId; label: string; icon: IconName; group: string }[] = [
  { id: "appearance", label: "Appearance", icon: "palette", group: "App" },
  { id: "performance", label: "Performance", icon: "cpu", group: "App" },
  { id: "notifications", label: "Notifications", icon: "bell", group: "App" },
  { id: "integrations", label: "Integrations", icon: "plugin", group: "App" },
  { id: "windows", label: "Windows", icon: "monitor", group: "System" },
  { id: "data", label: "Data & workspaces", icon: "database", group: "System" },
  { id: "privacy", label: "Privacy & security", icon: "shield", group: "System" },
  { id: "updates", label: "Updates", icon: "download", group: "System" },
  { id: "shortcuts", label: "Shortcuts", icon: "keyboard", group: "Advanced" },
  { id: "plugins", label: "Plugins", icon: "code", group: "Advanced" },
  { id: "about", label: "About", icon: "info", group: "Advanced" },
];

const EVENTS = [
  ["crashed", "Client crashed"],
  ["disconnected", "Client disconnected"],
  ["rejoined", "Client rejoined"],
  ["launched", "Client launched"],
  ["validated", "Account validated"],
  ["validation_failed", "Validation failed"],
  ["friend_online", "Friend online"],
  ["robux_changed", "Robux changed"],
  ["backup", "Backup finished"],
  ["update", "Update available"],
  ["automation", "Automation ran"],
  ["low_battery", "Battery low"],
];

export default function Settings() {
  const [section, setSection] = createSignal<SectionId>((settingsSection() as SectionId) ?? "appearance");
  const [themeJsonOpen, setThemeJsonOpen] = createSignal(false);
  const [themeJson, setThemeJson] = createSignal("");
  const [remoteOpen, setRemoteOpen] = createSignal(false);
  const [remotePin, setRemotePin] = createSignal("");
  const [update, setUpdate] = createSignal<UpdateStatus | null>(null);
  const [workspaces, setWorkspaces] = createSignal<{ id: string; name: string; path: string; active: boolean }[]>([]);
  const [plugins, setPlugins] = createSignal<any[]>([]);
  const [checking, setChecking] = createSignal(false);

  const s = () => settings();
  const groups = createMemo(() => [...new Set(SECTIONS.map((x) => x.group))]);

  onMount(async () => {
    try {
      setUpdate(await api.updateStatus());
      setWorkspaces(await invoke("list_workspaces", {}));
      setPlugins(await api.plugins());
    } catch {
    }
  });

  const pickTheme = (id: string) => {
    updateSettings((draft) => { draft.theme.builtin = id; draft.theme.custom = null; });
    toast("Theme applied", palettes[id]?.name ?? id, "success");
  };

  const exportTheme = async () => {
    const payload = { schema_version: 1, name: s().theme.custom?.name ?? palettes[s().theme.builtin]?.name ?? "Custom", theme: s().theme };
    const path = await saveFile({ defaultPath: "rofarm-theme.json", filters: [{ name: "Theme", extensions: ["json"] }], contents: JSON.stringify(payload, null, 2) });
    if (path) toast("Theme exported", path, "success");
  };

  const importTheme = async () => {
    const file = await pickFile({ filters: [{ name: "Theme", extensions: ["json"] }] });
    if (typeof file !== "string") return;
    try {
      const parsed = JSON.parse(file);
      const theme = parsed.theme ?? parsed;
      if (!theme?.colors) throw new Error("That file has no theme colours.");
      updateSettings((draft) => {
        draft.theme.custom = {
          schema_version: 1,
          id: theme.id ?? "imported",
          name: theme.name ?? "Imported theme",
          author: theme.author ?? "You",
          mode: theme.mode === "light" ? "light" : "dark",
          colors: theme.colors,
          radius_px: theme.radius_px ?? 12,
          background_image: theme.background_image ?? null,
          background_dim: theme.background_dim ?? 0.4,
          background_blur: theme.background_blur ?? 0,
          font_family: null,
        };
      });
      toast("Theme imported", theme.name ?? "Imported theme", "success");
    } catch (error: any) {
      toast("Could not import theme", String(error?.message ?? error), "error");
    }
  };

  const saveThemeCustom = (key: string, value: string) => {
    updateSettings((draft) => {
      const base = draft.theme.custom ?? {
        schema_version: 1,
        id: "custom",
        name: "My theme",
        author: "You",
        mode: palettes[draft.theme.builtin]?.mode ?? "dark",
        colors: { ...(palettes[draft.theme.builtin] ?? palettes.obsidian) },
        radius_px: 12,
        background_dim: 0.4,
        background_blur: 0,
        background_image: null,
        font_family: null,
      };
      draft.theme.custom = { ...base, colors: { ...base.colors, [key]: value } };
    });
  };

  const checkUpdates = async () => {
    setChecking(true);
    try {
      setUpdate(await api.checkUpdates());
      toast("Update check complete", update()?.available ? `Version ${update()!.available!.version} is available.` : "You're on the latest stable build.", "success");
    } catch (error: any) {
      toast("Update check failed", String(error?.message ?? error), "error");
    } finally {
      setChecking(false);
    }
  };

  const startRemote = async () => {
    try {
      await api.remoteStart({ pin: remotePin(), bind: "0.0.0.0", port: 47810, tls: true });
      setRemoteOpen(false);
      toast("Remote dashboard started", "Pair a device with the PIN shown in the dashboard. Firewall changes only happen on your click.", "success");
    } catch (error: any) {
      toast("Could not start remote dashboard", String(error?.message ?? error), "error");
    }
  };

  const setNotifier = (id: string, patch: Record<string, unknown>) => {
    updateSettings((draft) => {
      const notify = (draft.notifications ??= {});
      const notifiers = (notify.channels ??= {});
      notifiers[id] = { ...(notifiers[id] ?? {}), ...patch };
    });
  };

  const notifier = (id: string) => s().notifications?.channels?.[id] ?? {};

  return (
    <div class="page settings-page">
      <div class="page-head">
        <div><h1 id="view-title" tabindex="-1">Settings</h1><p class="subtitle">Local preferences, saved to this workspace only.</p></div>
        <button class="btn primary" onClick={() => api.saveSettings(s()).then(() => toast("Settings saved", "", "success")).catch((e) => toast("Could not save", String(e), "error"))}><Icon name="save" size={14} /> Save now</button>
      </div>

      <div class="settings-layout">
        <nav class="settings-nav" aria-label="Settings sections">
          <For each={groups()}>
            {(group) => (
              <div class="settings-nav-group">
                <span class="settings-nav-label">{group}</span>
                <For each={SECTIONS.filter((x) => x.group === group)}>
                  {(item) => (
                    <button class={section() === item.id ? "active" : ""} onClick={() => setSection(item.id)}>
                      <Icon name={item.icon} size={15} /> {item.label}
                    </button>
                  )}
                </For>
              </div>
            )}
          </For>
        </nav>

        <div class="settings-body">
          <Show when={section() === "appearance"}>
            <Section title="Density" desc="Comfortable for browsing, compact for long sessions, table for max accounts per screen.">
              <Segmented value={s().density} options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }, { value: "ultra_compact", label: "Ultra-compact" }] as any} onChange={(v) => updateSettings((d) => { d.density = v; })} label="Density" />
            </Section>
            <Section title="Theme" desc="Built-in packs or your own palette. Everything is a CSS custom property.">
              <div class="theme-grid">
                <For each={Object.values(palettes)}>
                  {(palette) => (
                    <button class={`theme-swatch ${s().theme.builtin === palette.id && !s().theme.custom ? "active" : ""}`} onClick={() => pickTheme(palette.id)} aria-label={`Use ${palette.name} theme`}>
                      <span class="swatch-preview" style={{ background: palette.bg, "border-color": palette.line }}>
                        <span style={{ background: palette.panel, "border-color": palette.line }} />
                        <span style={{ background: palette.accent }} />
                        <span style={{ background: palette.second }} />
                      </span>
                      <span class="swatch-name">{palette.name}</span>
                    </button>
                  )}
                </For>
              </div>
              <div class="row" style={{ "margin-top": "10px" }}>
                <button class="btn sm" onClick={exportTheme}><Icon name="download" size={13} /> Export theme</button>
                <button class="btn sm ghost" onClick={importTheme}><Icon name="upload" size={13} /> Import theme</button>
                <button class="btn sm ghost" onClick={() => { setThemeJson(JSON.stringify({ name: "My theme", mode: "dark", colors: palettes.obsidian }, null, 2)); setThemeJsonOpen(true); }}><Icon name="code" size={13} /> Live editor</button>
              </div>
            </Section>
            <Section title="Live editor" desc="Tweak the active palette; changes apply instantly and are saved.">
              <div class="color-row">
                <For each={[["accent", "Accent"], ["bg", "Background"], ["panel", "Surface"], ["line", "Borders"], ["text", "Text"], ["muted", "Muted"], ["ok", "Success"], ["warn", "Warning"], ["danger", "Danger"]] as [string, string][]}>
                  {([key, label]) => {
                    const current = () => s().theme.custom?.colors?.[key] ?? (palettes[s().theme.builtin] ?? palettes.obsidian)[key as keyof typeof palettes.obsidian] as string;
                    return (
                      <label class="color-field"><span>{label}</span><input type="color" value={String(current())} onInput={(e) => saveThemeCustom(key, e.currentTarget.value)} /><code>{String(current())}</code></label>
                    );
                  }}
                </For>
              </div>
            </Section>
            <Section title="Window material" desc="Mica and Acrylic are Windows 11 only; fall back to a solid surface elsewhere.">
              <Segmented value={s().theme.material} options={[{ value: "mica", label: "Mica" }, { value: "acrylic", label: "Acrylic" }, { value: "solid", label: "Solid" }] as any} onChange={(v) => updateSettings((d) => { d.theme.material = v; d.theme.reduce_transparency = v === "solid"; })} label="Window material" />
              <ToggleRow title="Sync with Windows accent colour" desc="Follows the system accent, including when it changes live." checked={s().theme.accent_sync} onChange={(v) => updateSettings((d) => { d.theme.accent_sync = v; })} />
              <ToggleRow title="Custom wallpaper" desc="Use an image behind the board, with dimming and optional blur." checked={!!s().theme.background_image} onChange={(v) => updateSettings((d) => { d.theme.custom = { ...(d.theme.custom ?? palettes.obsidian), colors: d.theme.custom?.colors ?? {}, schema_version: 1, id: "custom", name: "Custom", author: "You", mode: "dark", radius_px: 12, background_dim: 0.4, background_blur: 0, background_image: v ? "C:\\wallpaper.jpg" : null, font_family: null }; })} />
            </Section>
            <Section title="Accessibility" desc="Everything works from the keyboard; nothing relies on colour alone.">
              <div class="form-grid two">
                <label class="field"><span>Language</span><select class="input" value={s().language} onChange={(e) => updateSettings((d) => { d.language = e.currentTarget.value; })}><For each={[...languages]}>{(l) => <option value={l.code}>{l.name}</option>}</For></select></label>
                <label class="field"><span>UI scale · {Math.round(s().ui_scale * 100)}%</span><input type="range" min="75" max="200" step="5" value={s().ui_scale * 100} onInput={(e) => updateSettings((d) => { d.ui_scale = Number(e.currentTarget.value) / 100; document.documentElement.style.setProperty("--ui-scale", String(d.ui_scale)); })} /></label>
              </div>
              <ToggleRow title="Reduce motion" desc="Honours the OS preference automatically; this forces it in RoFarm too." checked={s().performance.reduce_motion === "always"} onChange={(v) => updateSettings((d) => { d.performance.reduce_motion = v ? "always" : "system"; })} />
              <ToggleRow title="Colour-blind-safe status palette" desc="Adds shapes and text to status chips, not just green/red." checked={!!s().notifications?.colorblind_safe} onChange={(v) => updateSettings((d) => { d.notifications.colorblind_safe = v; })} />
            </Section>
          </Show>

          <Show when={section() === "performance"}>
            <Section title="Resource profile" desc="Applies to newly launched clients; live clients update on next restart.">
              <Segmented value={s().resource_profile} options={[{ value: "eco", label: "Eco" }, { value: "balanced", label: "Balanced" }, { value: "performance", label: "Performance" }, { value: "custom", label: "Custom" }] as any} onChange={(v) => updateSettings((d) => { d.resource_profile = v; })} />
              <div class="profile-grid">
                <div class="profile-card"><strong>Eco</strong><span>25% CPU cap, low priority, FPS 30, thumbnails off.</span></div>
                <div class="profile-card"><strong>Balanced</strong><span>50% CPU cap, normal priority, FPS 45.</span></div>
                <div class="profile-card"><strong>Performance</strong><span>No cap, above-normal priority, FPS 60.</span></div>
              </div>
            </Section>
            <Section title="UI performance" desc="Budgets: idle ≤ 60 MB RSS and < 0.3% CPU with ten accounts.">
              <div class="form-grid two">
                <label class="field"><span>Refresh interval · {s().performance.refresh_ms} ms</span><input type="range" min="250" max="5000" step="50" value={s().performance.refresh_ms} onInput={(e) => updateSettings((d) => { d.performance.refresh_ms = Number(e.currentTarget.value); })} /></label>
                <label class="field"><span>Metrics interval · {s().performance.metrics_interval_ms} ms</span><input type="range" min="500" max="10000" step="100" value={s().performance.metrics_interval_ms} onInput={(e) => updateSettings((d) => { d.performance.metrics_interval_ms = Number(e.currentTarget.value); })} /></label>
              </div>
              <ToggleRow title="Window thumbnails" desc="Captured only for visible tiles, paused when the Fleet view is hidden." checked={s().performance.thumbnails} onChange={(v) => updateSettings((d) => { d.performance.thumbnails = v; })} />
              <ToggleRow title="Sparklines" desc="Metric history draws 60-point SVGs; disable on very old GPUs." checked={s().performance.sparklines} onChange={(v) => updateSettings((d) => { d.performance.sparklines = v; })} />
              <ToggleRow title="Profiler panel" desc="Adds timing probes to IPC and paint. Off by default." checked={s().performance.profiler_visible} onChange={(v) => updateSettings((d) => { d.performance.profiler_visible = v; })} />
              <ToggleRow title="Pause UI timers when hidden" desc="When minimized, the webview is suspended and the core keeps running." checked={s().performance.pause_when_hidden} onChange={(v) => updateSettings((d) => { d.performance.pause_when_hidden = v; })} />
              <ToggleRow title="Pause animations on battery" desc="Saves a measurable amount of power on 60 Hz panels." checked={s().performance.pause_animations_on_battery} onChange={(v) => updateSettings((d) => { d.performance.pause_animations_on_battery = v; })} />
            </Section>
            <Section title="Eco mode" desc="A single switch for low refresh, no thumbnails, and EcoQoS.">
              <ToggleRow title="Enable Eco mode" desc="RoFarm itself gets Windows EcoQoS when hidden in the tray." checked={s().eco_mode} onChange={(v) => updateSettings((d) => { d.eco_mode = v; d.performance.thumbnails = !v; })} />
              <div class="form-grid two">
                <label class="field"><span>Thermal threshold · {s().eco_cpu_temp_c}°C</span><input type="range" min="60" max="95" value={s().eco_cpu_temp_c} onInput={(e) => updateSettings((d) => { d.eco_cpu_temp_c = Number(e.currentTarget.value); })} /></label>
                <label class="field"><span>Battery threshold · {s().eco_battery_percent}%</span><input type="range" min="5" max="60" value={s().eco_battery_percent} onInput={(e) => updateSettings((d) => { d.eco_battery_percent = Number(e.currentTarget.value); })} /></label>
              </div>
            </Section>
          </Show>

          <Show when={section() === "notifications"}>
            <Section title="Delivery" desc="Native toasts use WinRT action buttons; everything else is opt-in.">
              <ToggleRow title="Native Windows toasts" desc="Appear in Action Center with Restart, Kill, and Snooze buttons." checked={s().notifications?.native !== false} onChange={(v) => updateSettings((d) => { d.notifications.native = v; })} />
              <ToggleRow title="Respect Focus Assist / Do Not Disturb" desc="Notifications stay in the center without popping up." checked={s().notifications?.respect_focus !== false} onChange={(v) => updateSettings((d) => { d.notifications.respect_focus = v; })} />
              <ToggleRow title="Quiet hours" desc="Non-critical alerts are queued until quiet hours end." checked={!!s().notifications?.quiet_hours?.enabled} onChange={(v) => updateSettings((d) => { d.notifications.quiet_hours = { ...(d.notifications.quiet_hours ?? {}), enabled: v, start: 23, end: 8 }; })} />
            </Section>
            <Section title="Events" desc="Choose which events create a notification. Per-account mutes override these.">
              <div class="event-grid">
                <For each={EVENTS}>
                  {([id, label]) => {
                    const enabled = () => s().notifications?.events?.[id] !== false;
                    return <label class="event-toggle"><input type="checkbox" checked={enabled()} onChange={(e) => updateSettings((d) => { d.notifications.events = { ...(d.notifications.events ?? {}), [id]: e.currentTarget.checked }; })} /><span>{label}</span></label>;
                  }}
                </For>
              </div>
            </Section>
            <Section title="Rate limiting" desc="Protects you and the remote endpoint from notification storms.">
              <div class="form-grid two">
                <label class="field"><span>Max per minute</span><input class="input" type="number" min="1" max="60" value={s().notifications?.rate_limit_per_minute ?? 12} onInput={(e) => updateSettings((d) => { d.notifications.rate_limit_per_minute = Number(e.currentTarget.value); })} /></label>
                <label class="field"><span>Duplicate window (seconds)</span><input class="input" type="number" min="0" max="3600" value={s().notifications?.dedupe_seconds ?? 120} onInput={(e) => updateSettings((d) => { d.notifications.dedupe_seconds = Number(e.currentTarget.value); })} /></label>
              </div>
            </Section>
          </Show>

          <Show when={section() === "integrations"}>
            <p class="settings-note"><Icon name="shield" size={14} /> Secrets are encrypted in the vault. They are never sent anywhere except the endpoint you configure.</p>
            <NotifierCard title="Discord" icon="discord" id="discord" fields={[["webhook_url", "Webhook URL", "https://discord.com/api/webhooks/…"], ["username", "Bot name", "RoFarm"]]} values={notifier("discord")} onChange={(p) => setNotifier("discord", p)} />
            <NotifierCard title="Telegram" icon="telegram" id="telegram" fields={[["bot_token", "Bot token", "123456:ABC…"], ["chat_id", "Chat ID", "-100…"]]} values={notifier("telegram")} onChange={(p) => setNotifier("telegram", p)} />
            <NotifierCard title="Slack" icon="slack" id="slack" fields={[["webhook_url", "Incoming webhook URL", "https://hooks.slack.com/…"]]} values={notifier("slack")} onChange={(p) => setNotifier("slack", p)} />
            <NotifierCard title="ntfy" icon="bell" id="ntfy" fields={[["topic", "Topic", "rofarm-alerts"], ["server", "Server", "https://ntfy.sh"]]} values={notifier("ntfy")} onChange={(p) => setNotifier("ntfy", p)} />
            <NotifierCard title="Pushover" icon="bell" id="pushover" fields={[["user_key", "User key", "u…"], ["app_token", "App token", "a…"]]} values={notifier("pushover")} onChange={(p) => setNotifier("pushover", p)} />
            <NotifierCard title="Email (SMTP)" icon="mail" id="email" fields={[["host", "SMTP host", "smtp.example.com"], ["port", "Port", "587"], ["username", "Username", "you@example.com"], ["password", "Password", "••••••••"], ["to", "Send to", "you@example.com"]]} values={notifier("email")} onChange={(p) => setNotifier("email", p)} />
            <Section title="Remote dashboard" desc="Opt-in browser access from another device on your network. TLS with a pinned certificate, PIN + optional TOTP.">
              <ToggleRow title="Enable remote dashboard" desc="Creates a Windows Firewall rule only after you confirm the UAC prompt." checked={!!s().remote?.enabled} onChange={(v) => { if (v) setRemoteOpen(true); else updateSettings((d) => { d.remote = { ...(d.remote ?? {}), enabled: false }; }); }} />
              <Show when={s().remote?.enabled}>
                <div class="row"><span class="chip ok"><Icon name="wifi" size={11} /> {s().remote?.address ?? "https://0.0.0.0:47810"}</span><span class="muted tiny">{s().remote?.devices?.length ?? 0} paired device(s)</span></div>
              </Show>
            </Section>
            <Section title="Webhook out" desc="Post every selected event as JSON to your own endpoint.">
              <label class="field"><span>Webhook URL (optional)</span><input class="input" value={s().integrations?.webhook_url ?? ""} onInput={(e) => updateSettings((d) => { d.integrations = { ...(d.integrations ?? {}), webhook_url: e.currentTarget.value }; })} placeholder="https://example.com/rofarm" /></label>
            </Section>
          </Show>

          <Show when={section() === "windows"}>
            <Section title="Startup" desc="Per-user, no admin, no scheduled task unless you need wake timers.">
              <ToggleRow title="Start RoFarm when I sign in" desc="Uses the Startup folder or Task Scheduler for wake timers." checked={s().start_with_windows} onChange={(v) => updateSettings((d) => { d.start_with_windows = v; })} />
              <ToggleRow title="Start minimized to tray" checked={!!s().windows?.start_minimized} onChange={(v) => updateSettings((d) => { d.windows.start_minimized = v; })} />
              <ToggleRow title="Close button hides to tray" desc="Kill clients separately; closing the window never stops your fleet." checked={s().windows?.close_to_tray !== false} onChange={(v) => updateSettings((d) => { d.windows.close_to_tray = v; })} />
              <ToggleRow title="Confirm before quitting" checked={s().windows?.confirm_quit !== false} onChange={(v) => updateSettings((d) => { d.windows.confirm_quit = v; })} />
            </Section>
            <Section title="Shell integration" desc="Jump lists, taskbar progress, overlays, and the rofarm:// URL scheme.">
              <ToggleRow title="Taskbar jump lists" desc="Quick actions: launch scene, open Mini mode, stop all." checked={s().windows?.jump_list !== false} onChange={(v) => updateSettings((d) => { d.windows.jump_list = v; })} />
              <ToggleRow title="Taskbar progress and overlay badges" desc="Shows launching progress and a red badge when a client fails." checked={s().windows?.taskbar_progress !== false} onChange={(v) => updateSettings((d) => { d.windows.taskbar_progress = v; })} />
              <ToggleRow title="Shell context menu: “Launch with RoFarm”" desc="Adds a per-user Explorer entry for .rofarm, .rbxl, and Roblox links." checked={!!s().windows?.context_menu} onChange={(v) => updateSettings((d) => { d.windows.context_menu = v; })} />
              <ToggleRow title="Always-on-top Mini mode" desc="A small overlay with fleet status and Kill." checked={s().windows?.mini_always_on_top !== false} onChange={(v) => updateSettings((d) => { d.windows.mini_always_on_top = v; })} />
            </Section>
            <Section title="Sign-in" desc="Windows Hello protects the vault without a typed passphrase.">
              <ToggleRow title="Enable Windows Hello" desc="Requires a TPM 2.0 and a configured PIN or biometric." checked={!!s().security?.hello_enabled} onChange={(v) => updateSettings((d) => { d.security.hello_enabled = v; })} />
            </Section>
          </Show>

          <Show when={section() === "data"}>
            <Section title="Workspaces" desc="Separate vaults and settings. Quick-switch without restarting the core.">
              <div class="workspace-list">
                <For each={workspaces()} fallback={<p class="muted small">Only the default workspace exists.</p>}>
                  {(workspace) => (
                    <div class="workspace-row">
                      <Icon name="database" size={15} />
                      <div class="grow"><strong>{workspace.name}</strong><div class="tiny muted">{workspace.path}</div></div>
                      <Show when={workspace.active} fallback={<button class="btn sm" onClick={() => invoke("switch_workspace", { id: workspace.id }).then(() => refresh()).then(() => toast("Workspace switched", workspace.name, "success")).catch((e) => toast("Could not switch", String(e), "error"))}>Switch</button>}>
                        <span class="chip ok tiny">active</span>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </Section>
            <Section title="Portable mode" desc="Keep the vault, settings, and logs beside rofarm.exe.">
              <div class="kv-grid">
                <div><span class="muted tiny">Data directory</span><code>{bootstrap()?.data_dir ?? "—"}</code></div>
                <div><span class="muted tiny">Portable</span><strong>{bootstrap()?.portable ? "Yes" : "No"}</strong></div>
              </div>
            </Section>
            <Section title="Import and export" desc="Move settings and themes between PCs. Account data moves through the encrypted vault.">
              <div class="row" style={{ "flex-wrap": "wrap" }}>
                <button class="btn sm" onClick={async () => { const path = await saveFile({ defaultPath: "rofarm-settings.json", filters: [{ name: "RoFarm settings", extensions: ["json"] }], contents: JSON.stringify(s(), null, 2) }); if (path) toast("Settings exported", path, "success"); }}><Icon name="download" size={13} /> Export settings</button>
                <button class="btn sm ghost" onClick={async () => { const file = await pickFile({ filters: [{ name: "RoFarm settings", extensions: ["json"] }] }); if (typeof file !== "string") return; try { const parsed = JSON.parse(file); await invoke("import_settings", { json: JSON.stringify(parsed) }); await refresh(); toast("Settings imported", "Restart RoFarm if a window material looks wrong.", "success"); } catch (e: any) { toast("Import failed", String(e?.message ?? e), "error"); } }}><Icon name="upload" size={13} /> Import settings</button>
                <button class="btn sm ghost" onClick={() => downloadText("rofarm-theme.json", JSON.stringify({ theme: s().theme }, null, 2))}><Icon name="palette" size={13} /> Export theme</button>
              </div>
            </Section>
            <Section title="Backups" desc="Encrypted before leaving this machine.">
              <ToggleRow title="Daily encrypted backup" desc="04:00 local time, keeps 7 verified archives." checked={!!s().backups?.enabled} onChange={(v) => updateSettings((d) => { d.backups.enabled = v; })} />
              <ToggleRow title="Copy to a synced folder" desc="OneDrive, a network share, or an external drive." checked={!!s().backups?.remote_copy} onChange={(v) => updateSettings((d) => { d.backups.remote_copy = v; })} />
              <p class="hint"><Icon name="info" size={12} /> Last backup: {s().backups?.last_at ? formatDate(s().backups.last_at, true) : "never"}. Manage backups in the Vault view.</p>
            </Section>
          </Show>

          <Show when={section() === "privacy"}>
            <Section title="Network policy" desc="A strict allow-list, enforced before DNS resolution.">
              <ToggleRow title="Show the network log panel" desc="Every request, its host, duration, and allow/block decision." checked={s().show_network_log} onChange={(v) => updateSettings((d) => { d.show_network_log = v; })} />
              <div class="allow-list">
                <For each={[".roblox.com", ".rbxcdn.com", "update feed (signed)", "your notifier endpoints"]}>{(host) => <span class="chip ghost"><Icon name="check" size={11} /> *{host}</span>}</For>
              </div>
            </Section>
            <Section title="Telemetry" desc="There is none. RoFarm has no analytics SDK, no account, and no cloud.">
              <ToggleRow title="Opt-in redacted crash reports" desc="Off by default. Writes a local file you choose to send; never includes cookies or account names." checked={!!s().privacy?.crash_reports} onChange={(v) => updateSettings((d) => { d.privacy.crash_reports = v; })} />
              <ToggleRow title="Hide account names in screenshots" desc="Blurs usernames in captured client thumbnails." checked={!!s().privacy?.blur_screenshots} onChange={(v) => updateSettings((d) => { d.privacy.blur_screenshots = v; })} />
              <ToggleRow title="Clear clipboard after copying a secret" desc="30 seconds, with the clipboard-history exclusion flag." checked={s().security?.clipboard_clear !== false} onChange={(v) => updateSettings((d) => { d.security.clipboard_clear = v; })} />
            </Section>
            <Section title="Vault" desc="Crypto and recovery live in the Vault view.">
              <div class="row"><button class="btn" onClick={() => { window.location.hash = "#vault"; }}>Open Vault</button><span class="muted small">XChaCha20-Poly1305 · Argon2id · DPAPI/TPM · Hello · YubiKey</span></div>
            </Section>
          </Show>

          <Show when={section() === "updates"}>
            <Section title="Channel" desc="Stable is recommended. Beta gets features early and breaks more often.">
              <Segmented value={s().updates?.channel ?? "stable"} options={[{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }] as any} onChange={(v) => updateSettings((d) => { d.updates = { ...(d.updates ?? {}), channel: v }; })} />
              <ToggleRow title="Install updates automatically" desc="Only signed releases; downgrade attempts are rejected." checked={s().updates?.auto_install !== false} onChange={(v) => updateSettings((d) => { d.updates.auto_install = v; })} />
              <ToggleRow title="Check on launch" checked={s().updates?.check_on_launch !== false} onChange={(v) => updateSettings((d) => { d.updates.check_on_launch = v; })} />
            </Section>
            <Section title="Status" desc="Detached ed25519 signatures over the installer and manifest.">
              <div class="update-box">
                <div class="row"><Icon name="shield" size={16} /><div class="grow"><strong>RoFarm {update()?.current_version ?? bootstrap()?.version ?? "0.1.0"}</strong><div class="tiny muted">{(update()?.channel ?? s().updates?.channel ?? "stable")} channel · signed feed · delta updates</div></div><button class="btn" onClick={checkUpdates} disabled={checking()}><Icon name={checking() ? "rotate" : "refresh"} size={14} /> {checking() ? "Checking…" : "Check now"}</button></div>
                <Show when={update()?.available}>
                  {(available) => <div class="update-available"><strong>{available().version}</strong><span class="muted small">{available().notes}</span><span class="tiny muted">{(available().size_bytes / 1024 / 1024).toFixed(1)} MB · published {formatDate(available().published_at)}</span><button class="btn primary sm" onClick={() => invoke("install_update", {}).catch((e) => toast("Update could not start", String(e), "error"))}>Install and restart</button></div>}
                </Show>
                <div class="row tiny muted"><Icon name={update()?.verified ? "check" : "info"} size={11} /> {update()?.verified ? "Signature verified" : "Not checked yet"} <span>·</span> Rollback keeps the previous installer</div>
              </div>
            </Section>
          </Show>

          <Show when={section() === "shortcuts"}>
            <Section title="Keyboard shortcuts" desc="Every binding is rebindable. Press ? anywhere for the cheat sheet.">
              <ShortcutEditor />
            </Section>
          </Show>

          <Show when={section() === "plugins"}>
            <Section title="Plugins" desc="Sandboxed WASM only. No native code, no network without a declared host.">
              <Show when={plugins().length} fallback={<p class="muted small">No plugins installed. Drop a signed .rofarm-plugin file here or import from the plugin index.</p>}>
                <div class="plugin-list">
                  <For each={plugins()}>
                    {(plugin) => (
                      <div class="plugin-row">
                        <div class="plugin-icon"><Icon name="plugin" size={16} /></div>
                        <div class="grow"><strong>{plugin.manifest?.name}</strong><div class="tiny muted">{plugin.manifest?.description} · v{plugin.manifest?.version} {plugin.signed ? "· signed" : "· unsigned"}</div></div>
                        <Show when={plugin.manifest?.permissions?.network_hosts?.length}><span class="chip ghost tiny">{plugin.manifest.permissions.network_hosts.join(", ")}</span></Show>
                        <Switch checked={!!plugin.enabled} label={`Enable ${plugin.manifest?.name}`} onChange={(v) => invoke("plugin_toggle", { id: plugin.manifest.id, enabled: v }).then(() => setPlugins(plugins().map((p) => p.manifest.id === plugin.manifest.id ? { ...p, enabled: v } : p)))} />
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Section>
          </Show>

          <Show when={section() === "about"}>
            <Section title="RoFarm" desc="Command Deck for accounts you own.">
              <div class="about-hero">
                <div class="about-logo">RF</div>
                <div><h2>RoFarm {bootstrap()?.version ?? "0.1.0"}</h2><p>Local-first, Windows-only. No account, no telemetry, no cloud.</p></div>
              </div>
              <div class="kv-grid">
                <div><span class="muted tiny">Platform</span><strong>{bootstrap()?.platform ?? "windows"}</strong></div>
                <div><span class="muted tiny">Uptime</span><strong>{formatDuration(performance.now() / 1000)}</strong></div>
                <div><span class="muted tiny">Build</span><code>release · LTO · stripped</code></div>
                <div><span class="muted tiny">License</span><strong>AGPL-3.0-or-later</strong></div>
              </div>
            </Section>
            <Section title="Disclaimer" desc="Please read.">
              <p class="disclaimer">RoFarm is an independent project and is not affiliated with, endorsed by, or sponsored by Roblox Corporation. Use it only with accounts you own and in line with Roblox's Terms of Use. RoFarm manages accounts, clients, and sessions; it never injects into the Roblox client, never reads its memory, and never automates gameplay. Features that depend on Roblox web endpoints or the desktop client may change or break whenever Roblox updates.</p>
            </Section>
            <Section title="Documentation">
              <div class="doc-links">
                <For each={[["SECURITY.md", "Threat model and disclosure policy"], ["PERFORMANCE.md", "Budgets and benchmark methodology"], ["AUTOMATION.md", "Triggers, conditions, actions, YAML"], ["PLUGINS.md", "WASM plugin SDK"], ["ARCHITECTURE.md", "Event-sourced core and IPC"]]}>
                  {([file, desc]) => <button class="doc-link" onClick={() => invoke("open_docs", { page: file }).catch(() => toast(file, desc, "info"))}><Icon name="file" size={14} /><div><strong>{file}</strong><span class="tiny muted">{desc}</span></div><Icon name="external" size={13} /></button>}
                </For>
              </div>
            </Section>
          </Show>
        </div>
      </div>

      <Show when={themeJsonOpen()}>
        <Modal title="Theme JSON" icon="code" wide onClose={() => setThemeJsonOpen(false)} footer={<><button class="btn ghost" onClick={() => setThemeJsonOpen(false)}>Cancel</button><button class="btn primary" onClick={() => { try { const parsed = JSON.parse(themeJson()); updateSettings((d) => { d.theme.custom = { schema_version: 1, id: parsed.id ?? "custom", name: parsed.name ?? "Custom", author: "You", mode: parsed.mode === "light" ? "light" : "dark", colors: parsed.colors ?? {}, radius_px: parsed.radius ?? 12, background_dim: 0.4, background_blur: 0, background_image: null, font_family: null }; }); setThemeJsonOpen(false); toast("Theme applied", "", "success"); } catch (e: any) { toast("Invalid JSON", String(e.message), "error"); } }}>Apply</button></>}>
          <textarea class="input mono code-area" rows="18" value={themeJson()} onInput={(e) => setThemeJson(e.currentTarget.value)} />
        </Modal>
      </Show>

      <Show when={remoteOpen()}>
        <Modal title="Start remote dashboard" icon="wifi" narrow onClose={() => setRemoteOpen(false)} footer={<><button class="btn ghost" onClick={() => setRemoteOpen(false)}>Cancel</button><button class="btn primary" onClick={startRemote}>Start server</button></>}>
          <p class="muted small">Binds to 0.0.0.0:47810 over TLS with a self-signed certificate. Pair each device with this PIN; sessions expire in 30 days and can be revoked.</p>
          <label class="field"><span>Dashboard PIN (6 digits)</span><input class="input mono" type="password" maxlength="6" value={remotePin()} onInput={(e) => setRemotePin(e.currentTarget.value.replace(/\D/g, ""))} placeholder="••••••" /></label>
          <p class="hint"><Icon name="warning" size={12} /> Only enable this on trusted networks. RoFarm asks before creating the Windows Firewall rule.</p>
        </Modal>
      </Show>

      <span class="hidden">{formatDuration(0)}</span>
    </div>
  );
}

function NotifierCard(props: { title: string; icon: IconName; id: string; fields: [string, string, string][]; values: Record<string, any>; onChange: (patch: Record<string, unknown>) => void }) {
  const enabled = () => props.values?.enabled !== false && !!Object.values(props.values ?? {}).some((v) => typeof v === "string" && v.length > 0 && v !== "false");
  return (
    <Section
      title={props.title}
      desc={"Stored encrypted in the vault; sent only to this endpoint."}
      action={<div class="row"><Show when={enabled()}><button class="btn sm ghost" onClick={() => invoke("test_notifier", { id: props.id }).then(() => toast("Test sent", props.title, "success")).catch((e) => toast("Test failed", String(e), "error"))}><Icon name="play" size={12} /> Test</button></Show><Switch checked={props.values?.enabled !== false} label={`Enable ${props.title}`} onChange={(v) => props.onChange({ enabled: v })} /></div>}
    >
      <div class="notifier-grid">
        <For each={props.fields}>
          {([key, label, placeholder]) => (
            <label class="field"><span>{label}</span><input class="input" type={/token|password|key/i.test(key) ? "password" : "text"} value={props.values?.[key] ?? ""} onInput={(e) => props.onChange({ [key]: e.currentTarget.value })} placeholder={placeholder} /></label>
          )}
        </For>
      </div>
    </Section>
  );
}

function ShortcutEditor() {
  const defaults: Record<string, string> = {
    "Command palette": "Ctrl+K",
    "Kill all (hold)": "Ctrl+Shift+K",
    "New account": "Ctrl+N",
    "Launch selected": "Ctrl+Enter",
    "Toggle Mini mode": "Ctrl+Shift+M",
    "Cheat sheet": "?",
    "Board": "Ctrl+1",
    "Fleet": "Ctrl+2",
    "Automations": "Ctrl+5",
    "Lock vault": "Ctrl+L",
    "Undo": "Ctrl+Z",
    "Redo": "Ctrl+Shift+Z",
    "Eco mode": "Ctrl+Shift+E",
  };
  const [bindings, setBindings] = createSignal<Record<string, string>>({ ...defaults, ...(settings().shortcuts ?? {}) });
  const persist = (next: Record<string, string>) => {
    setBindings(next);
    updateSettings((draft) => { draft.shortcuts = next; });
  };
  return (
    <div class="shortcut-list">
      <For each={Object.entries(bindings())}>
        {([label, key]) => (
          <div class="shortcut-row">
            <span>{label}</span>
            <input class="input sm mono" value={key} onInput={(e) => persist({ ...bindings(), [label]: e.currentTarget.value })} aria-label={`Shortcut for ${label}`} />
            <button class="icon-btn" aria-label={`Reset ${label}`} onClick={() => persist({ ...bindings(), [label]: defaults[label] ?? "" })}><Icon name="rotate" size={13} /></button>
          </div>
        )}
      </For>
    </div>
  );
}
