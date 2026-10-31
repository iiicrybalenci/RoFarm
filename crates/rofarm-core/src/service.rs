use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use uuid::Uuid;

use crate::account::{Account, AccountFilter, BulkEdit, LoginState};
use crate::automation::{Automation, AutomationRun};
use crate::client::{
    decide_after_exit, ClientRuntime, ClientState, CrashBreaker, ExitReason, KillSource, SupervisorDecision,
    SupervisorPolicy,
};
use crate::error::{CoreError, Result};
use crate::events::{Actor, DomainEvent, EventFilter, NewEvent, StoredEvent};
use crate::fleet::FleetState;
use crate::ids::{AccountId, AutomationId, ClientId, GameId, LaneId, LaunchProfileId, LayoutId, PrivateServerId, SceneId, ServerBookmarkId};
use crate::lane::{Lane, LaneRules};
use crate::launch::{plan_batch, LaunchProfile};
use crate::library::{GameEntry, PrivateServerLink, ServerBookmark};
use crate::notice::{Notice, NoticeSeverity};
use crate::scene::{diff_scene, Scene, SceneDiff};
use crate::settings::Settings;
use crate::store::EventStore;
use crate::undo::{UndoEntry, UndoStack};
use crate::window::WindowLayout;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewAccount {
    pub username: String,
    pub roblox_user_id: Option<u64>,
    pub display_name: Option<String>,
    pub lane_id: Option<LaneId>,
    pub alias: Option<String>,
    pub notes: String,
    pub tags: BTreeSet<String>,
    pub avatar_url: Option<String>,
    pub cookie_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountPatch {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub roblox_user_id: Option<u64>,
    pub avatar_url: Option<String>,
    pub notes: Option<String>,
    pub alias: Option<String>,
    pub tags: Option<BTreeSet<String>>,
    pub favourite: Option<bool>,
    pub color: Option<String>,
    pub presence: Option<crate::account::Presence>,
    pub robux: Option<u64>,
    pub premium: Option<bool>,
    pub friend_count: Option<u32>,
    pub follower_count: Option<u32>,
    pub two_step: Option<bool>,
    pub account_age_days: Option<u32>,
    pub last_validated: Option<DateTime<Utc>>,
    pub cookie_expires_at: Option<DateTime<Utc>>,
    pub cookie_fingerprint: Option<String>,
    pub custom_fields: Option<BTreeMap<String, String>>,
    pub login: Option<LoginState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedBatch {
    pub batch_id: Uuid,
    pub clients: Vec<ClientRuntime>,
    pub offsets_ms: Vec<u64>,
    pub total_ms: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientExitOutcome {
    pub decision: SupervisorDecision,
    pub state: ClientState,
    pub attempts: u32,
}

pub struct FleetService {
    store: Arc<dyn EventStore>,
    state: RwLock<FleetState>,
    undo: Mutex<UndoStack>,
}

impl std::fmt::Debug for FleetService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FleetService")
            .field("revision", &self.state.read().revision)
            .field("accounts", &self.state.read().accounts.len())
            .finish()
    }
}

impl FleetService {
    pub fn open(store: Arc<dyn EventStore>) -> Result<Self> {
        let events = store.load()?;
        let mut state = FleetState::default();
        for e in &events {
            state.apply(&e.event, e.at);
        }
        Ok(Self { store, state: RwLock::new(state), undo: Mutex::new(UndoStack::default()) })
    }

    pub fn store(&self) -> &Arc<dyn EventStore> {
        &self.store
    }

    pub fn snapshot(&self) -> FleetState {
        self.state.read().clone()
    }

    pub fn summary(&self) -> crate::fleet::FleetSummary {
        self.state.read().summary()
    }

    pub fn account(&self, id: AccountId) -> Option<Account> {
        self.state.read().accounts.get(&id).cloned()
    }

    pub fn accounts(&self) -> Vec<Account> {
        let state = self.state.read();
        let mut out: Vec<Account> = state.accounts.values().cloned().collect();
        out.sort_by(|a, b| {
            b.favourite
                .cmp(&a.favourite)
                .then_with(|| a.display().to_ascii_lowercase().cmp(&b.display().to_ascii_lowercase()))
        });
        out
    }

    pub fn filter_accounts(&self, filter: &AccountFilter) -> Vec<Account> {
        self.accounts().into_iter().filter(|a| filter.matches(a)).collect()
    }

    pub fn events(&self, filter: &EventFilter) -> Result<Vec<StoredEvent>> {
        let events = self.store.load()?;
        let mut out: Vec<StoredEvent> = events.into_iter().filter(|e| filter.matches(e)).collect();
        if let Some(limit) = filter.limit {
            if out.len() > limit {
                out = out.split_off(out.len() - limit);
            }
        }
        Ok(out)
    }

    pub fn verify(&self) -> Result<()> {
        self.store.verify()
    }

    fn append(&self, actor: &Actor, events: Vec<DomainEvent>) -> Result<Vec<StoredEvent>> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        let now = Utc::now();
        let batch: Vec<NewEvent> = events.into_iter().map(|event| NewEvent::new(actor.clone(), event).at(now)).collect();
        let stored = self.store.append(&batch)?;
        let mut state = self.state.write();
        for e in &stored {
            state.apply(&e.event, e.at);
        }
        Ok(stored)
    }

    fn push_undo(&self, entry: UndoEntry) {
        self.undo.lock().push(entry);
    }

    pub fn add_account(&self, actor: &Actor, input: NewAccount) -> Result<Account> {
        let username = input.username.trim().to_string();
        if username.is_empty() || username.len() > 32 {
            return Err(CoreError::Invalid("Username must be 1-32 characters.".into()));
        }
        if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(CoreError::Invalid("Username contains invalid characters.".into()));
        }
        let lane = {
            let state = self.state.read();
            if state.accounts.values().any(|a| a.username.eq_ignore_ascii_case(&username)) {
                return Err(CoreError::Conflict(format!("@{username} is already in the fleet.")));
            }
            if let Some(uid) = input.roblox_user_id {
                if state.accounts.values().any(|a| a.roblox_user_id == Some(uid)) {
                    return Err(CoreError::Conflict(format!(
                        "Roblox user {uid} is already connected to another account."
                    )));
                }
            }
            input
                .lane_id
                .filter(|l| state.lanes.contains_key(l))
                .or_else(|| state.lanes_ordered().first().map(|l| l.id))
                .ok_or_else(|| CoreError::Invalid("No lanes configured.".into()))?
        };
        let mut account = Account::new(&username, lane);
        account.roblox_user_id = input.roblox_user_id;
        account.display_name = input.display_name.unwrap_or_else(|| username.clone());
        account.alias = input.alias;
        account.notes = input.notes;
        account.tags = input.tags;
        account.avatar_url = input.avatar_url;
        account.cookie_fingerprint = input.cookie_fingerprint;
        account.recompute_health(Utc::now());
        self.append(actor, vec![DomainEvent::AccountAdded { account: account.clone() }])?;
        Ok(account)
    }

    pub fn update_account(&self, actor: &Actor, mut account: Account) -> Result<Account> {
        let (before, changed) = {
            let state = self.state.read();
            let before = state
                .accounts
                .get(&account.id)
                .ok_or_else(|| CoreError::NotFound(format!("account {}", account.id)))?
                .clone();
            if before.lane_id != account.lane_id {
                return Err(CoreError::Invalid(
                    "Use move_account to change lanes so lane rules and undo stay consistent.".into(),
                ));
            }
            let mut changed = Vec::new();
            if before.username != account.username {
                changed.push("username".to_string());
            }
            if before.display_name != account.display_name {
                changed.push("display_name".to_string());
            }
            if before.alias != account.alias {
                changed.push("alias".to_string());
            }
            if before.notes != account.notes {
                changed.push("notes".to_string());
            }
            if before.tags != account.tags {
                changed.push("tags".to_string());
            }
            if before.favourite != account.favourite {
                changed.push("favourite".to_string());
            }
            if before.color != account.color {
                changed.push("color".to_string());
            }
            if before.limits != account.limits {
                changed.push("limits".to_string());
            }
            if before.custom_fields != account.custom_fields {
                changed.push("custom_fields".to_string());
            }
            if before.login != account.login {
                changed.push("login".to_string());
            }
            (before, changed)
        };
        account.updated_at = Utc::now();
        account.recompute_health(Utc::now());
        if changed.is_empty() {
            return Ok(before);
        }
        let entry = UndoEntry {
            label: format!("Edit @{}", account.username),
            undo_events: vec![DomainEvent::AccountUpdated { account: before.clone(), changed: changed.clone() }],
            redo_events: vec![DomainEvent::AccountUpdated { account: account.clone(), changed }],
        };
        self.append(actor, entry.redo_events.clone())?;
        self.push_undo(entry);
        Ok(account)
    }

    pub fn update_account_fields(&self, actor: &Actor, id: AccountId, patch: AccountPatch) -> Result<Account> {
        let mut account = self.account(id).ok_or_else(|| CoreError::NotFound("account not found".into()))?;
        if let Some(v) = patch.username {
            account.username = v;
        }
        if let Some(v) = patch.display_name {
            account.display_name = v;
        }
        if let Some(v) = patch.roblox_user_id {
            account.roblox_user_id = Some(v);
        }
        if let Some(v) = patch.avatar_url {
            account.avatar_url = Some(v);
        }
        if let Some(v) = patch.notes {
            account.notes = v;
        }
        if let Some(v) = patch.alias {
            account.alias = Some(v);
        }
        if let Some(v) = patch.tags {
            account.tags = v;
        }
        if let Some(v) = patch.favourite {
            account.favourite = v;
        }
        if let Some(v) = patch.color {
            account.color = v;
        }
        if let Some(v) = patch.presence {
            account.presence = v;
        }
        if let Some(v) = patch.robux {
            account.robux = Some(v);
        }
        if let Some(v) = patch.premium {
            account.premium = v;
        }
        if let Some(v) = patch.friend_count {
            account.friend_count = v;
        }
        if let Some(v) = patch.follower_count {
            account.follower_count = v;
        }
        if let Some(v) = patch.two_step {
            account.two_step.enabled = v;
        }
        if let Some(v) = patch.account_age_days {
            account.account_age_days = Some(v);
        }
        if let Some(v) = patch.last_validated {
            account.last_validated = Some(v);
        }
        if let Some(v) = patch.cookie_expires_at {
            account.cookie_expires_at = Some(v);
        }
        if let Some(v) = patch.cookie_fingerprint {
            account.cookie_fingerprint = Some(v);
        }
        if let Some(v) = patch.custom_fields {
            account.custom_fields = v;
        }
        if let Some(v) = patch.login {
            account.login = v;
        }
        self.update_account(actor, account)
    }

    pub fn set_login_state(&self, actor: &Actor, id: AccountId, login: LoginState) -> Result<Account> {
        let mut account = self.account(id).ok_or_else(|| CoreError::NotFound("account not found".into()))?;
        account.login = login;
        account.last_validated = Some(Utc::now());
        account.recompute_health(Utc::now());
        self.update_account(actor, account)
    }

    pub fn remove_account(&self, actor: &Actor, id: AccountId) -> Result<Account> {
        let state = self.state.read();
        let snapshot = state
            .accounts
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound("account not found".into()))?;
        if state.clients.values().any(|c| c.account_id == id && c.state.is_running()) {
            return Err(CoreError::Conflict("Stop this account's client before deleting it.".into()));
        }
        drop(state);
        let entry = UndoEntry {
            label: format!("Delete @{}", snapshot.username),
            undo_events: vec![DomainEvent::AccountAdded { account: snapshot.clone() }],
            redo_events: vec![DomainEvent::AccountRemoved { account_id: id, snapshot: snapshot.clone() }],
        };
        self.append(actor, entry.redo_events.clone())?;
        self.push_undo(entry);
        Ok(snapshot)
    }

    pub fn move_account(&self, actor: &Actor, id: AccountId, to: LaneId) -> Result<()> {
        let (from, name) = {
            let state = self.state.read();
            let account = state.accounts.get(&id).ok_or_else(|| CoreError::NotFound("account not found".into()))?;
            if !state.lanes.contains_key(&to) {
                return Err(CoreError::NotFound("lane not found".into()));
            }
            (account.lane_id, account.username.clone())
        };
        if from == to {
            return Ok(());
        }
        let entry = UndoEntry {
            label: format!("Move @{name}"),
            undo_events: vec![DomainEvent::AccountMoved { account_id: id, from: to, to: from }],
            redo_events: vec![DomainEvent::AccountMoved { account_id: id, from, to }],
        };
        self.append(actor, entry.redo_events.clone())?;
        self.push_undo(entry);
        Ok(())
    }

    pub fn bulk_edit(&self, actor: &Actor, ids: &[AccountId], edit: &BulkEdit) -> Result<usize> {
        if ids.is_empty() {
            return Err(CoreError::Invalid("Select at least one account.".into()));
        }
        if let Some(lane) = edit.lane_id {
            if !self.state.read().lanes.contains_key(&lane) {
                return Err(CoreError::NotFound("lane not found".into()));
            }
        }
        let mut forward = Vec::new();
        let mut backward = Vec::new();
        {
            let state = self.state.read();
            for id in ids {
                let Some(account) = state.accounts.get(id) else { continue };
                if let Some(lane) = edit.lane_id {
                    if lane != account.lane_id {
                        forward.push(DomainEvent::AccountMoved { account_id: *id, from: account.lane_id, to: lane });
                        backward.push(DomainEvent::AccountMoved { account_id: *id, from: lane, to: account.lane_id });
                    }
                }
                let mut updated = account.clone();
                edit.apply(&mut updated);
                if updated.tags != account.tags
                    || updated.alias != account.alias
                    || updated.favourite != account.favourite
                    || updated.color != account.color
                {
                    forward.push(DomainEvent::AccountUpdated {
                        account: updated.clone(),
                        changed: vec!["bulk_edit".into()],
                    });
                    backward.push(DomainEvent::AccountUpdated {
                        account: account.clone(),
                        changed: vec!["bulk_edit".into()],
                    });
                }
            }
        }
        if forward.is_empty() {
            return Ok(0);
        }
        let count = forward.len();
        self.append(actor, forward.clone())?;
        self.push_undo(UndoEntry {
            label: format!("Bulk edit {count} field(s)"),
            undo_events: backward,
            redo_events: forward,
        });
        Ok(count)
    }

    pub fn merge_accounts(&self, actor: &Actor, primary_id: AccountId, duplicate_ids: &[AccountId]) -> Result<Account> {
        let state = self.state.read();
        let mut primary = state
            .accounts
            .get(&primary_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound("primary account not found".into()))?;
        let original = primary.clone();
        let mut removed = Vec::new();
        let mut snapshots = Vec::new();
        for id in duplicate_ids {
            if *id == primary_id {
                continue;
            }
            if let Some(dup) = state.accounts.get(id) {
                crate::account::merge_accounts(&mut primary, dup);
                removed.push(*id);
                snapshots.push(dup.clone());
            }
        }
        if removed.is_empty() {
            return Err(CoreError::Invalid("Select another account to merge.".into()));
        }
        drop(state);
        let forward = DomainEvent::AccountsMerged { primary: primary.clone(), removed: removed.clone() };
        let mut undo_events = vec![DomainEvent::AccountUpdated { account: original, changed: vec!["merged".into()] }];
        undo_events.extend(snapshots.iter().map(|s| DomainEvent::AccountAdded { account: s.clone() }));
        self.append(actor, vec![forward.clone()])?;
        self.push_undo(UndoEntry {
            label: format!("Merge into @{}", primary.username),
            undo_events,
            redo_events: vec![forward],
        });
        Ok(primary)
    }

    pub fn add_lane(&self, actor: &Actor, name: &str) -> Result<Lane> {
        let name = name.trim();
        if name.is_empty() || name.len() > 40 {
            return Err(CoreError::Invalid("Lane name must be 1-40 characters.".into()));
        }
        let order = self.state.read().lanes_ordered().last().map(|l| l.order + 1).unwrap_or(0);
        let lane = Lane::new(name, order);
        self.append(actor, vec![DomainEvent::LaneAdded { lane: lane.clone() }])?;
        Ok(lane)
    }

    pub fn update_lane(&self, actor: &Actor, lane: Lane) -> Result<()> {
        let state = self.state.read();
        if !state.lanes.contains_key(&lane.id) {
            return Err(CoreError::NotFound("lane not found".into()));
        }
        state
            .lanes
            .get(&lane.id)
            .map(|l| l.rules.validate())
            .unwrap_or(Ok(()))
            .map_err(|e| CoreError::Invalid(e.to_string()))?;
        drop(state);
        self.append(actor, vec![DomainEvent::LaneUpdated { lane }])?;
        Ok(())
    }

    pub fn set_lane_rules(&self, actor: &Actor, lane_id: LaneId, rules: LaneRules) -> Result<()> {
        rules.limits.validate().map_err(CoreError::Invalid)?;
        rules.anti_afk.validate().map_err(CoreError::Invalid)?;
        if let Some(schedule) = &rules.schedule {
            schedule.validate()?;
        }
        if !self.state.read().lanes.contains_key(&lane_id) {
            return Err(CoreError::NotFound("lane not found".into()));
        }
        self.append(actor, vec![DomainEvent::LaneRulesChanged { lane_id, rules }])?;
        Ok(())
    }

    pub fn remove_lane(&self, actor: &Actor, lane_id: LaneId, move_to: Option<LaneId>) -> Result<()> {
        let state = self.state.read();
        let lane = state.lanes.get(&lane_id).ok_or_else(|| CoreError::NotFound("lane not found".into()))?;
        if state.lanes.len() <= 1 {
            return Err(CoreError::Invalid("At least one lane must exist.".into()));
        }
        let target = move_to
            .filter(|l| *l != lane_id && state.lanes.contains_key(l))
            .or_else(|| state.lanes.keys().find(|l| **l != lane_id).copied())
            .ok_or_else(|| CoreError::Invalid("No lane to move accounts into.".into()))?;
        let moved: Vec<AccountId> = state
            .accounts
            .values()
            .filter(|a| a.lane_id == lane_id)
            .map(|a| a.id)
            .collect();
        let snapshot = lane.clone();
        drop(state);
        let forward = DomainEvent::LaneRemoved { lane_id, moved_to: Some(target), moved_accounts: moved.clone() };
        let mut undo = vec![DomainEvent::LaneAdded { lane: snapshot }];
        undo.extend(moved.iter().map(|id| DomainEvent::AccountMoved { account_id: *id, from: target, to: lane_id }));
        self.append(actor, vec![forward])?;
        self.push_undo(UndoEntry {
            label: "Delete lane".into(),
            undo_events: undo,
            redo_events: vec![DomainEvent::LaneRemoved { lane_id, moved_to: Some(target), moved_accounts: moved }],
        });
        Ok(())
    }

    pub fn save_profile(&self, actor: &Actor, profile: LaunchProfile) -> Result<LaunchProfile> {
        profile.validate()?;
        self.append(actor, vec![DomainEvent::ProfileSaved { profile: profile.clone() }])?;
        Ok(profile)
    }

    pub fn remove_profile(&self, actor: &Actor, id: LaunchProfileId) -> Result<()> {
        self.append(actor, vec![DomainEvent::ProfileRemoved { profile_id: id }])?;
        Ok(())
    }

    pub fn save_scene(&self, actor: &Actor, scene: Scene) -> Result<Scene> {
        scene.validate().map_err(CoreError::Invalid)?;
        self.append(actor, vec![DomainEvent::SceneSaved { scene: scene.clone() }])?;
        Ok(scene)
    }

    pub fn remove_scene(&self, actor: &Actor, id: SceneId) -> Result<()> {
        self.append(actor, vec![DomainEvent::SceneRemoved { scene_id: id }])?;
        Ok(())
    }

    pub fn scene_diff(&self, id: SceneId) -> Result<SceneDiff> {
        let state = self.state.read();
        let scene = state.scenes.get(&id).ok_or_else(|| CoreError::NotFound("scene not found".into()))?;
        let accounts: Vec<Account> = state.accounts.values().cloned().collect();
        Ok(diff_scene(scene, &accounts))
    }

    pub fn apply_scene(&self, actor: &Actor, id: SceneId) -> Result<u32> {
        let (scene, existing) = {
            let state = self.state.read();
            let scene = state.scenes.get(&id).ok_or_else(|| CoreError::NotFound("scene not found".into()))?.clone();
            (scene, state.accounts.keys().copied().collect::<BTreeSet<_>>())
        };
        let mut events = Vec::new();
        let mut applied = 0;
        {
            let state = self.state.read();
            for sa in &scene.snapshot.accounts {
                if !existing.contains(&sa.account_id) {
                    continue;
                }
                if let Some(account) = state.accounts.get(&sa.account_id) {
                    if account.lane_id != sa.lane_id {
                        events.push(DomainEvent::AccountMoved {
                            account_id: sa.account_id,
                            from: account.lane_id,
                            to: sa.lane_id,
                        });
                    }
                    applied += 1;
                }
            }
        }
        if applied == 0 {
            return Err(CoreError::Invalid("Scene references no accounts that exist.".into()));
        }
        events.push(DomainEvent::SceneApplied { scene_id: id, at: Utc::now() });
        self.append(actor, events)?;
        let _ = scene;
        Ok(applied)
    }

    pub fn save_layout(&self, actor: &Actor, layout: WindowLayout) -> Result<WindowLayout> {
        self.append(actor, vec![DomainEvent::LayoutSaved { layout: layout.clone() }])?;
        Ok(layout)
    }

    pub fn remove_layout(&self, actor: &Actor, id: LayoutId) -> Result<()> {
        self.append(actor, vec![DomainEvent::LayoutRemoved { layout_id: id }])?;
        Ok(())
    }

    pub fn save_automation(&self, actor: &Actor, automation: Automation) -> Result<Automation> {
        automation.validate().map_err(CoreError::Invalid)?;
        self.append(actor, vec![DomainEvent::AutomationSaved { automation: automation.clone() }])?;
        Ok(automation)
    }

    pub fn remove_automation(&self, actor: &Actor, id: AutomationId) -> Result<()> {
        self.append(actor, vec![DomainEvent::AutomationRemoved { automation_id: id }])?;
        Ok(())
    }

    pub fn record_automation_run(&self, actor: &Actor, mut run: AutomationRun) -> Result<()> {
        run.finish();
        self.append(actor, vec![DomainEvent::AutomationFired { run }])?;
        Ok(())
    }

    pub fn save_game(&self, actor: &Actor, game: GameEntry) -> Result<GameEntry> {
        if game.name.trim().is_empty() {
            return Err(CoreError::Invalid("Game needs a name.".into()));
        }
        self.append(actor, vec![DomainEvent::GameSaved { game: game.clone() }])?;
        Ok(game)
    }

    pub fn remove_game(&self, actor: &Actor, id: GameId) -> Result<()> {
        self.append(actor, vec![DomainEvent::GameRemoved { game_id: id }])?;
        Ok(())
    }

    pub fn bookmark_server(&self, actor: &Actor, bookmark: ServerBookmark) -> Result<ServerBookmark> {
        self.append(actor, vec![DomainEvent::ServerBookmarked { bookmark: bookmark.clone() }])?;
        Ok(bookmark)
    }

    pub fn remove_bookmark(&self, actor: &Actor, id: ServerBookmarkId) -> Result<()> {
        self.append(actor, vec![DomainEvent::ServerBookmarkRemoved { bookmark_id: id }])?;
        Ok(())
    }

    pub fn save_private_server(&self, actor: &Actor, link: PrivateServerLink) -> Result<PrivateServerLink> {
        if link.url.trim().is_empty() || link.code.trim().is_empty() {
            return Err(CoreError::Invalid("Private server needs a URL and code.".into()));
        }
        self.append(actor, vec![DomainEvent::PrivateServerSaved { link: link.clone() }])?;
        Ok(link)
    }

    pub fn remove_private_server(&self, actor: &Actor, id: PrivateServerId) -> Result<()> {
        self.append(actor, vec![DomainEvent::PrivateServerRemoved { link_id: id }])?;
        Ok(())
    }

    pub fn save_settings(&self, actor: &Actor, settings: Settings) -> Result<Settings> {
        settings.validate()?;
        self.append(actor, vec![DomainEvent::SettingsChanged { settings: Box::new(settings.clone()) }])?;
        Ok(settings)
    }

    pub fn raise_notice(&self, actor: &Actor, notice: Notice) -> Result<()> {
        self.append(actor, vec![DomainEvent::NoticeRaised { notice }])?;
        Ok(())
    }

    pub fn read_notice(&self, actor: &Actor, id: Uuid) -> Result<()> {
        self.append(actor, vec![DomainEvent::NoticeRead { notice_id: id }])?;
        Ok(())
    }

    pub fn dismiss_notice(&self, actor: &Actor, id: Uuid) -> Result<()> {
        self.append(actor, vec![DomainEvent::NoticeDismissed { notice_id: id }])?;
        Ok(())
    }

    pub fn notify(&self, actor: &Actor, severity: NoticeSeverity, title: &str, body: &str) -> Result<()> {
        let notice = Notice::new(severity, title, body);
        self.raise_notice(actor, notice)
    }

    pub fn queue_clients(
        &self,
        actor: &Actor,
        account_ids: &[AccountId],
        profile: &LaunchProfile,
        same_server_job: Option<String>,
    ) -> Result<QueuedBatch> {
        if account_ids.is_empty() {
            return Err(CoreError::Invalid("Select at least one account.".into()));
        }
        profile.validate()?;
        let (accounts, lane_limits) = {
            let state = self.state.read();
            let mut accounts = Vec::new();
            for id in account_ids {
                let account = state
                    .accounts
                    .get(id)
                    .ok_or_else(|| CoreError::NotFound(format!("account {id}")))?;
                if state
                    .clients
                    .values()
                    .any(|c| c.account_id == *id && c.state.is_running())
                {
                    return Err(CoreError::Conflict(format!("@{} already has a running client.", account.username)));
                }
                accounts.push(account.clone());
            }
            let limits: BTreeMap<LaneId, u32> = state
                .lanes
                .iter()
                .filter_map(|(id, lane)| lane.rules.max_running.map(|m| (*id, m)))
                .collect();
            (accounts, limits)
        };
        let plan = plan_batch(&accounts.iter().map(|a| a.id).collect::<Vec<_>>(), profile, |id| {
            accounts.iter().find(|a| a.id == id).map(|a| a.lane_id)
        }, &lane_limits);
        let batch_id = Uuid::new_v4();
        let mut clients = Vec::new();
        for account in &accounts {
            let mut runtime = ClientRuntime::new(account.id, account.lane_id, profile.limits.clone());
            runtime.profile_id = Some(profile.id);
            runtime.target = Some(profile.target.clone());
            runtime.server_job_id = if plan.same_server {
                same_server_job.clone()
            } else {
                None
            };
            clients.push(runtime);
        }
        let mut events = vec![DomainEvent::LaunchBatchStarted {
            batch_id,
            profile_id: profile.id,
            account_ids: accounts.iter().map(|a| a.id).collect(),
            at: Utc::now(),
        }];
        events.extend(clients.iter().map(|c| DomainEvent::ClientQueued { client: c.clone() }));
        self.append(actor, events)?;
        let offsets_ms = plan.slots.iter().map(|s| s.offset_ms).collect();
        Ok(QueuedBatch {
            batch_id,
            clients,
            offsets_ms,
            total_ms: plan.total_ms,
            warnings: plan.warnings,
        })
    }

    pub fn attach_client_process(&self, actor: &Actor, client_id: ClientId, pid: u32, hwnd: Option<i64>) -> Result<()> {
        self.append(actor, vec![DomainEvent::ClientProcessAttached { client_id, pid, hwnd }])?;
        Ok(())
    }

    pub fn mark_client_state(
        &self,
        actor: &Actor,
        client_id: ClientId,
        to: ClientState,
        reason: Option<String>,
    ) -> Result<ClientState> {
        let (account_id, from) = {
            let state = self.state.read();
            let client = state
                .clients
                .get(&client_id)
                .ok_or_else(|| CoreError::NotFound("client not found".into()))?;
            (client.account_id, client.state)
        };
        let mut probe = {
            let state = self.state.read();
            state.clients.get(&client_id).cloned().unwrap()
        };
        probe.transition(to, Utc::now()).map_err(CoreError::Transition)?;
        self.append(
            actor,
            vec![DomainEvent::ClientStateChanged {
                client_id,
                account_id,
                from,
                to,
                at: Utc::now(),
                reason,
            }],
        )?;
        Ok(to)
    }

    pub fn client_exited(&self, actor: &Actor, client_id: ClientId, reason: ExitReason) -> Result<ClientExitOutcome> {
        let (mut client, lane_rules, profile_retry) = {
            let state = self.state.read();
            let client = state
                .clients
                .get(&client_id)
                .cloned()
                .ok_or_else(|| CoreError::NotFound("client not found".into()))?;
            let lane_rules = state.lanes.get(&client.lane_id).map(|l| l.rules.clone()).unwrap_or_default();
            let retry = client
                .profile_id
                .and_then(|p| state.profiles.get(&p))
                .map(|p| p.retry.clone())
                .unwrap_or_default();
            (client, lane_rules, retry)
        };
        let now = Utc::now();
        let mut policy = SupervisorPolicy {
            max_restarts: profile_retry.max_attempts as u32 + 1,
            base_backoff_ms: profile_retry.base_backoff_secs as u64 * 1000,
            max_backoff_ms: profile_retry.max_backoff_secs as u64 * 1000,
            allowed_to_stay_disconnected: lane_rules.allowed_to_stay_disconnected,
            auto_rejoin: lane_rules.auto_rejoin,
            restart_on_crash: lane_rules.restart_on_crash,
            memory_limit_mb: client.limits.memory_mb,
            restart_on_memory: client.limits.auto_throttle_memory,
            ..Default::default()
        };
        if !profile_retry.retry_on_crash {
            policy.restart_on_crash = false;
        }
        let mut breaker = CrashBreaker::new(policy.crash_threshold, policy.crash_window_minutes);
        if let Some(times) = self.state.read().crash_times.get(&client_id) {
            for t in times {
                breaker.record(*t);
            }
        }
        let before = client.state;
        let decision = decide_after_exit(&mut client, reason, now, &policy, &mut breaker);
        let after = client.state;
        let mut events = vec![DomainEvent::ClientExited {
            client_id,
            account_id: client.account_id,
            reason,
            at: now,
        }];
        if after != before {
            events.push(DomainEvent::ClientStateChanged {
                client_id,
                account_id: client.account_id,
                from: before,
                to: after,
                at: now,
                reason: client.last_reason.clone(),
            });
        }
        self.append(actor, events)?;
        if after == ClientState::Failed {
            let _ = self.raise_notice(
                &Actor::System,
                Notice::new(
                    NoticeSeverity::Critical,
                    "Client needs attention",
                    client.last_reason.clone().unwrap_or_else(|| "Client failed".into()),
                )
                .with_dedupe(format!("client-failed-{client_id}"))
                .with_action("open_fleet", "Open Fleet"),
            );
        }
        Ok(ClientExitOutcome { decision, state: after, attempts: client.consecutive_failures })
    }

    pub fn kill_client(&self, actor: &Actor, client_id: ClientId, source: KillSource) -> Result<bool> {
        let (account_id, state) = {
            let state = self.state.read();
            let client = state
                .clients
                .get(&client_id)
                .ok_or_else(|| CoreError::NotFound("client not found".into()))?;
            (client.account_id, client.state)
        };
        if !state.is_running() {
            return Ok(false);
        }
        let now = Utc::now();
        self.append(
            actor,
            vec![
                DomainEvent::ClientExited {
                    client_id,
                    account_id,
                    reason: ExitReason::Killed { source },
                    at: now,
                },
                DomainEvent::ClientStateChanged {
                    client_id,
                    account_id,
                    from: state,
                    to: ClientState::Stopped,
                    at: now,
                    reason: Some("Killed".into()),
                },
            ],
        )?;
        Ok(true)
    }

    pub fn kill_all(&self, actor: &Actor, source: KillSource) -> Result<u32> {
        let (targets, now) = {
            let state = self.state.read();
            let targets: Vec<(ClientId, AccountId, ClientState)> = state
                .clients
                .values()
                .filter(|c| c.state.is_running())
                .map(|c| (c.id, c.account_id, c.state))
                .collect();
            (targets, Utc::now())
        };
        if targets.is_empty() {
            return Ok(0);
        }
        let count = targets.len() as u32;
        let mut events = Vec::new();
        for (client_id, account_id, from) in targets {
            events.push(DomainEvent::ClientExited {
                client_id,
                account_id,
                reason: ExitReason::Killed { source },
                at: now,
            });
            events.push(DomainEvent::ClientStateChanged {
                client_id,
                account_id,
                from,
                to: ClientState::Stopped,
                at: now,
                reason: Some("Kill all".into()),
            });
        }
        events.push(DomainEvent::FleetStopped { source, killed: count });
        self.append(actor, events)?;
        Ok(count)
    }

    pub fn remove_client(&self, actor: &Actor, client_id: ClientId) -> Result<()> {
        self.append(actor, vec![DomainEvent::ClientRemoved { client_id }])?;
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        self.undo.lock().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.lock().can_redo()
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo.lock().undo_label().map(str::to_string)
    }

    pub fn redo_label(&self) -> Option<String> {
        self.undo.lock().redo_label().map(str::to_string)
    }

    pub fn undo(&self, actor: &Actor) -> Result<Option<String>> {
        let entry = self.undo.lock().undo();
        let Some(entry) = entry else { return Ok(None) };
        self.append(actor, entry.undo_events.clone())?;
        self.append(actor, vec![DomainEvent::UndoPerformed { label: entry.label.clone() }])?;
        Ok(Some(entry.label))
    }

    pub fn redo(&self, actor: &Actor) -> Result<Option<String>> {
        let entry = self.undo.lock().redo();
        let Some(entry) = entry else { return Ok(None) };
        self.append(actor, entry.redo_events.clone())?;
        self.append(actor, vec![DomainEvent::RedoPerformed { label: entry.label.clone() }])?;
        Ok(Some(entry.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountFilter, LoginStateKind};
    use crate::client::KillSource;
    use crate::events::DomainEvent;
    use crate::ids::builtin;
    use crate::lane::LaneRules;
    use crate::launch::LaunchProfile;
    use crate::settings::Settings;
    use crate::store::MemoryEventStore;

    fn service() -> FleetService {
        FleetService::open(Arc::new(MemoryEventStore::new())).unwrap()
    }

    fn add(service: &FleetService, name: &str) -> Account {
        service
            .add_account(
                &Actor::User,
                NewAccount { username: name.into(), lane_id: Some(builtin::farming()), ..Default::default() },
            )
            .unwrap()
    }

    #[test]
    fn add_account_rejects_duplicates_and_bad_names() {
        let service = service();
        add(&service, "Alpha");
        assert!(matches!(
            service.add_account(&Actor::User, NewAccount { username: "alpha".into(), ..Default::default() }),
            Err(CoreError::Conflict(_))
        ));
        assert!(service
            .add_account(&Actor::User, NewAccount { username: "bad name".into(), ..Default::default() })
            .is_err());
        assert_eq!(service.accounts().len(), 1);
    }

    #[test]
    fn move_then_undo_restores_lane() {
        let service = service();
        let a = add(&service, "Alpha");
        service.move_account(&Actor::User, a.id, builtin::trading()).unwrap();
        assert_eq!(service.account(a.id).unwrap().lane_id, builtin::trading());
        service.undo(&Actor::User).unwrap();
        assert_eq!(service.account(a.id).unwrap().lane_id, builtin::farming());
        service.redo(&Actor::User).unwrap();
        assert_eq!(service.account(a.id).unwrap().lane_id, builtin::trading());
        assert_eq!(service.snapshot().revision, 6);
    }

    #[test]
    fn delete_account_can_be_undone_and_redone() {
        let service = service();
        let a = add(&service, "Alpha");
        service.remove_account(&Actor::User, a.id).unwrap();
        assert!(service.account(a.id).is_none());
        assert!(service.undo(&Actor::User).unwrap().is_some());
        assert!(service.account(a.id).is_some());
        assert!(service.redo(&Actor::User).unwrap().is_some());
        assert!(service.account(a.id).is_none());
    }

    #[test]
    fn cannot_delete_running_account() {
        let service = service();
        let a = add(&service, "Alpha");
        let profile = LaunchProfile::new("p", "123");
        service.save_profile(&Actor::User, profile.clone()).unwrap();
        let batch = service.queue_clients(&Actor::User, &[a.id], &profile, None).unwrap();
        service
            .mark_client_state(&Actor::User, batch.clients[0].id, ClientState::Launching, None)
            .unwrap();
        assert!(service.remove_account(&Actor::User, a.id).is_err());
        service.kill_all(&Actor::User, KillSource::KillAll).unwrap();
        assert!(service.remove_account(&Actor::User, a.id).is_ok());
    }

    #[test]
    fn queue_clients_creates_offsets_and_events() {
        let service = service();
        let a = add(&service, "Alpha");
        let b = add(&service, "Beta");
        let mut profile = LaunchProfile::new("Farm", "4483381587");
        profile.stagger_ms = 1500;
        service.save_profile(&Actor::User, profile.clone()).unwrap();
        let batch = service.queue_clients(&Actor::User, &[a.id, b.id], &profile, None).unwrap();
        assert_eq!(batch.clients.len(), 2);
        assert_eq!(batch.offsets_ms, vec![0, 1500]);
        assert_eq!(batch.total_ms, 3000);
        let state = service.snapshot();
        assert_eq!(state.clients.len(), 2);
        assert!(state.clients.values().all(|c| c.state == ClientState::Queued));
        let kinds: Vec<&str> = service
            .events(&EventFilter::default())
            .unwrap()
            .iter()
            .map(|e| e.event.kind())
            .collect();
        assert!(kinds.contains(&"launch_batch_started"));
        assert!(kinds.contains(&"client_queued"));
    }

    #[test]
    fn state_machine_rejects_illegal_jump() {
        let service = service();
        let a = add(&service, "Alpha");
        let profile = LaunchProfile::new("p", "123");
        let batch = service.queue_clients(&Actor::User, &[a.id], &profile, None).unwrap();
        assert!(service
            .mark_client_state(&Actor::User, batch.clients[0].id, ClientState::InGame, None)
            .is_err());
        service
            .mark_client_state(&Actor::User, batch.clients[0].id, ClientState::Launching, None)
            .unwrap();
        service
            .mark_client_state(&Actor::User, batch.clients[0].id, ClientState::InGame, None)
            .unwrap();
        let s = service.snapshot();
        assert_eq!(s.clients[&batch.clients[0].id].state, ClientState::InGame);
    }

    #[test]
    fn crash_breaker_fails_client_after_repeated_crashes() {
        let service = service();
        let a = add(&service, "Alpha");
        let profile = LaunchProfile::new("p", "123");
        service.save_profile(&Actor::User, profile.clone()).unwrap();
        let batch = service.queue_clients(&Actor::User, &[a.id], &profile, None).unwrap();
        let id = batch.clients[0].id;
        for _ in 0..4 {
            let state = service.snapshot().clients.get(&id).map(|c| c.state).unwrap_or(ClientState::Stopped);
            if !state.is_running() {
                if state.is_terminal() {
                    service.mark_client_state(&Actor::System, id, ClientState::Queued, None).unwrap();
                }
                service.mark_client_state(&Actor::System, id, ClientState::Launching, None).unwrap();
                service.mark_client_state(&Actor::System, id, ClientState::InGame, None).unwrap();
            }
            let outcome = service
                .client_exited(&Actor::System, id, ExitReason::Crashed { code: Some(1) })
                .unwrap();
            if outcome.state == ClientState::Failed {
                break;
            }
        }
        assert_eq!(service.snapshot().clients[&id].state, ClientState::Failed);
        assert!(service.snapshot().notices.iter().any(|n| n.severity == NoticeSeverity::Critical));
    }

    #[test]
    fn kill_all_stops_everything() {
        let service = service();
        let a = add(&service, "Alpha");
        let b = add(&service, "Beta");
        let profile = LaunchProfile::new("p", "123");
        let batch = service.queue_clients(&Actor::User, &[a.id, b.id], &profile, None).unwrap();
        for c in &batch.clients {
            service.mark_client_state(&Actor::User, c.id, ClientState::Launching, None).unwrap();
            service.mark_client_state(&Actor::User, c.id, ClientState::InGame, None).unwrap();
        }
        assert_eq!(service.kill_all(&Actor::User, KillSource::KillAll).unwrap(), 2);
        assert!(service.snapshot().clients.values().all(|c| c.state == ClientState::Stopped));
        assert_eq!(service.kill_all(&Actor::User, KillSource::KillAll).unwrap(), 0);
    }

    #[test]
    fn lane_rules_and_scene_apply() {
        let service = service();
        let a = add(&service, "Alpha");
        let mut rules = LaneRules::default();
        rules.max_running = Some(2);
        service.set_lane_rules(&Actor::User, builtin::farming(), rules).unwrap();
        assert_eq!(service.snapshot().lanes[&builtin::farming()].rules.max_running, Some(2));
        let mut scene = Scene::new("Night");
        scene.snapshot.accounts.push(crate::scene::SceneAccount {
            account_id: a.id,
            target: Some("123".into()),
            profile_id: None,
            lane_id: builtin::trading(),
            limits: None,
            auto_launch: true,
        });
        let scene = service.save_scene(&Actor::User, scene).unwrap();
        assert!(!service.scene_diff(scene.id).unwrap().is_empty());
        assert_eq!(service.apply_scene(&Actor::User, scene.id).unwrap(), 1);
        assert_eq!(service.account(a.id).unwrap().lane_id, builtin::trading());
    }

    #[test]
    fn bulk_edit_and_merge() {
        let service = service();
        let a = add(&service, "Alpha");
        let b = add(&service, "Beta");
        let mut edit = BulkEdit::default();
        edit.add_tags.insert("farm".into());
        edit.favourite = Some(true);
        let n = service.bulk_edit(&Actor::User, &[a.id, b.id], &edit).unwrap();
        assert_eq!(n, 2);
        assert!(service.account(a.id).unwrap().favourite);

        service
            .add_account(
                &Actor::User,
                NewAccount {
                    username: "BetaAlt".into(),
                    roblox_user_id: Some(99),
                    ..Default::default()
                },
            )
            .unwrap();
        let merged = service.merge_accounts(&Actor::User, a.id, &[b.id]).unwrap();
        assert!(merged.tags.contains("farm"));
        assert!(service.account(b.id).is_none());
        service.undo(&Actor::User).unwrap();
        assert!(service.account(b.id).is_some());
    }

    #[test]
    fn settings_are_validated_before_persisting() {
        let service = service();
        let mut settings = Settings::default();
        settings.ui_scale = 10;
        assert!(service.save_settings(&Actor::User, settings).is_err());
        let settings = Settings::default();
        service.save_settings(&Actor::User, settings).unwrap();
        assert_eq!(service.snapshot().settings.schema_version, 2);
    }

    #[test]
    fn login_state_updates_health() {
        let service = service();
        let a = add(&service, "Alpha");
        service
            .set_login_state(
                &Actor::User,
                a.id,
                LoginState { kind: LoginStateKind::Expired, checked_at: Some(Utc::now()), detail: None },
            )
            .unwrap();
        let mut account = service.account(a.id).unwrap();
        assert!(account.recompute_health(Utc::now()).issues.iter().any(|i| i.code == "session_expired"));
        let filtered = service.filter_accounts(&AccountFilter {
            login: Some(LoginStateKind::Expired),
            ..Default::default()
        });
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn events_are_chain_verified_by_service() {
        let service = service();
        let a = add(&service, "Alpha");
        service.move_account(&Actor::User, a.id, builtin::idle()).unwrap();
        service.verify().unwrap();
        let events = service.events(&EventFilter::default()).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].prev_hash, events[0].hash);
    }

    #[test]
    fn lane_remove_moves_accounts_and_undo_restores() {
        let service = service();
        let a = add(&service, "Alpha");
        let lane = service.add_lane(&Actor::User, "Custom").unwrap();
        service.move_account(&Actor::User, a.id, lane.id).unwrap();
        service.remove_lane(&Actor::User, lane.id, Some(builtin::idle())).unwrap();
        assert_eq!(service.account(a.id).unwrap().lane_id, builtin::idle());
        assert!(!service.snapshot().lanes.contains_key(&lane.id));
        service.undo(&Actor::User).unwrap();
        assert!(service.snapshot().lanes.contains_key(&lane.id));
        assert_eq!(service.account(a.id).unwrap().lane_id, lane.id);
    }

    #[test]
    fn decisions_are_serializable_for_ipc() {
        let service = service();
        let a = add(&service, "Alpha");
        let profile = LaunchProfile::new("p", "123");
        let batch = service.queue_clients(&Actor::User, &[a.id], &profile, None).unwrap();
        let json = serde_json::to_string(&batch).unwrap();
        assert!(json.contains("client"));
        let summary = service.summary();
        assert_eq!(summary.accounts, 1);
        assert!(serde_json::to_string(&summary).is_ok());
        let _ = DomainEvent::FleetStopped { source: KillSource::Ui, killed: 0 };
    }
}
