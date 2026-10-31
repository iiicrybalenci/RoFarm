use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::error::{CoreError, Result};
use crate::lane::{AlertKind, QuietHours, ResourceProfile};
use crate::window::WindowMaterial;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
    UltraCompact,
}

impl Density {
    pub fn row_height(&self) -> u16 {
        match self {
            Density::Comfortable => 64,
            Density::Compact => 44,
            Density::UltraCompact => 28,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotionPreference {
    #[default]
    System,
    Full,
    Reduced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    En,
    Es,
    PtBr,
    De,
    Fr,
    Ru,
    Tr,
    Zh,
    Ja,
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Es => "es",
            Language::PtBr => "pt-BR",
            Language::De => "de",
            Language::Fr => "fr",
            Language::Ru => "ru",
            Language::Tr => "tr",
            Language::Zh => "zh",
            Language::Ja => "ja",
        }
    }

    pub fn native_name(&self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Es => "Español",
            Language::PtBr => "Português (Brasil)",
            Language::De => "Deutsch",
            Language::Fr => "Français",
            Language::Ru => "Русский",
            Language::Tr => "Türkçe",
            Language::Zh => "中文",
            Language::Ja => "日本語",
        }
    }

    pub fn all() -> &'static [Language] {
        &[
            Language::En,
            Language::Es,
            Language::PtBr,
            Language::De,
            Language::Fr,
            Language::Ru,
            Language::Tr,
            Language::Zh,
            Language::Ja,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeDefinition {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    pub mode: String,
    pub colors: BTreeMap<String, String>,
    pub radius_px: u8,
    pub font_family: Option<String>,
    pub background_image: Option<String>,
    pub background_dim: u8,
    pub background_blur: u8,
}

impl Default for ThemeDefinition {
    fn default() -> Self {
        Self {
            schema_version: 1,
            id: "custom".into(),
            name: "Custom".into(),
            author: "You".into(),
            mode: "dark".into(),
            colors: BTreeMap::new(),
            radius_px: 10,
            font_family: None,
            background_image: None,
            background_dim: 40,
            background_blur: 0,
        }
    }
}

const REQUIRED_COLORS: &[&str] = &[
    "bg", "panel", "panel-raised", "text", "text-muted", "accent", "accent-contrast", "border", "danger",
    "warning", "success", "in-game", "idle", "failed",
];

impl ThemeDefinition {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(CoreError::Invalid("unsupported theme schema version".into()));
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err(CoreError::Invalid("theme needs an id and name".into()));
        }
        if self.id.len() > 64 || self.name.len() > 80 {
            return Err(CoreError::Invalid("theme id or name is too long".into()));
        }
        if !matches!(self.mode.as_str(), "dark" | "light") {
            return Err(CoreError::Invalid("theme mode must be dark or light".into()));
        }
        for key in REQUIRED_COLORS {
            let value = self
                .colors
                .get(*key)
                .ok_or_else(|| CoreError::Invalid(format!("theme is missing colour '{key}'")))?;
            validate_color(value)?;
        }
        for (k, v) in &self.colors {
            if k.len() > 48 {
                return Err(CoreError::Invalid("theme colour key too long".into()));
            }
            validate_color(v)?;
        }
        if self.radius_px > 32 {
            return Err(CoreError::Invalid("theme radius must be <= 32".into()));
        }
        if self.background_dim > 90 || self.background_blur > 40 {
            return Err(CoreError::Invalid("background dim/blur out of range".into()));
        }
        if let Some(img) = &self.background_image {
            let lower = img.to_ascii_lowercase();
            if !(lower.starts_with("https://") || lower.starts_with("data:image/")) {
                return Err(CoreError::Invalid(
                    "background must be an https URL or embedded data:image".into(),
                ));
            }
            if img.len() > 4_000_000 {
                return Err(CoreError::Invalid("background image reference is too large".into()));
            }
        }
        Ok(())
    }
}

