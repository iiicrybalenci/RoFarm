use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::Account;
use crate::automation::{Automation, AutomationRun};
use crate::client::{ClientRuntime, ClientState, ExitReason, KillSource};
use crate::ids::{AccountId, AutomationId, ClientId, GameId, LaneId, LaunchProfileId, LayoutId, PluginId, PrivateServerId, SceneId, ServerBookmarkId};
use crate::lane::{Lane, LaneRules};
use crate::launch::LaunchProfile;
use crate::library::{GameEntry, PrivateServerLink, ServerBookmark};
use crate::notice::Notice;
use crate::scene::Scene;
use crate::settings::Settings;
use crate::window::WindowLayout;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Actor {
    User,
    Cli,
    Daemon,
    System,
    Api { device: String },
    Automation { id: AutomationId },
    Plugin { id: PluginId },
}

impl Actor {
    pub fn label(&self) -> String {
        match self {
            Actor::User => "you".into(),
            Actor::Cli => "CLI".into(),
            Actor::Daemon => "daemon".into(),
            Actor::System => "system".into(),
            Actor::Api { device } => format!("device {device}"),
            Actor::Automation { id } => format!("automation {id}"),
            Actor::Plugin { id } => format!("plugin {id}"),
        }
    }

    pub fn is_system(&self) -> bool {
        matches!(self, Actor::System | Actor::Daemon)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DomainEvent {
    AccountAdded { account: Account },
    AccountUpdated { account: Account, changed: Vec<String> },
    AccountRemoved { account_id: AccountId, snapshot: Account },
    AccountMoved { account_id: AccountId, from: LaneId, to: LaneId },
    AccountsMerged { primary: Account, removed: Vec<AccountId> },
    ImportCompleted { source: String, imported: u32, skipped: u32, failed: u32 },

    LaneAdded { lane: Lane },
    LaneUpdated { lane: Lane },
    LaneRulesChanged { lane_id: LaneId, rules: LaneRules },
    LaneRemoved { lane_id: LaneId, moved_to: Option<LaneId>, moved_accounts: Vec<AccountId> },

    ProfileSaved { profile: LaunchProfile },
    ProfileRemoved { profile_id: LaunchProfileId },

    ClientQueued { client: ClientRuntime },
    ClientProcessAttached { client_id: ClientId, pid: u32, hwnd: Option<i64> },
    ClientStateChanged {
        client_id: ClientId,
        account_id: AccountId,
        from: ClientState,
        to: ClientState,
        at: DateTime<Utc>,
        reason: Option<String>,
    },
    ClientExited {
        client_id: ClientId,
        account_id: AccountId,
        reason: ExitReason,
        at: DateTime<Utc>,
    },
    ClientRemoved { client_id: ClientId },
    LaunchBatchStarted { batch_id: Uuid, profile_id: LaunchProfileId, account_ids: Vec<AccountId>, at: DateTime<Utc> },
    LaunchBatchFinished { batch_id: Uuid, launched: u32, failed: u32 },
    FleetStopped { source: KillSource, killed: u32 },

    SceneSaved { scene: Scene },
    SceneRemoved { scene_id: SceneId },
    SceneApplied { scene_id: SceneId, at: DateTime<Utc> },

    AutomationSaved { automation: Automation },
    AutomationRemoved { automation_id: AutomationId },
    AutomationFired { run: AutomationRun },

    LayoutSaved { layout: WindowLayout },
    LayoutRemoved { layout_id: LayoutId },

    GameSaved { game: GameEntry },
    GameRemoved { game_id: GameId },
    ServerBookmarked { bookmark: ServerBookmark },
    ServerBookmarkRemoved { bookmark_id: ServerBookmarkId },
    PrivateServerSaved { link: PrivateServerLink },
    PrivateServerRemoved { link_id: PrivateServerId },

    SettingsChanged { settings: Box<Settings> },
    NoticeRaised { notice: Notice },
    NoticeRead { notice_id: Uuid },
    NoticeDismissed { notice_id: Uuid },
    UndoPerformed { label: String },
    RedoPerformed { label: String },
}

impl DomainEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            DomainEvent::AccountAdded { .. } => "account_added",
            DomainEvent::AccountUpdated { .. } => "account_updated",
            DomainEvent::AccountRemoved { .. } => "account_removed",
            DomainEvent::AccountMoved { .. } => "account_moved",
            DomainEvent::AccountsMerged { .. } => "accounts_merged",
            DomainEvent::ImportCompleted { .. } => "import_completed",
            DomainEvent::LaneAdded { .. } => "lane_added",
            DomainEvent::LaneUpdated { .. } => "lane_updated",
            DomainEvent::LaneRulesChanged { .. } => "lane_rules_changed",
            DomainEvent::LaneRemoved { .. } => "lane_removed",
            DomainEvent::ProfileSaved { .. } => "profile_saved",
            DomainEvent::ProfileRemoved { .. } => "profile_removed",
            DomainEvent::ClientQueued { .. } => "client_queued",
            DomainEvent::ClientProcessAttached { .. } => "client_process_attached",
            DomainEvent::ClientStateChanged { .. } => "client_state_changed",
            DomainEvent::ClientExited { .. } => "client_exited",
            DomainEvent::ClientRemoved { .. } => "client_removed",
            DomainEvent::LaunchBatchStarted { .. } => "launch_batch_started",
            DomainEvent::LaunchBatchFinished { .. } => "launch_batch_finished",
            DomainEvent::FleetStopped { .. } => "fleet_stopped",
            DomainEvent::SceneSaved { .. } => "scene_saved",
            DomainEvent::SceneRemoved { .. } => "scene_removed",
            DomainEvent::SceneApplied { .. } => "scene_applied",
            DomainEvent::AutomationSaved { .. } => "automation_saved",
            DomainEvent::AutomationRemoved { .. } => "automation_removed",
            DomainEvent::AutomationFired { .. } => "automation_fired",
            DomainEvent::LayoutSaved { .. } => "layout_saved",
            DomainEvent::LayoutRemoved { .. } => "layout_removed",
            DomainEvent::GameSaved { .. } => "game_saved",
            DomainEvent::GameRemoved { .. } => "game_removed",
            DomainEvent::ServerBookmarked { .. } => "server_bookmarked",
            DomainEvent::ServerBookmarkRemoved { .. } => "server_bookmark_removed",
            DomainEvent::PrivateServerSaved { .. } => "private_server_saved",
            DomainEvent::PrivateServerRemoved { .. } => "private_server_removed",
            DomainEvent::SettingsChanged { .. } => "settings_changed",
            DomainEvent::NoticeRaised { .. } => "notice_raised",
            DomainEvent::NoticeRead { .. } => "notice_read",
            DomainEvent::NoticeDismissed { .. } => "notice_dismissed",
            DomainEvent::UndoPerformed { .. } => "undo_performed",
            DomainEvent::RedoPerformed { .. } => "redo_performed",
        }
    }

    pub fn references_account(&self, id: AccountId) -> bool {
        match self {
            DomainEvent::AccountAdded { account }
            | DomainEvent::AccountUpdated { account, .. }
            | DomainEvent::AccountRemoved { snapshot: account, .. } => account.id == id,
            DomainEvent::AccountMoved { account_id, .. } => *account_id == id,
            DomainEvent::AccountsMerged { primary, removed } => primary.id == id || removed.contains(&id),
            DomainEvent::ClientStateChanged { account_id, .. } | DomainEvent::ClientExited { account_id, .. } => {
                *account_id == id
            }
            DomainEvent::ClientQueued { client } => client.account_id == id,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEvent {
    pub at: DateTime<Utc>,
    pub actor: Actor,
    pub correlation: Option<Uuid>,
    pub event: DomainEvent,
}

impl NewEvent {
    pub fn new(actor: Actor, event: DomainEvent) -> Self {
        Self { at: Utc::now(), actor, correlation: None, event }
    }

    pub fn at(mut self, at: DateTime<Utc>) -> Self {
        self.at = at;
        self
    }

    pub fn correlated(mut self, id: Uuid) -> Self {
        self.correlation = Some(id);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredEvent {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub actor: Actor,
    pub correlation: Option<Uuid>,
    pub event: DomainEvent,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Serialize)]
struct HashPayload<'a> {
    seq: u64,
    at_micros: i64,
    actor: &'a Actor,
    correlation: Option<Uuid>,
    event: &'a DomainEvent,
    prev_hash: &'a str,
}

impl StoredEvent {
    pub fn compute_hash(
        seq: u64,
        at: DateTime<Utc>,
        actor: &Actor,
        correlation: Option<Uuid>,
        event: &DomainEvent,
        prev_hash: &str,
    ) -> String {
        let payload = HashPayload {
            seq,
            at_micros: at.timestamp_micros(),
            actor,
            correlation,
            event,
            prev_hash,
        };
        let bytes = serde_json::to_vec(&payload).expect("event hash serialization is infallible");
        blake3::hash(&bytes).to_hex().to_string()
    }

    pub fn verify(&self) -> bool {
        self.hash
            == Self::compute_hash(self.seq, self.at, &self.actor, self.correlation, &self.event, &self.prev_hash)
    }
}

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventFilter {
    pub kinds: Vec<String>,
    pub account_id: Option<AccountId>,
    pub since_seq: Option<u64>,
    pub limit: Option<usize>,
}

impl EventFilter {
    pub fn matches(&self, e: &StoredEvent) -> bool {
        if let Some(id) = self.account_id {
            if !e.event.references_account(id) {
                return false;
            }
        }
        if !self.kinds.is_empty() && !self.kinds.iter().any(|k| k == e.event.kind()) {
            return false;
        }
        if let Some(since) = self.since_seq {
            if e.seq <= since {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;
    use crate::ids::builtin;

    #[test]
    fn hash_detects_any_field_change() {
        let at = Utc::now();
        let account = Account::new("A", builtin::farming());
        let event = DomainEvent::AccountAdded { account };
        let h1 = StoredEvent::compute_hash(1, at, &Actor::User, None, &event, GENESIS_HASH);
        let stored = StoredEvent {
            seq: 1,
            at,
            actor: Actor::User,
            correlation: None,
            event: event.clone(),
            prev_hash: GENESIS_HASH.into(),
            hash: h1.clone(),
        };
        assert!(stored.verify());
        let mut tampered = stored.clone();
        if let DomainEvent::AccountAdded { account } = &mut tampered.event {
            account.username = "B".into();
        }
        assert!(!tampered.verify());
        let h2 = StoredEvent::compute_hash(1, at, &Actor::User, None, &tampered.event, GENESIS_HASH);
        assert_ne!(h1, h2);
    }

    #[test]
    fn event_kinds_are_stable() {
        let e = DomainEvent::FleetStopped { source: KillSource::Tray, killed: 3 };
        assert_eq!(e.kind(), "fleet_stopped");
        let removed = DomainEvent::AccountRemoved {
            account_id: AccountId::new(),
            snapshot: Account::new("x", builtin::idle()),
        };
        assert_eq!(removed.kind(), "account_removed");
    }

    #[test]
    fn account_filter_matches_related_events() {
        let account = Account::new("x", builtin::idle());
        let a = account.id;
        let e = StoredEvent {
            seq: 2,
            at: Utc::now(),
            actor: Actor::User,
            correlation: None,
            event: DomainEvent::AccountMoved { account_id: a, from: builtin::idle(), to: builtin::farming() },
            prev_hash: GENESIS_HASH.into(),
            hash: String::new(),
        };
        assert!(EventFilter { account_id: Some(a), ..Default::default() }.matches(&e));
        assert!(!EventFilter { kinds: vec!["account_added".into()], ..Default::default() }.matches(&e));
        assert!(EventFilter { since_seq: Some(2), ..Default::default() }.matches(&e) == false);
    }
}
