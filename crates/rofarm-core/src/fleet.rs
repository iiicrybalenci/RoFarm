use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::account::{Account, LoginState};
use crate::automation::{Automation, AutomationRun};
use crate::client::{ClientRuntime, ClientState};
use crate::events::DomainEvent;
use crate::ids::{
    AccountId, AutomationId, ClientId, GameId, LaneId, LaunchProfileId, LayoutId, PrivateServerId, SceneId,
    ServerBookmarkId,
};
use crate::lane::{Lane, LaneRules};
use crate::launch::LaunchProfile;
use crate::library::{FleetEconomy, GameEntry, PrivateServerLink, ServerBookmark};
use crate::notice::{Notice, NoticeSeverity};
use crate::scene::Scene;
use crate::settings::Settings;
use crate::window::WindowLayout;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub client_id: ClientId,
    pub account_id: AccountId,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub seconds: u64,
    pub outcome: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetSummary {
    pub accounts: u32,
    pub lanes: u32,
    pub running_clients: u32,
    pub total_clients: u32,
    pub unread_notices: u32,
    pub critical_notices: u32,
    pub needs_attention: u32,
    pub total_net_worth: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub revision: u64,
    pub accounts: BTreeMap<AccountId, Account>,
    pub lanes: BTreeMap<LaneId, Lane>,
    pub clients: BTreeMap<ClientId, ClientRuntime>,
    pub profiles: BTreeMap<LaunchProfileId, LaunchProfile>,
    pub scenes: BTreeMap<SceneId, Scene>,
    pub automations: BTreeMap<AutomationId, Automation>,
    pub automation_runs: Vec<AutomationRun>,
    pub layouts: BTreeMap<LayoutId, WindowLayout>,
    pub games: BTreeMap<GameId, GameEntry>,
    pub server_bookmarks: BTreeMap<ServerBookmarkId, ServerBookmark>,
    pub private_servers: BTreeMap<PrivateServerId, PrivateServerLink>,
    pub notices: Vec<Notice>,
    pub settings: Settings,
    pub economy: FleetEconomy,
    pub sessions: Vec<SessionRecord>,
    pub crash_times: BTreeMap<ClientId, Vec<DateTime<Utc>>>,
    pub last_event_at: Option<DateTime<Utc>>,
}

impl Default for FleetState {
    fn default() -> Self {
        let mut lanes = BTreeMap::new();
        for lane in Lane::seed() {
            lanes.insert(lane.id, lane);
        }
        Self {
            revision: 0,
            accounts: BTreeMap::new(),
            lanes,
            clients: BTreeMap::new(),
            profiles: BTreeMap::new(),
            scenes: BTreeMap::new(),
            automations: BTreeMap::new(),
            automation_runs: Vec::new(),
            layouts: BTreeMap::new(),
            games: BTreeMap::new(),
            server_bookmarks: BTreeMap::new(),
            private_servers: BTreeMap::new(),
            notices: Vec::new(),
            settings: Settings::default(),
            economy: FleetEconomy::default(),
            sessions: Vec::new(),
            crash_times: BTreeMap::new(),
            last_event_at: None,
        }
    }
}