fn validate_color(value: &str) -> Result<()> {
    let v = value.trim();
    let hex = v.strip_prefix('#').unwrap_or(v);
    if (hex.len() == 3 || hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else if v.starts_with("rgb(") || v.starts_with("rgba(") || v.starts_with("hsl(") {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("invalid colour '{value}'")))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub builtin: String,
    pub custom: Option<ThemeDefinition>,
    pub accent_sync: bool,
    pub accent_override: Option<String>,
    pub material: WindowMaterial,
    pub reduce_transparency: bool,
    pub background_dim: u8,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            builtin: "obsidian".into(),
            custom: None,
            accent_sync: true,
            accent_override: None,
            material: WindowMaterial::Mica,
            reduce_transparency: false,
            background_dim: 40,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotifierKind {
    #[default]
    Discord,
    Telegram,
    Slack,
    Ntfy,
    Pushover,
    Email,
    Webhook,
    NativeToast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifierConfig {
    pub kind: NotifierKind,
    pub enabled: bool,
    pub vault_key: String,
    pub events: BTreeSet<AlertKind>,
    pub min_severity: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub native_toasts: bool,
    pub toast_actions: bool,
    pub sound: bool,
    pub focus_assist_respect: bool,
    pub quiet_hours: Option<QuietHours>,
    pub history_limit: u32,
    pub notifiers: Vec<NotifierConfig>,
    pub discord_rich_embeds: bool,
    pub discord_thumbnails: bool,
    pub rate_limit_per_hour: u32,
    pub dedupe_minutes: u32,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            native_toasts: true,
            toast_actions: true,
            sound: false,
            focus_assist_respect: true,
            quiet_hours: None,
            history_limit: 500,
            notifiers: Vec::new(),
            discord_rich_embeds: true,
            discord_thumbnails: true,
            rate_limit_per_hour: 30,
            dedupe_minutes: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteSettings {
    pub enabled: bool,
    pub port: u16,
    pub tls: bool,
    pub pinned_fingerprint: Option<String>,
    pub totp_required: bool,
    pub ip_allowlist: Vec<String>,
    pub firewall_rule_added: bool,
    pub allow_lan_bots: bool,
    pub discord_commands: bool,
    pub telegram_commands: bool,
    pub command_allowlist: Vec<String>,
}

impl Default for RemoteSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 47810,
            tls: true,
            pinned_fingerprint: None,
            totp_required: true,
            ip_allowlist: vec!["127.0.0.1".into(), "::1".into()],
            firewall_rule_added: false,
            allow_lan_bots: false,
            discord_commands: false,
            telegram_commands: false,
            command_allowlist: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub channel: UpdateChannel,
    pub auto_check: bool,
    pub auto_install: bool,
    pub install_only_idle: bool,
    pub last_checked: Option<DateTime<Utc>>,
    pub last_version: Option<String>,
    pub allow_downgrade: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Stable,
            auto_check: true,
            auto_install: false,
            install_only_idle: true,
            last_checked: None,
            last_version: None,
            allow_downgrade: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupSettings {
    pub enabled: bool,
    pub cron: String,
    pub destination: Option<String>,
    pub retention_count: u32,
    pub retention_days: u32,
    pub separate_passphrase: bool,
    pub verify_after_write: bool,
    pub include_audit_log: bool,
    pub last_run: Option<DateTime<Utc>>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            cron: "0 3 * * *".into(),
            destination: None,
            retention_count: 14,
            retention_days: 30,
            separate_passphrase: true,
            verify_after_write: true,
            include_audit_log: true,
            last_run: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceSettings {
    pub refresh_ms: u32,
    pub thumbnails: bool,
    pub thumbnail_fps: u8,
    pub sparklines: bool,
    pub metrics_interval_ms: u32,
    pub profiler_visible: bool,
    pub pause_when_hidden: bool,
    pub reduce_motion: MotionPreference,
    pub pause_animations_on_battery: bool,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            refresh_ms: 250,
            thumbnails: true,
            thumbnail_fps: 4,
            sparklines: true,
            metrics_interval_ms: 1000,
            profiler_visible: false,
            pause_when_hidden: true,
            reduce_motion: MotionPreference::System,
            pause_animations_on_battery: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    pub auto_lock_minutes: u32,
    pub lock_on_sleep: bool,
    pub lock_on_session_switch: bool,
    pub lock_on_screenshot: bool,
    pub clipboard_clear_seconds: u32,
    pub require_reauth_for_reveal: bool,
    pub require_reauth_for_export: bool,
    pub wer_exclusion: bool,
    pub duress_configured: bool,
    pub hello_enabled: bool,
    pub tpm_sealed: bool,
    pub fido2_enabled: bool,
    pub failed_attempt_limit: u8,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 10,
            lock_on_sleep: true,
            lock_on_session_switch: true,
            lock_on_screenshot: false,
            clipboard_clear_seconds: 20,
            require_reauth_for_reveal: true,
            require_reauth_for_export: true,
            wer_exclusion: false,
            duress_configured: false,
            hello_enabled: false,
            tpm_sealed: false,
            fido2_enabled: false,
            failed_attempt_limit: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    pub crash_reports: bool,
    pub diagnostics_redact_usernames: bool,
    pub network_log_limit: u32,
    pub keep_logs_days: u32,
    pub allow_roblox_profile_images: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            crash_reports: false,
            diagnostics_redact_usernames: true,
            network_log_limit: 1000,
            keep_logs_days: 14,
            allow_roblox_profile_images: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowUiSettings {
    pub remember_positions: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub always_on_top_default: bool,
    pub borderless_default: bool,
    pub mini_edge: String,
    pub mini_bounds: Option<crate::window::Rect>,
    pub widget_enabled: bool,
    pub taskbar_widget: bool,
    pub material: WindowMaterial,
    pub show_profiler: bool,
}

impl Default for WindowUiSettings {
    fn default() -> Self {
        Self {
            remember_positions: true,
            start_minimized: false,
            close_to_tray: true,
            always_on_top_default: false,
            borderless_default: false,
            mini_edge: "bottom-right".into(),
            mini_bounds: None,
            widget_enabled: false,
            taskbar_widget: false,
            material: WindowMaterial::Mica,
            show_profiler: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntegrationSettings {
    pub webhook_out_enabled: bool,
    pub rest_api_enabled: bool,
    pub obs_overlay_enabled: bool,
    pub streamdeck_enabled: bool,
    pub plugin_host_enabled: bool,
    pub plugin_network_default: bool,
    pub plugins: Vec<PluginInstall>,
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            webhook_out_enabled: false,
            rest_api_enabled: false,
            obs_overlay_enabled: false,
            streamdeck_enabled: false,
            plugin_host_enabled: false,
            plugin_network_default: false,
            plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInstall {
    pub id: crate::ids::PluginId,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub path: String,
    pub permissions: Vec<String>,
    pub installed_at: DateTime<Utc>,
    pub signature_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnboardingState {
    pub complete: bool,
    pub step: u8,
    pub vault_mode: String,
    pub resources_chosen: bool,
    pub imported_accounts: u32,
    pub tour_complete: bool,
    pub started_at: Option<DateTime<Utc>>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self { complete: false, step: 0, vault_mode: "passphrase".into(), resources_chosen: false, imported_accounts: 0, tour_complete: false, started_at: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub density: Density,
    pub language: Language,
    pub ui_scale: u16,
    pub theme: ThemeSettings,
    pub shortcuts: BTreeMap<String, String>,
    pub resource_profile: ResourceProfile,
    pub eco_mode: bool,
    pub eco_cpu_temp_c: u8,
    pub eco_battery_percent: u8,
    pub performance: PerformanceSettings,
    pub security: SecuritySettings,
    pub notifications: NotificationSettings,
    pub privacy: PrivacySettings,
    pub windows: WindowUiSettings,
    pub backups: BackupSettings,
    pub updates: UpdateSettings,
    pub remote: RemoteSettings,
    pub integrations: IntegrationSettings,
    pub onboarding: OnboardingState,
    pub start_with_windows: bool,
    pub auto_shutdown_minutes: Option<u32>,
    pub theme_packs: Vec<ThemeDefinition>,
    pub custom_tags: BTreeSet<String>,
    pub experimental: BTreeMap<String, bool>,
    pub show_network_log: bool,
    pub locale_override: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 2,
            density: Density::Comfortable,
            language: Language::En,
            ui_scale: 100,
            theme: ThemeSettings::default(),
            shortcuts: default_shortcuts(),
            resource_profile: ResourceProfile::Balanced,
            eco_mode: false,
            eco_cpu_temp_c: 85,
            eco_battery_percent: 25,
            performance: PerformanceSettings::default(),
            security: SecuritySettings::default(),
            notifications: NotificationSettings::default(),
            privacy: PrivacySettings::default(),
            windows: WindowUiSettings::default(),
            backups: BackupSettings::default(),
            updates: UpdateSettings::default(),
            remote: RemoteSettings::default(),
            integrations: IntegrationSettings::default(),
            onboarding: OnboardingState::default(),
            start_with_windows: false,
            auto_shutdown_minutes: None,
            theme_packs: Vec::new(),
            custom_tags: BTreeSet::new(),
            experimental: BTreeMap::new(),
            show_network_log: false,
            locale_override: None,
        }
    }
}

pub fn default_shortcuts() -> BTreeMap<String, String> {
    [
        ("command_palette", "Ctrl+K"),
        ("global_search", "Ctrl+Shift+F"),
        ("kill_all", "Ctrl+Shift+Alt+K"),
        ("panic", "None"),
        ("mini_mode", "Ctrl+Shift+M"),
        ("toggle_view_board", "Ctrl+1"),
        ("toggle_view_fleet", "Ctrl+2"),
        ("toggle_view_timeline", "Ctrl+3"),
        ("toggle_view_automations", "Ctrl+4"),
        ("toggle_view_vault", "Ctrl+5"),
        ("cheat_sheet", "?"),
        ("save_scene", "Ctrl+Alt+S"),
        ("screenshot_client", "Ctrl+Shift+P"),
        ("lock_vault", "Ctrl+L"),
        ("focus_next_client", "Ctrl+Tab"),
        ("undo", "Ctrl+Z"),
        ("redo", "Ctrl+Shift+Z"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 2 {
            return Err(CoreError::Invalid("unsupported settings schema".into()));
        }
        if !(75..=200).contains(&self.ui_scale) {
            return Err(CoreError::Invalid("UI scale must be between 75% and 200%".into()));
        }
        if !(16..=5000).contains(&self.performance.refresh_ms) {
            return Err(CoreError::Invalid("refresh rate out of range".into()));
        }
        if !(1..=15).contains(&self.performance.thumbnail_fps) {
            return Err(CoreError::Invalid("thumbnail FPS must be 1-15".into()));
        }
        if !(1..=120).contains(&self.security.auto_lock_minutes) {
            return Err(CoreError::Invalid("auto-lock must be 1-120 minutes".into()));
        }
        if !(5..=300).contains(&self.security.clipboard_clear_seconds) {
            return Err(CoreError::Invalid("clipboard clear must be 5-300 seconds".into()));
        }
        if self.remote.enabled && self.remote.port < 1024 {
            return Err(CoreError::Invalid("remote dashboard needs an unprivileged port".into()));
        }
        if self.backups.enabled && self.backups.destination.is_none() {
            return Err(CoreError::Invalid("backups are enabled but no destination is set".into()));
        }
        if self.backups.retention_count == 0 {
            return Err(CoreError::Invalid("keep at least one backup".into()));
        }
        if let Some(custom) = &self.theme.custom {
            custom.validate()?;
        }
        for pack in &self.theme_packs {
            pack.validate()?;
        }
        if self.security.auto_lock_minutes == 0 {
            return Err(CoreError::Invalid("auto-lock cannot be disabled".into()));
        }
        Ok(())
    }

    pub fn shortcut(&self, action: &str) -> Option<&str> {
        self.shortcuts.get(action).map(|s| s.as_str())
    }

    pub fn blocked_hosts_are_allowed(&self) -> bool {
        self.privacy.network_log_limit > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate_and_are_safe() {
        let s = Settings::default();
        s.validate().unwrap();
        assert!(!s.remote.enabled);
        assert!(s.security.clipboard_clear_seconds >= 5);
        assert_eq!(s.ui_scale, 100);
        assert_eq!(s.shortcuts["command_palette"], "Ctrl+K");
        assert_eq!(Language::all().len(), 9);
    }

    #[test]
    fn rejects_out_of_range_settings() {
        let mut s = Settings::default();
        s.ui_scale = 50;
        assert!(s.validate().is_err());
        s.ui_scale = 150;
        s.performance.thumbnail_fps = 0;
        assert!(s.validate().is_err());
        s.performance.thumbnail_fps = 4;
        s.security.auto_lock_minutes = 0;
        assert!(s.validate().is_err());
        s.security.auto_lock_minutes = 10;
        s.remote.enabled = true;
        s.remote.port = 80;
        assert!(s.validate().is_err());
        s.remote.port = 47810;
        s.backups.enabled = true;
        assert!(s.validate().is_err());
    }

    #[test]
    fn theme_validation_enforces_required_colors() {
        let mut theme = ThemeDefinition::default();
        assert!(theme.validate().is_err());
        for key in REQUIRED_COLORS {
            theme.colors.insert((*key).into(), "#123456".into());
        }
        theme.validate().unwrap();
        theme.colors.insert("bg".into(), "not-a-colour".into());
        assert!(theme.validate().is_err());
        theme.colors.insert("bg".into(), "#000".into());
        theme.validate().unwrap();
        theme.background_image = Some("file:///etc/passwd".into());
        assert!(theme.validate().is_err());
        theme.background_image = None;
        theme.radius_px = 40;
        assert!(theme.validate().is_err());
    }

    #[test]
    fn settings_round_trip_json() {
        let mut s = Settings::default();
        s.language = Language::Ja;
        s.density = Density::UltraCompact;
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn every_density_has_increasing_compactness() {
        assert!(Density::Comfortable.row_height() > Density::Compact.row_height());
        assert!(Density::Compact.row_height() > Density::UltraCompact.row_height());
    }
}
