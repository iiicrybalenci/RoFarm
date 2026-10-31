use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::fleet::FleetState;

pub struct Redactor {
    usernames: Vec<String>,
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor").field("usernames", &self.usernames.len()).finish()
    }
}

impl Redactor {
    pub fn new(usernames: impl IntoIterator<Item = String>) -> Self {
        Self { usernames: usernames.into_iter().filter(|u| u.len() >= 2).collect() }
    }

    pub fn empty() -> Self {
        Self { usernames: Vec::new() }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for token in split_keep_ws(text) {
            out.push_str(&self.redact_token(token));
        }
        for user in &self.usernames {
            if out.contains(user) {
                out = out.replace(user, "[account]");
            }
        }
        out
    }

    fn redact_token(&self, token: &str) -> String {
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("_|warning") || lower.contains("roblosecurity") && lower.contains('=') {
            return "[cookie]".into();
        }
        if token.len() > 40
            && (token.chars().all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_')
                || token.starts_with("eyJ"))
        {
            return "[redacted]".into();
        }
        if lower.starts_with("https://discord.com/api/webhooks/")
            || lower.starts_with("https://discordapp.com/api/webhooks/")
            || lower.starts_with("https://api.telegram.org/bot")
            || lower.starts_with("https://hooks.slack.com/")
        {
            return "[webhook]".into();
        }
        if lower.contains("cookie=") || lower.contains("token=") || lower.contains("password=") {
            let key = token.split('=').next().unwrap_or("");
            return format!("{key}=[redacted]");
        }
        token.to_string()
    }
}

