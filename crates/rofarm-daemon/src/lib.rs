use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rofarm_core::account::Account;
use rofarm_core::client::{ClientRuntime, ClientState, ExitReason, KillSource};
use rofarm_core::events::Actor;
use rofarm_core::fleet::FleetState;
use rofarm_core::ids::{AccountId, ClientId, LaneId, LaunchProfileId};
use rofarm_core::lane::{AntiAfkMode, ResourceLimits};
use rofarm_core::launch::{parse_target, ClientSettings, LaunchProfile, TargetKind};
use rofarm_core::service::{FleetService, QueuedBatch};
use rofarm_core::settings::Settings;
use rofarm_core::window::{grid, Rect, WindowPlacement};
use rofarm_roblox::api::{RobloxApi, SessionCookie};
use rofarm_roblox::launch::{find_installed_clients, ClientSettingsFile, InstalledClient};
use rofarm_roblox::net::NetPolicy;
use rofarm_vault::audit::{AuditKind, AuditLog};
use rofarm_vault::kdf::{calibrate, KdfParams};
use rofarm_vault::vault::Vault;
use rofarm_win::{Platform, ProcessMetrics, WindowId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Semaphore;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Core(#[from] rofarm_core::CoreError),
    #[error("{0}")]
    Vault(#[from] rofarm_vault::VaultError),
    #[error("{0}")]
    Roblox(#[from] rofarm_roblox::RobloxError),
    #[error("{0}")]
    Platform(#[from] rofarm_win::PlatformError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("vault is locked")]
    Locked,
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOptions {
    pub multi_instance: bool,
    pub anti_afk: bool,
    pub auto_tile: bool,
    pub launch_timeout: Duration,
    pub max_parallel_launches: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            multi_instance: true,
            anti_afk: true,
            auto_tile: true,
            launch_timeout: Duration::from_secs(25),
            max_parallel_launches: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub unlocked: bool,
    pub generation: u64,
    pub secrets: usize,
    pub has_recovery: bool,
    pub lockout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveClient {
    pub client_id: ClientId,
    pub account_id: AccountId,
    pub state: ClientState,
    pub pid: Option<u32>,
    pub hwnd: Option<i64>,
    pub cpu_pct: f32,
    pub mem_mb: u32,
    pub threads: u32,
    pub handles: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub state_since: DateTime<Utc>,
    pub target: Option<String>,
    pub restarts: u32,
    pub last_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetTotals {
    pub running: u32,
    pub cpu_pct: f32,
    pub mem_mb: u64,
    pub restarts: u32,
    pub crashes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub fleet: FleetState,
    pub live: Vec<LiveClient>,
    pub totals: FleetTotals,
    pub vault: VaultStatus,
    pub roblox_clients: Vec<InstalledRoblox>,
    pub options: RuntimeOptions,
    pub taken_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledRoblox {
    pub version: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOutcome {
    pub client_id: ClientId,
    pub account_id: AccountId,
    pub state: ClientState,
    pub pid: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOutcome {
    pub batch_id: uuid::Uuid,
    pub outcomes: Vec<LaunchOutcome>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTarget {
    pub place_id: u64,
    pub job_id: Option<String>,
    pub private_server_code: Option<String>,
    pub raw: String,
    pub username: Option<String>,
}

#[derive(Debug)]
struct Tracked {
    client_id: ClientId,
    account_id: AccountId,
    pid: u32,
    hwnd: Option<WindowId>,
    placed: bool,
    settings: Option<ClientSettingsFile>,
    started_at: DateTime<Utc>,
    next_anti_afk: Option<DateTime<Utc>>,
    anti_afk_tick: u64,
    exit_code: Arc<AtomicI32>,
    windows_checked: u32,
    high_memory_ticks: u32,
    profile_window: WindowPlacement,
    limits: ResourceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickReport {
    pub changed: bool,
    pub exited: Vec<ClientId>,
    pub restarted: Vec<ClientId>,
    pub trimmed: Vec<ClientId>,
    pub anti_afk_sent: u32,
    pub totals: FleetTotals,
}

pub struct FleetRuntime {
    service: Arc<FleetService>,
    platform: Arc<dyn Platform>,
    api: Arc<dyn RobloxApi>,
    vault: Mutex<Option<Vault>>,
    vault_path: PathBuf,
    audit: Mutex<AuditLog>,
    tracked: Mutex<HashMap<u32, Tracked>>,
    by_client: Mutex<HashMap<ClientId, u32>>,
    metrics: Mutex<HashMap<ClientId, ProcessMetrics>>,
    options: Mutex<RuntimeOptions>,
    launch_gate: Semaphore,
}

impl std::fmt::Debug for FleetRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FleetRuntime")
            .field("platform", &self.platform.name())
            .field("tracked", &self.tracked.lock().len())
            .field("vault", &self.vault_path)
            .finish()
    }
}

impl FleetRuntime {
    pub fn new(
        service: Arc<FleetService>,
        platform: Arc<dyn Platform>,
        api: Arc<dyn RobloxApi>,
        vault_path: impl Into<PathBuf>,
    ) -> Self {
        let vault_path = vault_path.into();
        let audit_path = vault_path.with_extension("audit.jsonl");
        let audit = AuditLog::open(audit_path).unwrap_or_else(|_| AuditLog::in_memory());
        let options = RuntimeOptions::default();
        let launch_gate = Semaphore::new(options.max_parallel_launches);
        Self {
            service,
            platform,
            api,
            vault: Mutex::new(None),
            vault_path,
            audit: Mutex::new(audit),
            tracked: Mutex::new(HashMap::new()),
            by_client: Mutex::new(HashMap::new()),
            metrics: Mutex::new(HashMap::new()),
            options: Mutex::new(options),
            launch_gate,
        }
    }

    pub fn service(&self) -> &Arc<FleetService> {
        &self.service
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }

    pub fn set_options(&self, options: RuntimeOptions) {
        *self.options.lock() = options;
    }

    pub fn options(&self) -> RuntimeOptions {
        self.options.lock().clone()
    }

    pub fn create_vault(&self, passphrase: &str) -> Result<VaultStatus> {
        if passphrase.chars().count() < 10 {
            return Err(RuntimeError::Invalid("Use a passphrase of at least 10 characters.".into()));
        }
        let params: KdfParams = calibrate(500, true);
        let mut vault = Vault::create(&self.vault_path, passphrase, params)?;
        vault.unlock(passphrase)?;
        vault.save()?;
        self.audit.lock().append(AuditKind::VaultCreated, "vault", "created")?;
        *self.vault.lock() = Some(vault);
        self.status()
    }

    pub fn open_vault(&self, passphrase: &str) -> Result<VaultStatus> {
        let mut vault = Vault::open_file(&self.vault_path)?;
        vault.unlock(passphrase)?;
        vault.save()?;
        self.audit.lock().append(AuditKind::Unlock, "vault", "passphrase")?;
        *self.vault.lock() = Some(vault);
        self.status()
    }

    pub fn unlock_with_recovery(&self, code: &str) -> Result<VaultStatus> {
        let mut vault = Vault::open_file(&self.vault_path)?;
        vault.unlock_with_recovery(code)?;
        vault.save()?;
        self.audit.lock().append(AuditKind::Unlock, "vault", "recovery code")?;
        *self.vault.lock() = Some(vault);
        self.status()
    }

    pub fn lock_vault(&self) {
        if let Some(mut vault) = self.vault.lock().take() {
            let _ = vault.save();
            vault.lock();
        }
        let _ = self.audit.lock().append(AuditKind::Lock, "vault", "manual");
    }

    pub fn status(&self) -> Result<VaultStatus> {
        let guard = self.vault.lock();
        let exists = self.vault_path.exists();
        match guard.as_ref() {
            Some(v) => Ok(VaultStatus {
                path: self.vault_path.clone(),
                exists,
                unlocked: v.is_unlocked(),
                generation: v.generation(),
                secrets: v.secret_count(),
                has_recovery: v.has_recovery(),
                lockout_seconds: v.lockout_remaining().map(|d| d.num_seconds().max(0) as u64),
            }),
            None => {
                let (generation, secrets, recovery, lockout) = if exists {
                    match Vault::open_file(&self.vault_path) {
                        Ok(v) => (
                            v.generation(),
                            v.secret_count(),
                            v.has_recovery(),
                            v.lockout_remaining().map(|d| d.num_seconds().max(0) as u64),
                        ),
                        Err(_) => (0, 0, false, None),
                    }
                } else {
                    (0, 0, false, None)
                };
                Ok(VaultStatus {
                    path: self.vault_path.clone(),
                    exists,
                    unlocked: false,
                    generation,
                    secrets,
                    has_recovery: recovery,
                    lockout_seconds: lockout,
                })
            }
        }
    }

    pub fn store_cookie(&self, account_id: AccountId, cookie: &str) -> Result<()> {
        let mut guard = self.vault.lock();
        let vault = guard.as_mut().ok_or(RuntimeError::Locked)?;
        vault.put_account_secret(account_id, "cookie", cookie.as_bytes())?;
        self.audit.lock().append(AuditKind::SecretWritten, &account_id.to_string(), "cookie")?;
        Ok(())
    }

    pub fn reveal_cookie(&self, account_id: AccountId) -> Result<String> {
        let mut guard = self.vault.lock();
        let vault = guard.as_mut().ok_or(RuntimeError::Locked)?;
        let secret = vault.get_account_secret(account_id, "cookie")?;
        self.audit.lock().append(AuditKind::Reveal, &account_id.to_string(), "cookie")?;
        String::from_utf8(secret.to_vec()).map_err(|_| RuntimeError::Invalid("Cookie is not valid UTF-8.".into()))
    }

    pub fn reauthenticate(&self, passphrase: &str) -> Result<bool> {
        let mut guard = self.vault.lock();
        let vault = guard.as_mut().ok_or(RuntimeError::Locked)?;
        Ok(vault.reauthenticate(passphrase)?)
    }

    pub fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> rofarm_vault::VaultResult<T>) -> Result<T> {
        let mut guard = self.vault.lock();
        let vault = guard.as_mut().ok_or(RuntimeError::Locked)?;
        Ok(f(vault)?)
    }

    pub fn issue_recovery_code(&self) -> Result<String> {
        let code = rofarm_vault::generate_recovery_code();
        self.with_vault(|vault| vault.set_recovery_code(&code))?;
        self.audit.lock().append(AuditKind::RecoveryIssued, "vault", "9-character recovery code")?;
        Ok(code)
    }

    pub fn recovery_shares(&self, threshold: u8, shares: u8) -> Result<Vec<String>> {
        let parts = self.with_vault(|vault| vault.create_recovery_shares(threshold, shares))?;
        self.audit.lock().append(AuditKind::RecoveryIssued, "vault", &format!("{threshold}-of-{shares} shares"))?;
        Ok(parts)
    }

    pub fn change_passphrase(&self, current: &str, new: &str) -> Result<()> {
        self.with_vault(|vault| {
            if !vault.reauthenticate(current)? {
                return Err(rofarm_vault::VaultError::Invalid("Current passphrase is incorrect.".into()));
            }
            vault.change_passphrase(new, calibrate(500, true))
        })?;
        self.audit.lock().append(AuditKind::PassphraseChanged, "vault", "passphrase")?;
        Ok(())
    }

    pub fn backup_vault(&self, destination: &Path, passphrase: &str) -> Result<rofarm_vault::backup::BackupInfo> {
        if self.tracked.lock().len() > 0 {
            return Err(RuntimeError::Invalid("Stop running clients before creating a backup.".into()));
        }
        let mut archive = rofarm_vault::backup::BackupArchive::default();
        archive.insert("vault.rofarmvault", std::fs::read(&self.vault_path)?)?;
        let audit_path = self.vault_path.with_extension("audit.jsonl");
        if audit_path.exists() {
            archive.insert("audit.jsonl", std::fs::read(audit_path)?)?;
        }
        let info = rofarm_vault::backup::write_backup(
            destination,
            passphrase,
            &archive,
            env!("CARGO_PKG_VERSION"),
            calibrate(500, true),
        )?;
        self.audit.lock().append(AuditKind::BackupCreated, "vault", &destination.display().to_string())?;
        Ok(info)
    }

    pub fn verify_backup(&self, source: &Path, passphrase: &str) -> Result<rofarm_vault::backup::BackupInfo> {
        let info = rofarm_vault::backup::verify_backup(source, passphrase)?;
        self.audit.lock().append(AuditKind::BackupVerified, "vault", &source.display().to_string())?;
        Ok(info)
    }

    pub fn restore_backup(&self, source: &Path, passphrase: &str) -> Result<u64> {
        if self.tracked.lock().len() > 0 {
            return Err(RuntimeError::Invalid("Stop running clients before restoring a backup.".into()));
        }
        let (info, archive) = rofarm_vault::backup::read_backup(source, passphrase)?;
        let data = archive
            .files()
            .get("vault.rofarmvault")
            .ok_or_else(|| RuntimeError::Invalid("Backup does not contain a vault.".into()))?;
        self.lock_vault();
        let temp = self.vault_path.with_extension("restore.tmp");
        std::fs::write(&temp, data)?;
        let check = Vault::open_file(&temp)?;
        if check.generation() == 0 {
            return Err(RuntimeError::Invalid("Backup vault is invalid.".into()));
        }
        std::fs::rename(&temp, &self.vault_path)?;
        self.audit.lock().append(AuditKind::Restore, "vault", &source.display().to_string())?;
        Ok(info.header.total_size)
    }

    pub fn audit_entries(&self) -> Vec<rofarm_vault::audit::AuditEntry> {
        self.audit.lock().entries().to_vec()
    }

    pub fn verify_audit(&self) -> bool {
        self.audit.lock().verify().is_ok()
    }

    fn cookie(&self, account_id: AccountId) -> Result<SessionCookie> {
        let mut guard = self.vault.lock();
        let vault = guard.as_mut().ok_or(RuntimeError::Locked)?;
        let secret = vault.get_account_secret(account_id, "cookie")?;
        let text = String::from_utf8(secret.to_vec())
            .map_err(|_| RuntimeError::Invalid("Stored cookie is not valid UTF-8.".into()))?;
        Ok(SessionCookie::new(text))
    }

    pub async fn launch(
        &self,
        actor: &Actor,
        account_ids: &[AccountId],
        mut profile: LaunchProfile,
    ) -> Result<BatchOutcome> {
        if account_ids.is_empty() {
            return Err(RuntimeError::Invalid("Select at least one account.".into()));
        }
        if !self.status()?.unlocked {
            return Err(RuntimeError::Locked);
        }
        let lane_target = {
            let state = self.service.snapshot();
            account_ids
                .first()
                .and_then(|id| state.accounts.get(id))
                .and_then(|a| state.lanes.get(&a.lane_id))
                .and_then(|l| l.rules.default_target.clone())
        };
        if profile.target.trim().is_empty() {
            profile.target = lane_target.ok_or_else(|| {
                RuntimeError::Invalid("This profile has no target and the lane has no default game.".into())
            })?;
        }
        let same_server_job = if profile.server_mode == rofarm_core::launch::ServerMode::SameServer {
            self.pick_server(account_ids[0], &profile.target).await.ok()
        } else {
            None
        };
        let batch = self.service.queue_clients(actor, account_ids, &profile, same_server_job.clone())?;
        self.run_batch(actor, batch, &profile, same_server_job).await
    }

    async fn run_batch(
        &self,
        actor: &Actor,
        batch: QueuedBatch,
        profile: &LaunchProfile,
        same_server_job: Option<String>,
    ) -> Result<BatchOutcome> {
        let mut outcomes = Vec::new();
        let mut previous_offset = 0u64;
        for (idx, client) in batch.clients.iter().enumerate() {
            let offset = batch.offsets_ms.get(idx).copied().unwrap_or(0);
            let delay = offset.saturating_sub(previous_offset);
            previous_offset = offset;
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            let permit = self.launch_gate.acquire().await;
            let result = self.launch_client(actor, client, profile, same_server_job.clone()).await;
            drop(permit);
            match result {
                Ok(pid) => outcomes.push(LaunchOutcome {
                    client_id: client.id,
                    account_id: client.account_id,
                    state: ClientState::InGame,
                    pid: Some(pid),
                    error: None,
                }),
                Err(error) => {
                    let message = error.to_string();
                    let _ = self.service.mark_client_state(actor, client.id, ClientState::Failed, Some(message.clone()));
                    outcomes.push(LaunchOutcome {
                        client_id: client.id,
                        account_id: client.account_id,
                        state: ClientState::Failed,
                        pid: None,
                        error: Some(message),
                    });
                }
            }
        }
        Ok(BatchOutcome { batch_id: batch.batch_id, outcomes, warnings: batch.warnings })
    }

    async fn launch_client(
        &self,
        actor: &Actor,
        client: &ClientRuntime,
        profile: &LaunchProfile,
        same_server_job: Option<String>,
    ) -> Result<u32> {
        self.service.mark_client_state(actor, client.id, ClientState::Launching, Some("Preparing".into()))?;
        let account = self
            .service
            .account(client.account_id)
            .ok_or_else(|| RuntimeError::Invalid("Account disappeared.".into()))?;
        let cookie = self.cookie(account.id)?;
        let target = self.resolve_target(&cookie, profile, same_server_job).await?;
        let ticket = self.api.auth_ticket(&cookie, target.place_id).await?;
        let uri = if target.raw.starts_with("roblox-player:") || target.raw.starts_with("roblox://") {
            target.raw.clone()
        } else {
            rofarm_roblox::launch::build_launch_uri(
                &ticket,
                target.place_id,
                target.job_id.as_deref(),
                target.private_server_code.as_deref(),
                Utc::now().timestamp_millis(),
            )
        };
        let settings_guard = self.install_client_settings(profile);
        if self.options.lock().multi_instance {
            let _ = self.platform.release_singleton_mutex();
        }
        let before: HashSet<u32> = self
            .platform
            .list_roblox_processes()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.pid)
            .collect();
        self.platform.launch_uri(&uri)?;
        let pid = self.wait_for_pid(&before).await?;
        if profile.limits.cpu_percent.is_some() || profile.limits.memory_mb.is_some() {
            if let Err(error) = self.platform.apply_limits(pid, &profile.limits) {
                tracing::warn!(pid, %error, "could not attach job limits");
            }
        }
        self.service.attach_client_process(actor, client.id, pid, None)?;
        self.service.mark_client_state(actor, client.id, ClientState::InGame, Some("Running".into()))?;
        let exit_code = self.platform.watch_process(pid);
        let anti_afk_at = self.anti_afk_schedule(&account, 0);
        {
            let mut tracked = self.tracked.lock();
            self.by_client.lock().insert(client.id, pid);
            tracked.insert(
                pid,
                Tracked {
                    client_id: client.id,
                    account_id: account.id,
                    pid,
                    hwnd: None,
                    placed: false,
                    settings: settings_guard,
                    started_at: Utc::now(),
                    next_anti_afk: anti_afk_at,
                    anti_afk_tick: 0,
                    exit_code,
                    windows_checked: 0,
                    high_memory_ticks: 0,
                    profile_window: profile.window.clone(),
                    limits: profile.limits.clone(),
                },
            );
        }        let _ = self.audit.lock().append(AuditKind::ClientLaunched, &client.id.to_string(), &account.username);
        Ok(pid)
    }

    async fn wait_for_pid(&self, before: &HashSet<u32>) -> Result<u32> {
        let deadline = tokio::time::Instant::now() + self.options.lock().launch_timeout;
        loop {
            if let Ok(processes) = self.platform.list_roblox_processes() {
                if let Some(found) = processes.into_iter().find(|p| !before.contains(&p.pid)) {
                    return Ok(found.pid);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(RuntimeError::Invalid(
                    "Roblox did not start. Check that the client is installed and signed in.".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn install_client_settings(&self, profile: &LaunchProfile) -> Option<ClientSettingsFile> {
        let installed = find_installed_clients();
        let latest = installed.first()?;
        let settings = ClientSettings::from_profile(profile);
        ClientSettingsFile::install(&latest.directory, &settings).ok()
    }

    pub async fn resolve_target(
        &self,
        cookie: &SessionCookie,
        profile: &LaunchProfile,
        same_server_job: Option<String>,
    ) -> Result<ResolvedTarget> {
        let mut target = parse_target(&profile.target)?;
        if profile.server_mode == rofarm_core::launch::ServerMode::FollowFriend {
            if let Some(friend) = profile.follow_username.as_deref().map(|s| s.trim_start_matches('@')) {
                let id = self
                    .api
                    .username_to_id(cookie, friend)
                    .await?
                    .ok_or_else(|| RuntimeError::Invalid(format!("@{friend} was not found.")))?;
                let presence = self
                    .api
                    .presence(cookie, &[id])
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| RuntimeError::Invalid(format!("@{friend} is offline.")))?;
                let place = presence
                    .place_id
                    .or(presence.root_place_id)
                    .ok_or_else(|| RuntimeError::Invalid(format!("@{friend} is not in a game.")))?;
                target.place_id = Some(place);
                target.game_instance_id = presence.game_instance_id.clone();
                target.kind = TargetKind::PlaceWithServer;
                target.username = Some(friend.to_string());
            }
        }
        if target.kind == TargetKind::ShareCode {
            if let Some(code) = target.share_code.clone() {
                let resolved = self.api.resolve_share_link(cookie, &code).await?;
                target.place_id = resolved.place_id;
                target.game_instance_id = resolved.game_instance_id;
                target.private_server_code = resolved.private_server_link_code;
                target.kind = TargetKind::PlaceWithServer;
            }
        }
        if target.kind == TargetKind::Profile {
            return Err(RuntimeError::Invalid(
                "That is a profile link. Pick a game for this account before launching.".into(),
            ));
        }
        let place_id = target
            .place_id
            .ok_or_else(|| RuntimeError::Invalid("Could not find a place ID in that target.".into()))?;
        Ok(ResolvedTarget {
            place_id,
            job_id: same_server_job.or(target.game_instance_id),
            private_server_code: target.private_server_code,
            raw: target.raw,
            username: target.username,
        })
    }

    async fn pick_server(&self, account_id: AccountId, raw: &str) -> Result<String> {
        let cookie = self.cookie(account_id)?;
        let target = parse_target(raw)?;
        let place = target
            .place_id
            .ok_or_else(|| RuntimeError::Invalid("Same-server mode needs a game target.".into()))?;
        let servers = self.api.servers(&cookie, place, None).await?;
        servers
            .data
            .into_iter()
            .filter(|s| s.playing < s.max_players)
            .min_by_key(|s| (s.ping.unwrap_or(999), s.playing))
            .map(|s| s.job_id)
            .ok_or_else(|| RuntimeError::Invalid("No joinable public servers right now.".into()))
    }

    pub fn tick(&self) -> Result<TickReport> {
        let pids: Vec<u32> = self.tracked.lock().keys().copied().collect();
        let metrics: HashMap<u32, ProcessMetrics> =
            self.platform.process_metrics(&pids).into_iter().map(|m| (m.pid, m)).collect();
        let mut exited = Vec::new();
        let mut restarted = Vec::new();
        let mut trimmed = Vec::new();
        let mut anti_afk_sent = 0;
        let mut changed = false;

        let mut done: Vec<u32> = Vec::new();
        {
            let mut tracked = self.tracked.lock();
            let state = self.service.snapshot();
            for (pid, track) in tracked.iter_mut() {
                let metric = metrics.get(pid).copied();
                let running = self.platform.is_running(*pid);
                if !running {
                    let code = track.exit_code.load(Ordering::Relaxed);
                    self.metrics.lock().insert(track.client_id, metric.unwrap_or_default());
                    let reason = if code == i32::MIN || code == 0 {
                        ExitReason::Normal
                    } else {
                        ExitReason::Crashed { code: Some(code) }
                    };
                    let client_id = track.client_id;
                    done.push(*pid);
                    exited.push(client_id);
                    changed = true;
                    if let Some(settings) = track.settings.take() {
                        drop(settings);
                    }
                    if self.service.client_exited(&Actor::System, client_id, reason).is_ok() {
                    }
                    continue;
                }
                if let Some(m) = metric {
                    self.metrics.lock().insert(track.client_id, m);
                    if let Some(limit) = track.limits.memory_mb {
                        if m.mem_mb > (limit as f32 * 1.4) as u32 && track.limits.trim_memory {
                            track.high_memory_ticks = track.high_memory_ticks.saturating_add(1);
                            if track.high_memory_ticks >= 3 {
                                let _ = self.platform.trim_working_set(*pid);
                                trimmed.push(track.client_id);
                                track.high_memory_ticks = 0;
                            }
                        } else {
                            track.high_memory_ticks = 0;
                        }
                    }
                }
                if track.hwnd.is_none() && track.windows_checked < 60 {
                    track.windows_checked += 1;
                    if let Ok(windows) = self.platform.list_windows(Some(*pid)) {
                        if let Some(win) = windows.into_iter().find(|w| w.visible && w.rect.width > 200) {
                            track.hwnd = Some(win.id);
                            changed = true;
                        }
                    }
                }
                if !track.placed {
                    if let (Some(hwnd), Some(placement)) = (track.hwnd, Some(&track.profile_window)) {
                        let rect = placement.bounds.unwrap_or_else(|| {
                            let area = self.platform.work_area();
                            let index = state.clients.values().filter(|c| c.state.is_running()).count();
                            let slots = grid(area, index.max(1), 8, 8);
                            slots.get(index.saturating_sub(1)).copied().unwrap_or(area)
                        });
                        let _ = self
                            .platform
                            .move_window(hwnd, rect, placement.always_on_top, placement.borderless);
                        track.placed = true;
                        changed = true;
                    }
                }
                if let Some(next) = track.next_anti_afk {
                    if Utc::now() >= next {
                        if let Some(client) = state.clients.get(&track.client_id) {
                            if let Some(lane) = state.lanes.get(&client.lane_id) {
                                if lane.rules.anti_afk.enabled
                                    && !lane.rules.anti_afk.opted_out_accounts.contains(&track.account_id)
                                {
                                    if let Some(hwnd) = track.hwnd {
                                        let foreground = lane.rules.anti_afk.mode == AntiAfkMode::Foreground;
                                        if self.platform.anti_afk(hwnd, foreground).is_ok() {
                                            anti_afk_sent += 1;
                                        }
                                    }
                                }
                                track.anti_afk_tick += 1;
                                track.next_anti_afk = Some(
                                    Utc::now()
                                        + chrono::Duration::seconds(
                                            lane.rules.anti_afk.jittered_interval(track.anti_afk_tick) as i64,
                                        ),
                                );
                            }
                        }
                    }
                }
                let _ = track.started_at;
            }
            for pid in &done {
                if let Some(t) = tracked.remove(pid) {
                    self.by_client.lock().remove(&t.client_id);
                }
            }
        }
        if exited.is_empty() && !changed && anti_afk_sent == 0 && trimmed.is_empty() {
        }
        let totals = self.totals();
        Ok(TickReport { changed, exited, restarted: std::mem::take(&mut restarted), trimmed, anti_afk_sent, totals })
    }

    pub fn anti_afk_schedule(&self, account: &Account, tick: u64) -> Option<DateTime<Utc>> {
        let state = self.service.snapshot();
        let lane = state.lanes.get(&account.lane_id)?;
        if !lane.rules.anti_afk.enabled || lane.rules.anti_afk.opted_out_accounts.contains(&account.id) {
            return None;
        }
        Some(Utc::now() + chrono::Duration::seconds(lane.rules.anti_afk.jittered_interval(tick) as i64))
    }

    pub fn totals(&self) -> FleetTotals {
        let state = self.service.snapshot();
        let metrics = self.metrics.lock();
        let mut totals = FleetTotals::default();
        for client in state.clients.values() {
            if client.state.is_running() {
                totals.running += 1;
            }
            totals.restarts += client.restart_count;
            totals.crashes += client.crash_count;
        }
        for m in metrics.values() {
            totals.cpu_pct += m.cpu_pct;
            totals.mem_mb += m.mem_mb as u64;
        }
        totals
    }

    pub fn kill_client(&self, actor: &Actor, client_id: ClientId) -> Result<bool> {
        let pid = self.by_client.lock().get(&client_id).copied();
        if let Some(pid) = pid {
            let _ = self.platform.terminate_process(pid);
        }
        let result = self.service.kill_client(actor, client_id, KillSource::Ui)?;
        if let Some(pid) = pid {
            self.tracked.lock().remove(&pid);
            self.by_client.lock().remove(&client_id);
            self.metrics.lock().remove(&client_id);
        }
        Ok(result)
    }

    pub fn kill_all(&self, actor: &Actor) -> Result<u32> {
        let pids: Vec<u32> = self.tracked.lock().keys().copied().collect();
        for pid in &pids {
            let _ = self.platform.terminate_process(*pid);
        }
        let count = self.service.kill_all(actor, KillSource::Ui)?;
        self.tracked.lock().clear();
        self.by_client.lock().clear();
        self.metrics.lock().clear();
        let _ = self.audit.lock().append(AuditKind::KillAll, "fleet", &format!("{count} clients"));
        Ok(count)
    }

    pub fn live_clients(&self) -> Vec<LiveClient> {
        let state = self.service.snapshot();
        let metrics = self.metrics.lock();
        let tracked = self.tracked.lock();
        let by_client: HashMap<ClientId, u32> = tracked.values().map(|t| (t.client_id, t.pid)).collect();
        let mut out: Vec<LiveClient> = state
            .clients
            .values()
            .map(|c| {
                let m = metrics.get(&c.id).copied().unwrap_or_default();
                let t = by_client.get(&c.id).and_then(|pid| tracked.get(pid));
                LiveClient {
                    client_id: c.id,
                    account_id: c.account_id,
                    state: c.state,
                    pid: by_client.get(&c.id).copied().or(c.pid),
                    hwnd: t.and_then(|t| t.hwnd).map(|h| h.0).or(c.hwnd),
                    cpu_pct: m.cpu_pct,
                    mem_mb: m.mem_mb,
                    threads: m.thread_count,
                    handles: m.handle_count,
                    started_at: t.map(|t| t.started_at).or(c.session_started_at),
                    state_since: c.state_since,
                    target: c.target.clone(),
                    restarts: c.restart_count,
                    last_reason: c.last_reason.clone(),
                }
            })
            .collect();
        out.sort_by_key(|c| c.started_at);
        out
    }

    pub fn snapshot(&self) -> Result<RuntimeSnapshot> {
        let vault = self.status()?;
        Ok(RuntimeSnapshot {
            fleet: self.service.snapshot(),
            live: self.live_clients(),
            totals: self.totals(),
            vault,
            roblox_clients: find_installed_clients()
                .into_iter()
                .map(|c: InstalledClient| InstalledRoblox { version: c.version, path: c.player_exe })
                .collect(),
            options: self.options(),
            taken_at: Utc::now(),
        })
    }

    pub fn apply_limits(&self, client_id: ClientId, limits: &ResourceLimits) -> Result<()> {
        let pid = self.by_client.lock().get(&client_id).copied();
        if let Some(pid) = pid {
            self.platform.apply_limits(pid, limits)?;
            if let Some(track) = self.tracked.lock().get_mut(&pid) {
                track.limits = limits.clone();
            }
        }
        Ok(())
    }

    pub fn focus_client(&self, client_id: ClientId) -> Result<()> {
        let pid = self.by_client.lock().get(&client_id).copied();
        if let Some(pid) = pid {
            if let Some(track) = self.tracked.lock().get(&pid) {
                if let Some(hwnd) = track.hwnd {
                    self.platform.focus_window(hwnd)?;
                }
            }
        }
        Ok(())
    }

    pub fn client_summary(&self, client_id: ClientId) -> Option<LiveClient> {
        self.live_clients().into_iter().find(|c| c.client_id == client_id)
    }

    pub fn apply_settings(&self, settings: &Settings) -> Result<()> {
        let limits = settings.resource_profile.limits();
        let pids: Vec<u32> = self.tracked.lock().keys().copied().collect();
        for pid in pids {
            let _ = self.platform.apply_limits(pid, &limits);
        }
        Ok(())
    }

    pub fn set_eco_qos_for_self(&self, enabled: bool) {
        #[cfg(windows)]
        {
            let pid = std::process::id();
            let _ = rofarm_win::process::set_eco_qos(pid, enabled);
        }
        #[cfg(not(windows))]
        {
            let _ = enabled;
        }
    }

    pub fn target_preview(raw: &str) -> Result<rofarm_core::launch::TargetPreview> {
        Ok(parse_target(raw)?.preview())
    }

    pub fn lane_for_account(&self, account_id: AccountId) -> Option<LaneId> {
        self.service.account(account_id).map(|a| a.lane_id)
    }

    pub fn profile(&self, id: LaunchProfileId) -> Option<LaunchProfile> {
        self.service.snapshot().profiles.get(&id).cloned()
    }

    pub fn launch_paths(&self) -> (PathBuf, PathBuf) {
        (self.vault_path.clone(), self.audit.lock().path().to_path_buf())
    }

    pub fn network_policy() -> Arc<NetPolicy> {
        Arc::new(NetPolicy::default())
    }

    pub fn set_tracked_window_for_test(&self, client_id: ClientId, hwnd: WindowId) {
        let pid = self.by_client.lock().get(&client_id).copied();
        if let Some(pid) = pid {
            if let Some(t) = self.tracked.lock().get_mut(&pid) {
                t.hwnd = Some(hwnd);
            }
        }
    }

    pub fn tracked_count(&self) -> usize {
        self.tracked.lock().len()
    }

    pub fn is_tracked(&self, pid: u32) -> bool {
        self.tracked.lock().contains_key(&pid)
    }

    pub fn pids(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.tracked.lock().keys().copied().collect();
        pids.sort_unstable();
        pids
    }

    pub fn rect_for_window(&self, area: Rect, index: usize, count: usize) -> Rect {
        grid(area, count, 8, 8).get(index).copied().unwrap_or(area)
    }

    pub fn launched_pid(&self, client_id: ClientId) -> Option<u32> {
        self.by_client.lock().get(&client_id).copied()
    }

    pub fn pid_of(&self, client_id: ClientId) -> Option<u32> {
        self.by_client.lock().get(&client_id).copied()
    }

    pub fn default_profiles(&self) -> Vec<LaunchProfile> {
        let state = self.service.snapshot();
        state.profiles.values().cloned().collect()
    }

    pub fn accounts(&self) -> Vec<Account> {
        self.service.accounts()
    }

    pub fn settings(&self) -> Settings {
        self.service.snapshot().settings
    }

    pub fn mark_disconnected(&self, actor: &Actor, client_id: ClientId, reason: rofarm_core::client::DisconnectReason) -> Result<()> {
        self.service.mark_client_state(actor, client_id, ClientState::Disconnected, Some(format!("{reason:?}")))?;
        Ok(())
    }

    pub fn mark_rejoined(&self, actor: &Actor, client_id: ClientId) -> Result<()> {
        self.service.mark_client_state(actor, client_id, ClientState::InGame, Some("Rejoined".into()))?;
        Ok(())
    }

    pub fn directory(&self) -> Option<PathBuf> {
        self.vault_path.parent().map(Path::to_path_buf)
    }

    pub fn installed_clients() -> Vec<InstalledClient> {
        find_installed_clients()
    }
}

pub mod paths;
pub use paths::AppPaths;