impl FleetState {
    pub fn apply(&mut self, event: &DomainEvent, at: DateTime<Utc>) {
        self.bump(at);
        match event {
            DomainEvent::AccountAdded { account } => {
                self.accounts.insert(account.id, account.clone());
            }
            DomainEvent::AccountUpdated { account, .. } => {
                self.accounts.insert(account.id, account.clone());
            }
            DomainEvent::AccountRemoved { account_id, .. } => {
                self.accounts.remove(account_id);
            }
            DomainEvent::AccountMoved { account_id, to, .. } => {
                if let Some(a) = self.accounts.get_mut(account_id) {
                    a.lane_id = *to;
                    a.updated_at = at;
                }
            }
            DomainEvent::AccountsMerged { primary, removed } => {
                for id in removed {
                    self.accounts.remove(id);
                }
                self.accounts.insert(primary.id, primary.clone());
            }
            DomainEvent::ImportCompleted { .. } => {}
            DomainEvent::LaneAdded { lane } => {
                self.lanes.insert(lane.id, lane.clone());
            }
            DomainEvent::LaneUpdated { lane } => {
                self.lanes.insert(lane.id, lane.clone());
            }
            DomainEvent::LaneRulesChanged { lane_id, rules } => {
                if let Some(l) = self.lanes.get_mut(lane_id) {
                    l.rules = rules.clone();
                }
            }
            DomainEvent::LaneRemoved { lane_id, moved_to, moved_accounts } => {
                self.lanes.remove(lane_id);
                if let Some(to) = moved_to {
                    for id in moved_accounts {
                        if let Some(a) = self.accounts.get_mut(id) {
                            a.lane_id = *to;
                        }
                    }
                }
            }
            DomainEvent::ProfileSaved { profile } => {
                self.profiles.insert(profile.id, profile.clone());
            }
            DomainEvent::ProfileRemoved { profile_id } => {
                self.profiles.remove(profile_id);
            }
            DomainEvent::ClientQueued { client } => {
                self.clients.insert(client.id, client.clone());
            }
            DomainEvent::ClientProcessAttached { client_id, pid, hwnd } => {
                if let Some(c) = self.clients.get_mut(client_id) {
                    c.pid = Some(*pid);
                    c.hwnd = *hwnd;
                }
            }
            DomainEvent::ClientStateChanged { client_id, to, at: change_at, reason, .. } => {
                if let Some(c) = self.clients.get_mut(client_id) {
                    if *to == ClientState::InGame {
                        c.session_started_at = Some(*change_at);
                        c.marked_disconnected_at = None;
                        c.frozen_since = None;
                    }
                    if *to == ClientState::Disconnected {
                        c.marked_disconnected_at = Some(*change_at);
                    }
                    if to.is_terminal() {
                        if let Some(started) = c.session_started_at.take() {
                            let seconds = (*change_at - started).num_seconds().max(0) as u64;
                            c.playtime_seconds += seconds;
                            self.sessions.push(SessionRecord {
                                client_id: *client_id,
                                account_id: c.account_id,
                                started_at: started,
                                ended_at: *change_at,
                                seconds,
                                outcome: reason.clone().unwrap_or_else(|| format!("{:?}", to)),
                                target: c.target.clone(),
                            });
                            if self.sessions.len() > 5000 {
                                self.sessions.drain(0..1000);
                            }
                        }
                    }
                    c.state = *to;
                    c.state_since = *change_at;
                    if let Some(r) = reason {
                        c.last_reason = Some(r.clone());
                    }
                }
            }
            DomainEvent::ClientExited { client_id, reason, .. } => {
                self.crash_times.entry(*client_id).or_default().push(at);
                if let Some(c) = self.clients.get_mut(client_id) {
                    c.last_reason = Some(format!("{reason:?}"));
                    if let crate::client::ExitReason::Crashed { code } = reason {
                        c.exit_code = *code;
                        c.crash_count += 1;
                    }
                }
            }
            DomainEvent::ClientRemoved { client_id } => {
                self.clients.remove(client_id);
                self.crash_times.remove(client_id);
            }
            DomainEvent::LaunchBatchStarted { .. } | DomainEvent::LaunchBatchFinished { .. } => {}
            DomainEvent::FleetStopped { killed, .. } => {
                let _ = killed;
                for c in self.clients.values_mut() {
                    if c.state.is_running() {
                        c.state = ClientState::Stopped;
                        c.state_since = at;
                    }
                }
            }
            DomainEvent::SceneSaved { scene } => {
                self.scenes.insert(scene.id, scene.clone());
            }
            DomainEvent::SceneRemoved { scene_id } => {
                self.scenes.remove(scene_id);
            }
            DomainEvent::SceneApplied { scene_id, at: applied } => {
                if let Some(s) = self.scenes.get_mut(scene_id) {
                    s.last_applied = Some(*applied);
                    s.apply_count += 1;
                }
            }
            DomainEvent::AutomationSaved { automation } => {
                self.automations.insert(automation.id, automation.clone());
            }
            DomainEvent::AutomationRemoved { automation_id } => {
                self.automations.remove(automation_id);
            }
            DomainEvent::AutomationFired { run } => {
                if let Some(a) = self.automations.get_mut(&run.automation_id) {
                    a.last_run = Some(run.at);
                    a.run_count += 1;
                }
                self.automation_runs.push(run.clone());
                if self.automation_runs.len() > 500 {
                    self.automation_runs.drain(0..100);
                }
            }
            DomainEvent::LayoutSaved { layout } => {
                self.layouts.insert(layout.id, layout.clone());
            }
            DomainEvent::LayoutRemoved { layout_id } => {
                self.layouts.remove(layout_id);
            }
            DomainEvent::GameSaved { game } => {
                self.games.insert(game.id, game.clone());
            }
            DomainEvent::GameRemoved { game_id } => {
                self.games.remove(game_id);
            }
            DomainEvent::ServerBookmarked { bookmark } => {
                self.server_bookmarks.insert(bookmark.id, bookmark.clone());
            }
            DomainEvent::ServerBookmarkRemoved { bookmark_id } => {
                self.server_bookmarks.remove(bookmark_id);
            }
            DomainEvent::PrivateServerSaved { link } => {
                self.private_servers.insert(link.id, link.clone());
            }
            DomainEvent::PrivateServerRemoved { link_id } => {
                self.private_servers.remove(link_id);
            }
            DomainEvent::SettingsChanged { settings } => {
                self.settings = (**settings).clone();
            }
            DomainEvent::NoticeRaised { notice } => {
                if let Some(key) = &notice.dedupe_key {
                    if let Some(existing) = self
                        .notices
                        .iter_mut()
                        .find(|n| n.dedupe_key.as_ref() == Some(key) && !n.read)
                    {
                        existing.at = notice.at;
                        existing.title = notice.title.clone();
                        existing.body = notice.body.clone();
                        existing.severity = notice.severity;
                        return;
                    }
                }
                self.notices.push(notice.clone());
                let limit = self.settings.notifications.history_limit.max(50) as usize;
                if self.notices.len() > limit {
                    let excess = self.notices.len() - limit;
                    self.notices.drain(0..excess);
                }
            }
            DomainEvent::NoticeRead { notice_id } => {
                if let Some(n) = self.notices.iter_mut().find(|n| n.id == *notice_id) {
                    n.read = true;
                }
            }
            DomainEvent::NoticeDismissed { notice_id } => {
                self.notices.retain(|n| n.id != *notice_id);
            }
            DomainEvent::UndoPerformed { .. } | DomainEvent::RedoPerformed { .. } => {}
        }
    }