fn split_keep_ws(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, c) in text.char_indices() {
        if c.is_whitespace() {
            if start < idx {
                parts.push(&text[start..idx]);
            }
            parts.push(&text[idx..idx + c.len_utf8()]);
            start = idx + c.len_utf8();
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub at: DateTime<Utc>,
    pub checks: Vec<DoctorCheck>,
    pub score: u8,
}

impl DoctorReport {
    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|c| c.status == CheckStatus::Fail).count()
    }
    pub fn warnings(&self) -> usize {
        self.checks.iter().filter(|c| c.status == CheckStatus::Warn).count()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoctorContext {
    pub vault_exists: bool,
    pub vault_unlocked: bool,
    pub event_chain_ok: bool,
    pub data_dir_writable: bool,
    pub free_disk_mb: u64,
    pub webview2_version: Option<String>,
    pub windows_build: Option<u32>,
    pub roblox_installed: bool,
    pub roblox_version: Option<String>,
    pub tpm_available: bool,
    pub hello_available: bool,
    pub administrator: bool,
    pub last_backup: Option<DateTime<Utc>>,
    pub clock_skew_seconds: i64,
    pub memory_available_mb: u64,
    pub firewall_rule: bool,
    pub auto_lock_minutes: u32,
    pub telemetry_free: bool,
}

pub fn run_doctor(ctx: &DoctorContext, now: DateTime<Utc>) -> DoctorReport {
    let mut checks = Vec::new();
    let mut push = |id: &str, label: &str, status: CheckStatus, detail: String, fix: &str| {
        checks.push(DoctorCheck { id: id.into(), label: label.into(), status, detail, fix: fix.into() });
    };
    push(
        "data_dir",
        "Data directory",
        if ctx.data_dir_writable { CheckStatus::Pass } else { CheckStatus::Fail },
        if ctx.data_dir_writable { "RoFarm can write its data directory.".into() } else { "RoFarm cannot write to its data directory.".into() },
        "Check permissions on %LOCALAPPDATA%\\RoFarm and your antivirus quarantine.",
    );
    push(
        "vault",
        "Vault",
        if ctx.vault_exists && ctx.vault_unlocked { CheckStatus::Pass } else if ctx.vault_exists { CheckStatus::Warn } else { CheckStatus::Fail },
        if !ctx.vault_exists { "No vault found.".into() } else if ctx.vault_unlocked { "Vault is open.".into() } else { "Vault is locked (that is normal when idle).".into() },
        "Create a vault in Settings → Vault.",
    );
    push(
        "event_chain",
        "Event log integrity",
        if ctx.event_chain_ok { CheckStatus::Pass } else { CheckStatus::Fail },
        if ctx.event_chain_ok { "Hash chain verified.".into() } else { "The event hash chain is broken.".into() },
        "Restore from an encrypted backup, then report a bug with the diagnostics bundle.",
    );
    push(
        "webview2",
        "WebView2 runtime",
        if ctx.webview2_version.is_some() { CheckStatus::Pass } else { CheckStatus::Fail },
        ctx.webview2_version.clone().unwrap_or_else(|| "Not installed.".into()),
        "Install the Evergreen WebView2 runtime from Microsoft, or use the portable bundle.",
    );
    push(
        "windows",
        "Windows version",
        match ctx.windows_build {
            Some(b) if b >= 22621 => CheckStatus::Pass,
            Some(b) if b >= 17763 => CheckStatus::Warn,
            Some(_) => CheckStatus::Fail,
            None => CheckStatus::Skipped,
        },
        ctx.windows_build.map(|b| format!("Build {b}")).unwrap_or_else(|| "Unknown.".into()),
        "RoFarm supports Windows 10 1809+ and Windows 11. Mica and Acrylic need Windows 11.",
    );
    push(
        "roblox",
        "Roblox client",
        if ctx.roblox_installed { CheckStatus::Pass } else { CheckStatus::Warn },
        ctx.roblox_version.clone().unwrap_or_else(|| "Roblox is not installed.".into()),
        "Install Roblox from roblox.com so RoFarm can launch clients.",
    );
    push(
        "tpm",
        "TPM 2.0",
        if ctx.tpm_available { CheckStatus::Pass } else { CheckStatus::Warn },
        if ctx.tpm_available { "Available for key sealing.".into() } else { "No TPM available; passphrase + DPAPI only.".into() },
        "Enable TPM 2.0 in firmware for hardware-backed sealing.",
    );
    push(
        "hello",
        "Windows Hello",
        if ctx.hello_available { CheckStatus::Pass } else { CheckStatus::Warn },
        if ctx.hello_available { "Available for unlock.".into() } else { "Windows Hello is not configured.".into() },
        "Set up a PIN or biometric in Windows Settings → Accounts → Sign-in options.",
    );
    push(
        "clock",
        "System clock",
        if ctx.clock_skew_seconds.abs() <= 300 { CheckStatus::Pass } else { CheckStatus::Warn },
        format!("Skew {}s", ctx.clock_skew_seconds),
        "Enable automatic time sync; TLS and 2FA codes depend on an accurate clock.",
    );
    push(
        "disk",
        "Free space",
        if ctx.free_disk_mb > 4096 { CheckStatus::Pass } else if ctx.free_disk_mb > 512 { CheckStatus::Warn } else { CheckStatus::Fail },
        format!("{} MB free", ctx.free_disk_mb),
        "Free at least 4 GB for updates, backups and the Roblox client.",
    );
    push(
        "memory",
        "Available memory",
        if ctx.memory_available_mb > 2048 { CheckStatus::Pass } else { CheckStatus::Warn },
        format!("{} MB available", ctx.memory_available_mb),
        "Close other memory-heavy apps or lower the resource profile.",
    );
    push(
        "backup",
        "Backup freshness",
        match ctx.last_backup {
            Some(t) if (now - t).num_days() <= 7 => CheckStatus::Pass,
            Some(t) => {
                if (now - t).num_days() > 30 { CheckStatus::Fail } else { CheckStatus::Warn }
            }
            None => CheckStatus::Warn,
        },
        ctx.last_backup.map(|t| format!("Last backup {}", t.format("%Y-%m-%d"))).unwrap_or_else(|| "No encrypted backup found.".into()),
        "Enable scheduled encrypted backups in Settings → Backups.",
    );
    push(
        "auto_lock",
        "Auto-lock",
        if ctx.auto_lock_minutes <= 15 { CheckStatus::Pass } else { CheckStatus::Warn },
        format!("Locks after {} minutes", ctx.auto_lock_minutes),
        "Set auto-lock to 15 minutes or less.",
    );
    push(
        "firewall",
        "Remote dashboard firewall",
        if ctx.firewall_rule { CheckStatus::Pass } else { CheckStatus::Skipped },
        if ctx.firewall_rule { "A RoFarm firewall rule exists.".into() } else { "Remote dashboard is off or no rule was added.".into() },
        "Only add a firewall rule if you intentionally use the remote dashboard.",
    );
    push(
        "privacy",
        "Telemetry",
        if ctx.telemetry_free { CheckStatus::Pass } else { CheckStatus::Fail },
        if ctx.telemetry_free { "No telemetry is enabled.".into() } else { "Unexpected network analytics configured.".into() },
        "RoFarm must never send analytics. Review the network allow-list.",
    );
    push(
        "admin",
        "Privilege level",
        if ctx.administrator { CheckStatus::Warn } else { CheckStatus::Pass },
        if ctx.administrator { "RoFarm is running elevated.".into() } else { "Running as a normal user (recommended).".into() },
        "Run RoFarm unelevated; only QoS policy needs a UAC prompt on demand.",
    );

    let fails = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let warns = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let score = (100i32 - fails as i32 * 20 - warns as i32 * 5).clamp(0, 100) as u8;
    DoctorReport { at: now, checks, score }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsBundle {
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub app_version: String,
    pub report: DoctorReport,
    pub summary: crate::fleet::FleetSummary,
    pub accounts_redacted: Vec<String>,
    pub event_counts: BTreeMap<String, u64>,
    pub logs: Vec<String>,
    pub settings_redacted: serde_json::Value,
}

pub fn build_bundle(
    state: &FleetState,
    report: DoctorReport,
    app_version: &str,
    logs: &[String],
) -> DiagnosticsBundle {
    let redactor = Redactor::new(state.accounts.values().map(|a| a.username.clone()));
    let mut event_counts = BTreeMap::new();
    let mut accounts_redacted = Vec::new();
    for a in state.accounts.values() {
        accounts_redacted.push(format!("account-{}", &a.id.to_string()[..8]));
        let _ = a;
    }
    for session in &state.sessions {
        *event_counts.entry(session.outcome.clone()).or_insert(0) += 1;
    }
    let settings_json = serde_json::to_value(&state.settings).unwrap_or(serde_json::Value::Null);
    let redacted_json = redact_json(&settings_json, &redactor);
    DiagnosticsBundle {
        schema_version: 1,
        created_at: Utc::now(),
        app_version: app_version.into(),
        report,
        summary: state.summary(),
        accounts_redacted,
        event_counts,
        logs: logs.iter().map(|l| redactor.redact(l)).collect(),
        settings_redacted: redacted_json,
    }
}

fn redact_json(value: &serde_json::Value, redactor: &Redactor) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(redactor.redact(s)),
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(|v| redact_json(v, redactor)).collect()),
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.iter()
                .map(|(k, v)| {
                    let lower = k.to_ascii_lowercase();
                    if lower.contains("token") || lower.contains("secret") || lower.contains("password") || lower.contains("cookie") {
                        (k.clone(), serde_json::Value::String("[redacted]".into()))
                    } else {
                        (k.clone(), redact_json(v, redactor))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_removes_cookies_webhooks_tokens_and_usernames() {
        let r = Redactor::new(vec!["Builderman".to_string()]);
        let text = "cookie _|WARNING:-DO-NOT-SHARE|_ABCDEFGHIJKLMNOP user Builderman webhook https://discord.com/api/webhooks/123/abc \
                    token eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.signature \
                    password=hunter2";
        let out = r.redact(text);
        assert!(!out.contains("DO-NOT-SHARE"));
        assert!(!out.contains("Builderman"));
        assert!(!out.contains("123/abc"));
        assert!(!out.contains("eyJhbGci"));
        assert!(!out.contains("hunter2"));
        assert!(out.contains("[redacted]"));
        assert!(out.contains("[webhook]"));
        assert!(out.contains("[account]"));
    }

    #[test]
    fn doctor_scores_healthy_and_broken_setups() {
        let now = Utc::now();
        let ctx = DoctorContext {
            vault_exists: true,
            vault_unlocked: true,
            event_chain_ok: true,
            data_dir_writable: true,
            free_disk_mb: 50_000,
            webview2_version: Some("156.0".into()),
            windows_build: Some(22631),
            roblox_installed: true,
            roblox_version: Some("version-abc".into()),
            tpm_available: true,
            hello_available: true,
            administrator: false,
            last_backup: Some(now - chrono::Duration::days(1)),
            clock_skew_seconds: 2,
            memory_available_mb: 8192,
            firewall_rule: false,
            auto_lock_minutes: 10,
            telemetry_free: true,
        };
        let report = run_doctor(&ctx, now);
        assert_eq!(report.failed(), 0);
        assert_eq!(report.warnings(), 0);
        assert!(report.score >= 95);

        let broken = DoctorContext { event_chain_ok: false, data_dir_writable: false, webview2_version: None, ..DoctorContext::default() };
        let report = run_doctor(&broken, now);
        assert!(report.failed() >= 3);
        assert!(report.score < 50);
        assert!(report.checks.iter().any(|c| c.id == "backup" && c.status == CheckStatus::Warn));
    }

    #[test]
    fn bundle_redacts_settings_and_accounts() {
        let mut state = FleetState::default();
        state.accounts.insert(crate::ids::AccountId::new(), crate::account::Account::new("SecretUser", crate::ids::builtin::farming()));
        state.settings.remote.pinned_fingerprint = Some("deadbeef".into());
        state.settings.notifications.notifiers.push(crate::settings::NotifierConfig {
            kind: crate::settings::NotifierKind::Discord,
            enabled: true,
            vault_key: "notifier.discord.1".into(),
            events: Default::default(),
            min_severity: 1,
        });
        let report = run_doctor(&DoctorContext::default(), Utc::now());
        let bundle = build_bundle(&state, report, "0.1.0", &["SecretUser logs in".to_string()]);
        assert!(bundle.accounts_redacted.iter().all(|a| a.starts_with("account-")));
        assert!(!bundle.logs[0].contains("SecretUser"));
        assert_eq!(bundle.schema_version, 1);
    }
}
