use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::ids::{AccountId, AutomationId, LaneId, LaunchProfileId, SceneId};
use crate::schedule::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    #[default]
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Equals,
    NotEquals,
}

impl CompareOp {
    pub fn eval_f64(&self, lhs: f64, rhs: f64) -> bool {
        match self {
            CompareOp::GreaterThan => lhs > rhs,
            CompareOp::GreaterOrEqual => lhs >= rhs,
            CompareOp::LessThan => lhs < rhs,
            CompareOp::LessOrEqual => lhs <= rhs,
            CompareOp::Equals => (lhs - rhs).abs() < f64::EPSILON,
            CompareOp::NotEquals => (lhs - rhs).abs() >= f64::EPSILON,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CompareOp::GreaterThan => ">",
            CompareOp::GreaterOrEqual => ">=",
            CompareOp::LessThan => "<",
            CompareOp::LessOrEqual => "<=",
            CompareOp::Equals => "==",
            CompareOp::NotEquals => "!=",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Trigger {
    ClientCrashed { account: Option<AccountId> },
    ClientDisconnected { account: Option<AccountId> },
    ClientFrozen { account: Option<AccountId> },
    MemoryAbove { account: Option<AccountId>, megabytes: u32 },
    CpuAbove { account: Option<AccountId>, percent: u8 },
    TimeOfDay { cron: String },
    AccountValidated { account: Option<AccountId> },
    AccountValidationFailed { account: Option<AccountId> },
    FriendOnline { username: String },
    RobuxChanged { account: Option<AccountId>, delta_at_least: Option<i64> },
    BatteryLow { percent: u8 },
    NetworkChanged,
    SessionLocked,
    SessionUnlocked,
    ForegroundAppStarted { executable: String },
    ForegroundGameStarted { universe_id: u64 },
    RobloxVersionChanged,
    AutomationFinished { automation: Option<AutomationId> },
    Manual,
    Webhook { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MetricField {
    CpuPct,
    MemoryMb,
    GpuPct,
    VramMb,
    FpsEstimate,
    PlayerCount,
    Robux,
    Friends,
    HealthScore,
    PlaytimeMinutes,
    DisconnectsLastHour,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Condition {
    Metric { account: Option<AccountId>, field: MetricField, op: CompareOp, value: f64 },
    AccountHealthBelow { account: AccountId, score: u8 },
    AccountInLane { account: AccountId, lane: LaneId },
    ClientStateIs { account: AccountId, state: crate::client::ClientState },
    TimeWindow { start_minute: u32, end_minute: u32, days: BTreeSet<u8> },
    BatteryBelow { percent: u8 },
    OnBattery,
    DayOfWeek { days: BTreeSet<u8> },
    Script { rhai: String },
    Not { condition: Box<Condition> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Action {
    Launch { account: Option<AccountId>, target: Option<String>, profile: Option<LaunchProfileId> },
    Restart { account: Option<AccountId> },
    Rejoin { account: Option<AccountId> },
    MoveLane { account: AccountId, lane: LaneId },
    ApplyLimits { account: Option<AccountId>, limits: crate::lane::ResourceLimits },
    Notify { channel: NotifyChannel, message: String, severity: u8 },
    RunScript { script: String, args: Vec<String> },
    Kill { account: Option<AccountId> },
    KillAll,
    LockVault,
    SwitchProfile { profile: crate::lane::ResourceProfile },
    OpenUrl { url: String },
    PlaySound { name: String },
    StartScene { scene: SceneId },
    StopAutomation { automation: AutomationId },
    CreateBackup,
    SetEcoMode { enabled: bool },
    SetAntiAfk { enabled: bool },
    WebhookOut { event: String, payload: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel {
    #[default]
    Native,
    Discord,
    Telegram,
    Slack,
    Ntfy,
    Pushover,
    Email,
    Webhook,
}

impl Action {
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            Action::LockVault
                | Action::KillAll
                | Action::RunScript { .. }
                | Action::OpenUrl { .. }
                | Action::CreateBackup
                | Action::StopAutomation { .. }
        )
    }

    pub fn label(&self) -> String {
        match self {
            Action::Launch { account, target, .. } => format!("Launch {}{}", target.clone().unwrap_or_default(), account.map(|a| format!(" ({a})")).unwrap_or_default()),
            Action::Restart { .. } => "Restart client".into(),
            Action::Rejoin { .. } => "Rejoin server".into(),
            Action::MoveLane { .. } => "Move account to lane".into(),
            Action::ApplyLimits { .. } => "Apply resource limits".into(),
            Action::Notify { channel, message, .. } => format!("Notify via {channel:?}: {message}"),
            Action::RunScript { script, .. } => format!("Run script {}", if script.len() > 40 { &script[..40] } else { script }),
            Action::Kill { .. } => "Kill client".into(),
            Action::KillAll => "Kill all clients".into(),
            Action::LockVault => "Lock vault".into(),
            Action::SwitchProfile { profile } => format!("Switch to {profile:?} profile"),
            Action::OpenUrl { url } => format!("Open {url}"),
            Action::PlaySound { name } => format!("Play sound {name}"),
            Action::StartScene { .. } => "Start scene".into(),
            Action::StopAutomation { .. } => "Stop automation".into(),
            Action::CreateBackup => "Create encrypted backup".into(),
            Action::SetEcoMode { .. } => "Set Eco mode".into(),
            Action::SetAntiAfk { .. } => "Set anti-AFK".into(),
            Action::WebhookOut { event, .. } => format!("Send webhook {event}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: AutomationId,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    pub cooldown_seconds: u32,
    pub daily_budget: u32,
    pub dry_run: bool,
    pub schedule: Option<Schedule>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub stop_on_error: bool,
}

impl Automation {
    pub fn new(name: impl Into<String>, trigger: Trigger) -> Self {
        let now = Utc::now();
        Self {
            id: AutomationId::new(),
            name: name.into(),
            description: String::new(),
            enabled: true,
            trigger,
            conditions: Vec::new(),
            actions: Vec::new(),
            cooldown_seconds: 60,
            daily_budget: 50,
            dry_run: false,
            schedule: None,
            last_run: None,
            run_count: 0,
            created_at: now,
            updated_at: now,
            stop_on_error: true,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Automation needs a name".into());
        }
        if self.actions.is_empty() {
            return Err("Automation has no actions".into());
        }
        if self.actions.len() > 32 {
            return Err("Too many actions (max 32)".into());
        }
        if self.cooldown_seconds < 5 {
            return Err("Cooldown must be at least 5 seconds".into());
        }
        if let Trigger::TimeOfDay { cron } = &self.trigger {
            crate::schedule::CronExpr::parse(cron).map_err(|e| e.to_string())?;
        }
        if let Trigger::BatteryLow { percent } = &self.trigger {
            if *percent > 100 {
                return Err("Battery threshold must be <= 100".into());
            }
        }
        if self.actions.iter().filter(|a| **a == Action::KillAll).count() > 1 {
            return Err("Kill-all can only appear once".into());
        }
        if self.daily_budget == 0 {
            return Err("Daily budget must be at least 1".into());
        }
        Ok(())
    }

    pub fn may_fire(&self, now: DateTime<Utc>, runs_today: u32) -> bool {
        if !self.enabled || self.dry_run {
            return self.enabled && self.dry_run;
        }
        if runs_today >= self.daily_budget {
            return false;
        }
        if let Some(last) = self.last_run {
            if (now - last).num_seconds() < self.cooldown_seconds as i64 {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Succeeded,
    Partial,
    Failed,
    Skipped,
    DryRun,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStep {
    pub action: Action,
    pub status: RunStatus,
    pub detail: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: uuid::Uuid,
    pub automation_id: AutomationId,
    pub automation_name: String,
    pub at: DateTime<Utc>,
    pub trigger_summary: String,
    pub status: RunStatus,
    pub steps: Vec<RunStep>,
    pub dry_run: bool,
}

impl AutomationRun {
    pub fn started(automation: &Automation, at: DateTime<Utc>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            automation_id: automation.id,
            automation_name: automation.name.clone(),
            at,
            trigger_summary: format!("{:?}", automation.trigger),
            status: RunStatus::Succeeded,
            steps: Vec::new(),
            dry_run: automation.dry_run,
        }
    }

    pub fn finish(&mut self) {
        if self.steps.iter().any(|s| s.status == RunStatus::Failed) {
            self.status = if self.steps.iter().any(|s| s.status == RunStatus::Succeeded) {
                RunStatus::Partial
            } else {
                RunStatus::Failed
            };
        } else if self.dry_run {
            self.status = RunStatus::DryRun;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_validation_requires_actions_and_name() {
        let mut a = Automation::new("", Trigger::Manual);
        assert!(a.validate().is_err());
        a.name = "Test".into();
        assert!(a.validate().is_err());
        a.actions.push(Action::KillAll);
        assert!(a.validate().is_ok());
        a.trigger = Trigger::TimeOfDay { cron: "bad".into() };
        assert!(a.validate().is_err());
    }

    #[test]
    fn cooldown_and_budget_gate_firing() {
        let mut a = Automation::new("x", Trigger::Manual);
        a.actions.push(Action::KillAll);
        a.cooldown_seconds = 60;
        a.daily_budget = 2;
        let now = Utc::now();
        a.last_run = Some(now - chrono::Duration::seconds(30));
        assert!(!a.may_fire(now, 0));
        a.last_run = Some(now - chrono::Duration::seconds(61));
        assert!(a.may_fire(now, 1));
        assert!(!a.may_fire(now, 2));
    }

    #[test]
    fn dry_run_automations_always_evaluate() {
        let mut a = Automation::new("dry", Trigger::Manual);
        a.actions.push(Action::Notify { channel: NotifyChannel::Native, message: "hi".into(), severity: 1 });
        a.dry_run = true;
        a.enabled = true;
        assert!(a.may_fire(Utc::now(), 1000));
    }

    #[test]
    fn run_status_rolls_up_partial_failures() {
        let mut a = Automation::new("x", Trigger::Manual);
        let mut run = AutomationRun::started(&a, Utc::now());
        run.steps.push(RunStep { action: Action::KillAll, status: RunStatus::Succeeded, detail: String::new(), duration_ms: 1 });
        run.steps.push(RunStep { action: Action::LockVault, status: RunStatus::Failed, detail: "no".into(), duration_ms: 1 });
        run.finish();
        assert_eq!(run.status, RunStatus::Partial);
        let _ = &mut a;
    }

    #[test]
    fn compare_ops_are_correct() {
        assert!(CompareOp::GreaterThan.eval_f64(2.0, 1.0));
        assert!(!CompareOp::LessOrEqual.eval_f64(2.0, 1.0));
        assert!(CompareOp::Equals.eval_f64(1.0, 1.0));
        assert!(CompareOp::NotEquals.eval_f64(1.0, 2.0));
    }
}