    fn bump(&mut self, at: DateTime<Utc>) {
        self.revision += 1;
        self.last_event_at = Some(at);
    }

    pub fn summary(&self) -> FleetSummary {
        FleetSummary {
            accounts: self.accounts.len() as u32,
            lanes: self.lanes.len() as u32,
            running_clients: self.clients.values().filter(|c| c.state.is_running()).count() as u32,
            total_clients: self.clients.len() as u32,
            unread_notices: self.notices.iter().filter(|n| !n.read).count() as u32,
            critical_notices: self
                .notices
                .iter()
                .filter(|n| !n.read && n.severity == NoticeSeverity::Critical)
                .count() as u32,
            needs_attention: self
                .accounts
                .values()
                .filter(|a| a.login.kind.needs_attention())
                .count() as u32,
            total_net_worth: self.economy.total_net_worth(),
            revision: self.revision,
        }
    }

    pub fn accounts_in_lane(&self, lane: LaneId) -> Vec<&Account> {
        let mut out: Vec<&Account> = self.accounts.values().filter(|a| a.lane_id == lane).collect();
        out.sort_by(|a, b| {
            b.favourite
                .cmp(&a.favourite)
                .then_with(|| a.display().to_ascii_lowercase().cmp(&b.display().to_ascii_lowercase()))
        });
        out
    }

