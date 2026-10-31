use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ids::{AccountId, ClientId, LaneId, LaunchProfileId};
use crate::lane::ResourceLimits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClientState {
    #[default]
    Queued,
    Launching,
    InGame,
    Disconnected,
    Restarting,
    Failed,
    Stopped,
}

impl ClientState {
    pub fn label(&self) -> &'static str {
        match self {
            ClientState::Queued => "Queued",
            ClientState::Launching => "Launching",
            ClientState::InGame => "In game",
            ClientState::Disconnected => "Disconnected",
            ClientState::Restarting => "Restarting",
            ClientState::Failed => "Failed",
            ClientState::Stopped => "Stopped",
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, ClientState::Launching | ClientState::InGame | ClientState::Disconnected | ClientState::Restarting)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, ClientState::Failed | ClientState::Stopped)
    }

    pub fn can_transition_to(&self, next: ClientState) -> bool {
        use ClientState::*;
        match (self, next) {
            (a, b) if *a == b => false,
            (Queued, Launching | Stopped | Failed) => true,
            (Launching, InGame | Disconnected | Failed | Stopped | Restarting) => true,
            (InGame, Disconnected | Stopped | Failed) => true,
            (Disconnected, InGame | Restarting | Stopped | Failed) => true,
            (Restarting, Launching | Queued | Failed | Stopped) => true,
            (Failed, Queued | Stopped) => true,
            (Stopped, Queued) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum TransitionError {
    #[error("illegal client transition {from:?} -> {to:?}")]
    Illegal { from: ClientState, to: ClientState },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExitReason {
    Normal,
    Crashed { code: Option<i32> },
    Killed { source: KillSource },
    Disconnected { reason: DisconnectReason },
    Frozen,
    MemoryThrottled,
    LaunchFailed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectReason {
    Kicked,
    NetworkLost,
    RobloxUpdate,
    ServerClosed,
    Unknown,
}

impl DisconnectReason {
    pub fn parse_log_line(line: &str) -> Option<Self> {
        let l = line.to_ascii_lowercase();
        if l.contains("kicked") || l.contains("kick from") {
            Some(DisconnectReason::Kicked)
        } else if l.contains("disconnected") || l.contains("connection lost") || l.contains("idle") {
            Some(DisconnectReason::NetworkLost)
        } else if l.contains("update") && l.contains("required") {
            Some(DisconnectReason::RobloxUpdate)
        } else if l.contains("server") && l.contains("closed") {
            Some(DisconnectReason::ServerClosed)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillSource {
    Ui,
    KillAll,
    Tray,
    Hotkey,
    Panic,
    Cli,
    Api,
    Automation,
    Daemon,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientRuntime {
    pub id: ClientId,
    pub account_id: AccountId,
    pub lane_id: LaneId,
    pub profile_id: Option<LaunchProfileId>,
    pub state: ClientState,
    pub pid: Option<u32>,
    pub hwnd: Option<i64>,
    pub launched_at: Option<DateTime<Utc>>,
    pub state_since: DateTime<Utc>,
    pub session_started_at: Option<DateTime<Utc>>,
    pub playtime_seconds: u64,
    pub restart_count: u32,
    pub crash_count: u32,
    pub consecutive_failures: u32,
    pub exit_code: Option<i32>,
    pub last_reason: Option<String>,
    pub server_job_id: Option<String>,
    pub target: Option<String>,
    pub limits: ResourceLimits,
    pub marked_disconnected_at: Option<DateTime<Utc>>,
    pub frozen_since: Option<DateTime<Utc>>,
    pub memory_mb: u64,
    pub cpu_pct: f32,
}

impl ClientRuntime {
    pub fn new(account_id: AccountId, lane_id: LaneId, limits: ResourceLimits) -> Self {
        Self {
            id: ClientId::new(),
            account_id,
            lane_id,
            profile_id: None,
            state: ClientState::Queued,
            pid: None,
            hwnd: None,
            launched_at: None,
            state_since: Utc::now(),
            session_started_at: None,
            playtime_seconds: 0,
            restart_count: 0,
            crash_count: 0,
            consecutive_failures: 0,
            exit_code: None,
            last_reason: None,
            server_job_id: None,
            target: None,
            limits,
            marked_disconnected_at: None,
            frozen_since: None,
            memory_mb: 0,
            cpu_pct: 0.0,
        }
    }

    pub fn transition(&mut self, next: ClientState, at: DateTime<Utc>) -> Result<(), TransitionError> {
        if !self.state.can_transition_to(next) {
            return Err(TransitionError::Illegal { from: self.state, to: next });
        }
        if next == ClientState::InGame {
            self.launched_at.get_or_insert(at);
            self.session_started_at = Some(at);
            self.marked_disconnected_at = None;
            self.frozen_since = None;
        }
        if next == ClientState::Disconnected {
            self.marked_disconnected_at = Some(at);
        }
        if next == ClientState::Launching && self.launched_at.is_none() {
            self.launched_at = Some(at);
        }
        if next.is_terminal() {
            self.frozen_since = None;
            if let Some(started) = self.session_started_at.take() {
                self.playtime_seconds += (at - started).num_seconds().max(0) as u64;
            }
        }
        self.state = next;
        self.state_since = at;
        Ok(())
    }

    pub fn mark_stable(&mut self, now: DateTime<Utc>) {
        if let Some(since) = self.session_started_at {
            if now - since > Duration::minutes(10) {
                self.consecutive_failures = 0;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorPolicy {
    pub max_restarts: u32,
    pub crash_threshold: u32,
    pub crash_window_minutes: i64,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub allowed_to_stay_disconnected: bool,
    pub auto_rejoin: bool,
    pub restart_on_crash: bool,
    pub freeze_timeout_seconds: i64,
    pub memory_limit_mb: Option<u32>,
    pub restart_on_memory: bool,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 4,
            crash_threshold: 3,
            crash_window_minutes: 15,
            base_backoff_ms: 15_000,
            max_backoff_ms: 10 * 60_000,
            allowed_to_stay_disconnected: true,
            auto_rejoin: true,
            restart_on_crash: true,
            freeze_timeout_seconds: 180,
            memory_limit_mb: None,
            restart_on_memory: false,
        }
    }
}

impl SupervisorPolicy {
    fn backoff(&self, attempt: u32) -> u64 {
        let exp = self.base_backoff_ms.saturating_mul(1u64 << attempt.min(8).saturating_sub(1));
        exp.min(self.max_backoff_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum SupervisorDecision {
    NoAction,
    Stop,
    StayDisconnected,
    RestartAfter { delay_ms: u64 },
    MarkFailed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashBreaker {
    events: Vec<DateTime<Utc>>,
    pub threshold: u32,
    pub window_minutes: i64,
    pub tripped: bool,
}

impl CrashBreaker {
    pub fn new(threshold: u32, window_minutes: i64) -> Self {
        Self { events: Vec::new(), threshold, window_minutes, tripped: false }
    }

    pub fn record(&mut self, at: DateTime<Utc>) {
        self.events.push(at);
        self.prune(at);
        if self.events.len() as u32 >= self.threshold {
            self.tripped = true;
        }
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now - Duration::minutes(self.window_minutes);
        self.events.retain(|e| *e >= cutoff);
        if self.events.len() < self.threshold as usize {
            self.tripped = false;
        }
    }

    pub fn count(&mut self, now: DateTime<Utc>) -> usize {
        self.prune(now);
        self.events.len()
    }

    pub fn reset(&mut self) {
        self.events.clear();
        self.tripped = false;
    }
}

pub fn decide_after_exit(
    client: &mut ClientRuntime,
    reason: ExitReason,
    at: DateTime<Utc>,
    policy: &SupervisorPolicy,
    breaker: &mut CrashBreaker,
) -> SupervisorDecision {
    client.exit_code = match reason {
        ExitReason::Crashed { code } => code,
        _ => None,
    };
    client.last_reason = Some(match reason {
        ExitReason::Normal => "Client exited".into(),
        ExitReason::Crashed { code } => format!("Crashed (code {})", code.unwrap_or(-1)),
        ExitReason::Killed { source } => format!("Killed via {source:?}"),
        ExitReason::Disconnected { reason } => format!("Disconnected: {reason:?}"),
        ExitReason::Frozen => "Client frozen".into(),
        ExitReason::MemoryThrottled => "Memory limit exceeded".into(),
        ExitReason::LaunchFailed => "Launch failed".into(),
        ExitReason::Unknown => "Exited unexpectedly".into(),
    });

    match reason {
        ExitReason::Normal => {
            client.state = ClientState::Stopped;
            SupervisorDecision::Stop
        }
        ExitReason::Killed { .. } => {
            client.state = ClientState::Stopped;
            SupervisorDecision::Stop
        }
        ExitReason::Disconnected { .. } => {
            if policy.allowed_to_stay_disconnected {
                client.state = ClientState::Disconnected;
                SupervisorDecision::StayDisconnected
            } else if policy.auto_rejoin && client.restart_count < policy.max_restarts {
                client.restart_count += 1;
                let attempt = client.consecutive_failures + 1;
                client.consecutive_failures = attempt;
                client.state = ClientState::Restarting;
                SupervisorDecision::RestartAfter { delay_ms: policy.backoff(attempt) }
            } else {
                client.state = ClientState::Failed;
                SupervisorDecision::MarkFailed
            }
        }
        ExitReason::Crashed { .. } | ExitReason::Frozen | ExitReason::MemoryThrottled => {
            client.crash_count += 1;
            breaker.record(at);
            let retryable = match reason {
                ExitReason::Crashed { .. } => policy.restart_on_crash,
                ExitReason::Frozen => true,
                ExitReason::MemoryThrottled => policy.restart_on_memory,
                _ => false,
            };
            if breaker.tripped {
                client.state = ClientState::Failed;
                client.last_reason = Some(format!(
                    "Crash breaker tripped: {} crashes in {} min",
                    breaker.count(at),
                    policy.crash_window_minutes
                ));
                SupervisorDecision::MarkFailed
            } else if retryable && client.restart_count < policy.max_restarts {
                client.restart_count += 1;
                let attempt = client.consecutive_failures + 1;
                client.consecutive_failures = attempt;
                client.state = ClientState::Restarting;
                SupervisorDecision::RestartAfter { delay_ms: policy.backoff(attempt) }
            } else {
                client.state = ClientState::Failed;
                SupervisorDecision::MarkFailed
            }
        }
        ExitReason::LaunchFailed => {
            client.consecutive_failures += 1;
            if client.consecutive_failures < 3 {
                client.state = ClientState::Restarting;
                SupervisorDecision::RestartAfter { delay_ms: policy.backoff(client.consecutive_failures) }
            } else {
                client.state = ClientState::Failed;
                SupervisorDecision::MarkFailed
            }
        }
        ExitReason::Unknown => {
            client.crash_count += 1;
            breaker.record(at);
            if breaker.tripped {
                client.state = ClientState::Failed;
                SupervisorDecision::MarkFailed
            } else {
                client.state = ClientState::Disconnected;
                SupervisorDecision::StayDisconnected
            }
        }
    }
}

pub fn rotation_pick(
    pool: &[(AccountId, u64, u8)],
    order: crate::lane::RotationOrder,
    keep: u32,
    already_running: &[AccountId],
) -> Vec<AccountId> {
    let mut candidates: Vec<(AccountId, u64, u8)> = pool
        .iter()
        .filter(|(id, _, _)| !already_running.contains(id))
        .copied()
        .collect();
    match order {
        crate::lane::RotationOrder::RoundRobin => {}
        crate::lane::RotationOrder::LeastPlaytime => candidates.sort_by_key(|(_, play, _)| *play),
        crate::lane::RotationOrder::Healthiest => {
            candidates.sort_by_key(|(_, _, health)| std::cmp::Reverse(*health))
        }
    }
    let need = keep.saturating_sub(already_running.len() as u32);
    candidates.into_iter().take(need as usize).map(|(id, _, _)| id).collect()
}

pub fn lane_counts(clients: &BTreeMap<ClientId, ClientRuntime>) -> BTreeMap<LaneId, u32> {
    let mut counts = BTreeMap::new();
    for c in clients.values() {
        if c.state.is_running() {
            *counts.entry(c.lane_id).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::builtin;

    fn runtime() -> ClientRuntime {
        ClientRuntime::new(AccountId::new(), builtin::farming(), ResourceLimits::default())
    }

    #[test]
    fn valid_transitions_are_enforced() {
        assert!(ClientState::Queued.can_transition_to(ClientState::Launching));
        assert!(ClientState::Launching.can_transition_to(ClientState::InGame));
        assert!(ClientState::InGame.can_transition_to(ClientState::Disconnected));
        assert!(ClientState::Disconnected.can_transition_to(ClientState::Restarting));
        assert!(ClientState::Restarting.can_transition_to(ClientState::Launching));
        assert!(!ClientState::Failed.can_transition_to(ClientState::InGame));
        assert!(!ClientState::Queued.can_transition_to(ClientState::InGame));
        assert!(!ClientState::InGame.can_transition_to(ClientState::Queued));
    }

    #[test]
    fn transitions_track_playtime() {
        let mut c = runtime();
        let t0 = Utc::now();
        c.transition(ClientState::Launching, t0).unwrap();
        c.transition(ClientState::InGame, t0).unwrap();
        c.transition(ClientState::Stopped, t0 + Duration::seconds(125)).unwrap();
        assert_eq!(c.playtime_seconds, 125);
        assert_eq!(c.state, ClientState::Stopped);
    }

    #[test]
    fn crash_breaker_trips_and_resets_after_window() {
        let mut b = CrashBreaker::new(3, 10);
        let t = Utc::now();
        b.record(t - Duration::minutes(11));
        b.record(t);
        assert!(!b.tripped);
        b.record(t);
        b.record(t);
        assert!(b.tripped);
        let later = Utc::now() + Duration::minutes(11);
        assert_eq!(b.count(later), 0);
        assert!(!b.tripped);
        assert_eq!(b.threshold, 3);
    }

    #[test]
    fn crashed_client_restarts_with_backoff_then_fails() {
        let mut c = runtime();
        c.transition(ClientState::Launching, Utc::now()).unwrap();
        c.transition(ClientState::InGame, Utc::now()).unwrap();
        let policy = SupervisorPolicy::default();
        let mut breaker = CrashBreaker::new(3, 15);
        let d = decide_after_exit(
            &mut c,
            ExitReason::Crashed { code: Some(-1073741819) },
            Utc::now(),
            &policy,
            &mut breaker,
        );
        assert!(matches!(d, SupervisorDecision::RestartAfter { .. }));
        assert_eq!(c.state, ClientState::Restarting);
        c.transition(ClientState::Launching, Utc::now()).unwrap();
        c.transition(ClientState::InGame, Utc::now()).unwrap();
        let _ = decide_after_exit(&mut c, ExitReason::Crashed { code: None }, Utc::now(), &policy, &mut breaker);
        c.transition(ClientState::Launching, Utc::now()).unwrap();
        c.transition(ClientState::InGame, Utc::now()).unwrap();
        let d = decide_after_exit(&mut c, ExitReason::Crashed { code: None }, Utc::now(), &policy, &mut breaker);
        assert_eq!(d, SupervisorDecision::MarkFailed);
        assert_eq!(c.state, ClientState::Failed);
        assert!(c.last_reason.unwrap().contains("Crash breaker"));
    }

    #[test]
    fn disconnect_can_stay_disconnected_when_allowed() {
        let mut c = runtime();
        c.transition(ClientState::Launching, Utc::now()).unwrap();
        c.transition(ClientState::InGame, Utc::now()).unwrap();
        let policy = SupervisorPolicy { allowed_to_stay_disconnected: true, ..Default::default() };
        let mut breaker = CrashBreaker::default();
        let d = decide_after_exit(
            &mut c,
            ExitReason::Disconnected { reason: DisconnectReason::Kicked },
            Utc::now(),
            &policy,
            &mut breaker,
        );
        assert_eq!(d, SupervisorDecision::StayDisconnected);
        assert_eq!(c.state, ClientState::Disconnected);
    }

    #[test]
    fn rotation_prefers_least_playtime() {
        let a = AccountId::new();
        let b = AccountId::new();
        let c = AccountId::new();
        let pool = vec![(a, 900, 90), (b, 100, 50), (c, 500, 80)];
        let picked = rotation_pick(&pool, crate::lane::RotationOrder::LeastPlaytime, 2, &[]);
        assert_eq!(picked, vec![b, c]);
    }

    #[test]
    fn disconnect_reason_parses_common_logs() {
        assert_eq!(
            DisconnectReason::parse_log_line("You were kicked from this experience"),
            Some(DisconnectReason::Kicked)
        );
        assert_eq!(
            DisconnectReason::parse_log_line("Connection lost"),
            Some(DisconnectReason::NetworkLost)
        );
        assert_eq!(DisconnectReason::parse_log_line("all good"), None);
    }
}