    pub fn lanes_ordered(&self) -> Vec<&Lane> {
        let mut lanes: Vec<&Lane> = self.lanes.values().collect();
        lanes.sort_by_key(|l| l.order);
        lanes
    }

    pub fn clients_for_account(&self, account_id: AccountId) -> Vec<&ClientRuntime> {
        self.clients.values().filter(|c| c.account_id == account_id).collect()
    }

    pub fn active_client_for_account(&self, account_id: AccountId) -> Option<&ClientRuntime> {
        self.clients
            .values()
            .filter(|c| c.account_id == account_id && c.state.is_running())
            .max_by_key(|c| c.state_since)
    }

    pub fn lane_rules(&self, lane: LaneId) -> Option<&LaneRules> {
        self.lanes.get(&lane).map(|l| &l.rules)
    }

    pub fn playtime_on(&self, date: NaiveDate) -> u64 {
        self.sessions
            .iter()
            .filter(|s| s.started_at.date_naive() == date)
            .map(|s| s.seconds)
            .sum()
    }

    pub fn playtime_by_account(&self) -> BTreeMap<AccountId, u64> {
        let mut map = BTreeMap::new();
        for s in &self.sessions {
            *map.entry(s.account_id).or_insert(0) += s.seconds;
        }
        map
    }

    pub fn playtime_by_target(&self) -> BTreeMap<String, u64> {
        let mut map: BTreeMap<String, u64> = BTreeMap::new();
        for s in &self.sessions {
            if let Some(t) = &s.target {
                *map.entry(t.clone()).or_insert(0) += s.seconds;
            }
        }
        map
    }

    pub fn needs_login_queue(&self) -> Vec<&Account> {
        self.accounts.values().filter(|a| a.login.kind.needs_attention()).collect()
    }

    pub fn set_login_state(&mut self, account_id: AccountId, login: LoginState) {
        if let Some(a) = self.accounts.get_mut(&account_id) {
            a.login = login;
            a.recompute_health(Utc::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{HealthScore, LoginStateKind, Presence};
    use crate::events::DomainEvent;
    use crate::ids::builtin;
    use crate::lane::{Lane, ResourceLimits};

    fn account(name: &str, lane: LaneId) -> Account {
        Account::new(name, lane)
    }

    #[test]
    fn default_state_has_four_builtin_lanes() {
        let state = FleetState::default();
        assert_eq!(state.lanes.len(), 4);
        assert_eq!(state.revision, 0);
        assert_eq!(state.summary().accounts, 0);
        assert!(state.lanes_ordered()[0].name == "Farming");
    }

    #[test]
    fn account_lifecycle_projects() {
        let mut state = FleetState::default();
        let mut a = account("one", builtin::farming());
        state.apply(&DomainEvent::AccountAdded { account: a.clone() }, Utc::now());
        assert_eq!(state.accounts.len(), 1);
        a.username = "renamed".into();
        state.apply(&DomainEvent::AccountUpdated { account: a.clone(), changed: vec!["username".into()] }, Utc::now());
        assert_eq!(state.accounts[&a.id].username, "renamed");
        state.apply(
            &DomainEvent::AccountMoved { account_id: a.id, from: builtin::farming(), to: builtin::trading() },
            Utc::now(),
        );
        assert_eq!(state.accounts[&a.id].lane_id, builtin::trading());
        state.apply(
            &DomainEvent::AccountRemoved { account_id: a.id, snapshot: a.clone() },
            Utc::now(),
        );
        assert!(state.accounts.is_empty());
        assert_eq!(state.revision, 4);
    }

    #[test]
    fn client_sessions_track_playtime() {
        let mut state = FleetState::default();
        let a = account("x", builtin::farming());
        let mut c = ClientRuntime::new(a.id, a.lane_id, ResourceLimits::default());
        c.transition(ClientState::Launching, Utc::now()).unwrap();
        state.apply(&DomainEvent::ClientQueued { client: c.clone() }, Utc::now());
        let t0 = Utc::now();
        state.apply(
            &DomainEvent::ClientStateChanged {
                client_id: c.id,
                account_id: a.id,
                from: ClientState::Launching,
                to: ClientState::InGame,
                at: t0,
                reason: None,
            },
            t0,
        );
        assert_eq!(state.clients[&c.id].state, ClientState::InGame);
        let t1 = t0 + chrono::Duration::seconds(90);
        state.apply(
            &DomainEvent::ClientStateChanged {
                client_id: c.id,
                account_id: a.id,
                from: ClientState::InGame,
                to: ClientState::Stopped,
                at: t1,
                reason: Some("normal".into()),
            },
            t1,
        );
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.clients[&c.id].playtime_seconds, 90);
        assert_eq!(state.playtime_by_account()[&a.id], 90);
    }

    #[test]
    fn notices_dedupe_unread_and_respect_limit() {
        let mut state = FleetState::default();
        state.settings.notifications.history_limit = 50;
        let n = crate::notice::Notice::new(NoticeSeverity::Warning, "Crash", "client x")
            .with_dedupe("crash-x");
        state.apply(&DomainEvent::NoticeRaised { notice: n.clone() }, Utc::now());
        state.apply(&DomainEvent::NoticeRaised { notice: n.clone() }, Utc::now());
        assert_eq!(state.notices.len(), 1);
        let read = DomainEvent::NoticeRead { notice_id: n.id };
        state.apply(&read, Utc::now());
        state.apply(&DomainEvent::NoticeRaised { notice: n.clone() }, Utc::now());
        assert_eq!(state.notices.len(), 2);
        assert_eq!(state.summary().unread_notices, 1);
    }

    #[test]
    fn fleet_stop_marks_running_clients_stopped() {
        let mut state = FleetState::default();
        let a = account("x", builtin::farming());
        let mut c = ClientRuntime::new(a.id, a.lane_id, ResourceLimits::default());
        c.state = ClientState::InGame;
        state.clients.insert(c.id, c.clone());
        state.apply(&DomainEvent::FleetStopped { source: crate::client::KillSource::KillAll, killed: 1 }, Utc::now());
        assert_eq!(state.clients[&c.id].state, ClientState::Stopped);
    }

    #[test]
    fn settings_event_replaces_settings() {
        let mut state = FleetState::default();
        let mut s = Settings::default();
        s.density = crate::settings::Density::Compact;
        state.apply(&DomainEvent::SettingsChanged { settings: Box::new(s.clone()) }, Utc::now());
        assert_eq!(state.settings.density, crate::settings::Density::Compact);
    }

    #[test]
    fn summary_counts_attention_and_presence_irrelevant() {
        let mut state = FleetState::default();
        let mut a = account("x", builtin::farming());
        a.login = LoginState { kind: LoginStateKind::Expired, checked_at: Some(Utc::now()), detail: None };
        a.presence = Presence::InGame;
        a.health = HealthScore::from_score(20, vec![]);
        state.apply(&DomainEvent::AccountAdded { account: a.clone() }, Utc::now());
        assert_eq!(state.summary().needs_attention, 1);
        assert_eq!(state.accounts_in_lane(builtin::farming()).len(), 1);
    }

    #[test]
    fn lane_removal_moves_accounts() {
        let mut state = FleetState::default();
        let a = account("x", builtin::testing());
        state.apply(&DomainEvent::AccountAdded { account: a.clone() }, Utc::now());
        let custom = Lane::new("Custom", 10);
        state.apply(&DomainEvent::LaneAdded { lane: custom.clone() }, Utc::now());
        state.apply(
            &DomainEvent::LaneRemoved {
                lane_id: builtin::testing(),
                moved_to: Some(custom.id),
                moved_accounts: vec![a.id],
            },
            Utc::now(),
        );
        assert!(!state.lanes.contains_key(&builtin::testing()));
        assert_eq!(state.accounts[&a.id].lane_id, custom.id);
    }
}
