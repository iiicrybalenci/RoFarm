#![forbid(unsafe_code)]

use chrono::{Datelike, NaiveDateTime, Timelike, Utc};
use rofarm_core::account::Account;
use rofarm_core::automation::{
    Action, Automation, AutomationRun, CompareOp, Condition, MetricField, RunStatus, RunStep, Trigger,
};
use rofarm_core::client::{ClientRuntime, ClientState};
use rofarm_core::fleet::FleetState;
use rofarm_core::ids::{AccountId, ClientId, LaneId};
use rofarm_core::schedule::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub cpu_pct: f32,
    pub memory_mb: u32,
    pub gpu_pct: Option<f32>,
    pub vram_mb: Option<u32>,
    pub fps_estimate: Option<f32>,
    pub player_count: Option<u32>,
    pub disconnects_last_hour: u32,
}

impl Default for MetricSample {
    fn default() -> Self {
        Self { cpu_pct: 0.0, memory_mb: 0, gpu_pct: None, vram_mb: None, fps_estimate: None, player_count: None, disconnects_last_hour: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalContext {
    pub state: FleetState,
    pub metrics: BTreeMap<ClientId, MetricSample>,
    pub now: NaiveDateTime,
    pub battery_percent: Option<u8>,
    pub on_battery: bool,
    pub session_locked: bool,
    pub foreground_exe: Option<String>,
    pub friends_online: BTreeSet<String>,
    pub robux_delta: BTreeMap<AccountId, i64>,
    pub new_roblox_version: bool,
    pub minutes_used_today: BTreeMap<AccountId, u32>,
}

impl EvalContext {
    pub fn from_state(state: FleetState, now: NaiveDateTime) -> Self {
        Self {
            state,
            metrics: BTreeMap::new(),
            now,
            battery_percent: None,
            on_battery: false,
            session_locked: false,
            foreground_exe: None,
            friends_online: BTreeSet::new(),
            robux_delta: BTreeMap::new(),
            new_roblox_version: false,
            minutes_used_today: BTreeMap::new(),
        }
    }

    fn account(&self, id: AccountId) -> Option<&Account> {
        self.state.accounts.get(&id)
    }

    fn clients_for(&self, account: Option<AccountId>) -> Vec<&ClientRuntime> {
        self.state
            .clients
            .values()
            .filter(|c| account.map_or(true, |a| c.account_id == a))
            .collect()
    }

    fn metric_for(&self, account: Option<AccountId>, field: MetricField) -> Option<f64> {
        let clients = self.clients_for(account);
        let sum = |f: fn(&MetricSample) -> Option<f32>| -> Option<f64> {
            let values: Vec<f64> = clients
                .iter()
                .filter_map(|c| self.metrics.get(&c.id))
                .filter_map(f)
                .map(|v| v as f64)
                .collect();
            if values.is_empty() { None } else { Some(values.iter().sum()) }
        };
        match field {
            MetricField::CpuPct => sum(|m| Some(m.cpu_pct)),
            MetricField::MemoryMb => sum(|m| Some(m.memory_mb as f32)),
            MetricField::GpuPct => sum(|m| m.gpu_pct),
            MetricField::VramMb => sum(|m| m.vram_mb.map(|v| v as f32)),
            MetricField::FpsEstimate => sum(|m| m.fps_estimate),
            MetricField::PlayerCount => sum(|m| m.player_count.map(|v| v as f32)),
            MetricField::DisconnectsLastHour => sum(|m| Some(m.disconnects_last_hour as f32)),
            MetricField::Robux => account.and_then(|a| self.account(a)).map(|a| a.robux.unwrap_or(0) as f64),
            MetricField::Friends => account.and_then(|a| self.account(a)).map(|a| a.friend_count as f64),
            MetricField::HealthScore => account.and_then(|a| self.account(a)).map(|a| a.health.score as f64),
            MetricField::PlaytimeMinutes => {
                let seconds: u64 = clients.iter().map(|c| c.playtime_seconds).sum();
                Some(seconds as f64 / 60.0)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EngineEvent {
    ClientCrashed { account: AccountId, client: ClientId },
    ClientDisconnected { account: AccountId, client: ClientId },
    ClientFrozen { account: AccountId, client: ClientId },
    AccountValidated { account: AccountId },
    AccountValidationFailed { account: AccountId },
    FriendOnline { username: String },
    RobuxChanged { account: AccountId, delta: i64 },
    BatteryLow { percent: u8 },
    NetworkChanged,
    SessionLocked,
    SessionUnlocked,
    ForegroundApp { executable: String },
    ForegroundGame { universe_id: u64 },
    RobloxVersionChanged,
    Tick,
    Manual { automation: Option<rofarm_core::ids::AutomationId> },
    Webhook { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicy {
    pub allow_dangerous_actions: bool,
    pub dry_run: bool,
}

impl Default for EvalPolicy {
    fn default() -> Self {
        Self { allow_dangerous_actions: false, dry_run: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedRun {
    pub automation_id: rofarm_core::ids::AutomationId,
    pub automation_name: String,
    pub run: AutomationRun,
    pub actions: Vec<Action>,
    pub blocked: Vec<(Action, String)>,
    pub dry_run: bool,
}

#[derive(Debug, Error)]
pub enum AutomationError {
    #[error("{0}")]
    Yaml(String),
    #[error("{0}")]
    Invalid(String),
}

fn trigger_matches(trigger: &Trigger, event: &EngineEvent) -> bool {
    use EngineEvent as E;
    let account_match = |wanted: &Option<AccountId>, got: AccountId| wanted.map_or(true, |w| w == got);
    match (trigger, event) {
        (Trigger::ClientCrashed { account }, E::ClientCrashed { account: got, .. }) => account_match(account, *got),
        (Trigger::ClientDisconnected { account }, E::ClientDisconnected { account: got, .. }) => account_match(account, *got),
        (Trigger::ClientFrozen { account }, E::ClientFrozen { account: got, .. }) => account_match(account, *got),
        (Trigger::AccountValidated { account }, E::AccountValidated { account: got }) => account_match(account, *got),
        (Trigger::AccountValidationFailed { account }, E::AccountValidationFailed { account: got }) => {
            account_match(account, *got)
        }
        (Trigger::FriendOnline { username }, E::FriendOnline { username: got }) => username.eq_ignore_ascii_case(got),
        (Trigger::RobuxChanged { account, delta_at_least }, E::RobuxChanged { account: got, delta }) => {
            account_match(account, *got) && delta_at_least.map_or(true, |min| delta.abs() >= min)
        }
        (Trigger::BatteryLow { percent }, E::BatteryLow { percent: got }) => got <= percent,
        (Trigger::NetworkChanged, E::NetworkChanged) => true,
        (Trigger::SessionLocked, E::SessionLocked) => true,
        (Trigger::SessionUnlocked, E::SessionUnlocked) => true,
        (Trigger::ForegroundAppStarted { executable }, E::ForegroundApp { executable: got }) => {
            executable.eq_ignore_ascii_case(got)
        }
        (Trigger::ForegroundGameStarted { universe_id }, E::ForegroundGame { universe_id: got }) => universe_id == got,
        (Trigger::RobloxVersionChanged, E::RobloxVersionChanged) => true,
        (Trigger::TimeOfDay { .. }, E::Tick) => true,
        (Trigger::Manual, E::Manual { automation: None }) => true,
        (Trigger::Manual, E::Manual { automation: Some(id) }) => trigger_id_matches(trigger, *id),
        (Trigger::Webhook { path }, E::Webhook { path: got }) => path == got,
        (Trigger::AutomationFinished { .. }, _) => false,
        (Trigger::MemoryAbove { .. } | Trigger::CpuAbove { .. }, E::Tick) => true,
        _ => false,
    }
}

fn trigger_id_matches(_trigger: &Trigger, _id: rofarm_core::ids::AutomationId) -> bool {
    true
}

fn condition_holds(condition: &Condition, ctx: &EvalContext) -> bool {
    match condition {
        Condition::Metric { account, field, op, value } => ctx
            .metric_for(*account, field.clone())
            .is_some_and(|actual| op.eval_f64(actual, *value)),
        Condition::AccountHealthBelow { account, score } => {
            ctx.account(*account).is_some_and(|a| a.health.score < *score)
        }
        Condition::AccountInLane { account, lane } => ctx.account(*account).is_some_and(|a| a.lane_id == *lane),
        Condition::ClientStateIs { account, state } => ctx.clients_for(Some(*account)).iter().any(|c| c.state == *state),
        Condition::TimeWindow { start_minute, end_minute, days } => {
            let day = ctx.now.weekday().num_days_from_monday() as u8;
            let minute = ctx.now.hour() * 60 + ctx.now.minute();
            if !days.is_empty() && !days.contains(&day) {
                return false;
            }
            if start_minute <= end_minute {
                (*start_minute..*end_minute).contains(&minute)
            } else {
                minute >= *start_minute || minute < *end_minute
            }
        }
        Condition::BatteryBelow { percent } => ctx.battery_percent.is_some_and(|b| b < *percent),
        Condition::OnBattery => ctx.on_battery,
        Condition::DayOfWeek { days } => days.contains(&(ctx.now.weekday().num_days_from_monday() as u8)),
        Condition::Script { rhai } => eval_condition_script(rhai, ctx).unwrap_or(false),
        Condition::Not { condition } => !condition_holds(condition, ctx),
    }
}

fn trigger_context_holds(trigger: &Trigger, ctx: &EvalContext) -> bool {
    match trigger {
        Trigger::MemoryAbove { account, megabytes } => ctx
            .metric_for(*account, MetricField::MemoryMb)
            .is_some_and(|v| v > *megabytes as f64),
        Trigger::CpuAbove { account, percent } => ctx
            .metric_for(*account, MetricField::CpuPct)
            .is_some_and(|v| v > *percent as f64),
        Trigger::TimeOfDay { cron } => rofarm_core::schedule::CronExpr::parse(cron)
            .map(|c| c.matches(ctx.now))
            .unwrap_or(false),
        _ => true,
    }
}

pub fn evaluate(
    automations: impl IntoIterator<Item = Automation>,
    event: &EngineEvent,
    ctx: &EvalContext,
    policy: &EvalPolicy,
) -> Vec<PlannedRun> {
    let today = ctx.now.date();
    let mut runs_today: BTreeMap<_, u32> = BTreeMap::new();
    for run in &ctx.state.automation_runs {
        if run.at.date_naive() == today {
            *runs_today.entry(run.automation_id).or_default() += 1;
        }
    }
    let mut plans = Vec::new();
    for automation in automations {
        if !automation.enabled {
            continue;
        }
        if let Some(schedule) = &automation.schedule {
            if !schedule.allows(ctx.now, ctx.minutes_used_today.get(&ctx.state.accounts.keys().next().copied().unwrap_or_default()).copied().unwrap_or(0)) {
                continue;
            }
        }
        if !trigger_matches(&automation.trigger, event) || !trigger_context_holds(&automation.trigger, ctx) {
            continue;
        }
        if !automation.conditions.iter().all(|c| condition_holds(c, ctx)) {
            continue;
        }
        let runs = runs_today.get(&automation.id).copied().unwrap_or(0);
        if runs >= automation.daily_budget {
            continue;
        }
        if let Some(last) = automation.last_run {
            if (Utc::now() - last).num_seconds() < automation.cooldown_seconds as i64 {
                continue;
            }
        }
        let dry_run = automation.dry_run || policy.dry_run;
        let mut run = AutomationRun::started(&automation, Utc::now());
        run.dry_run = dry_run;
        let mut actions = Vec::new();
        let mut blocked = Vec::new();
        for action in &automation.actions {
            if action.is_dangerous() && !policy.allow_dangerous_actions && !dry_run {
                blocked.push((action.clone(), "Needs explicit confirmation in Settings → Automations.".into()));
                continue;
            }
            run.steps.push(RunStep {
                action: action.clone(),
                status: if dry_run { RunStatus::DryRun } else { RunStatus::Succeeded },
                detail: if dry_run { format!("Would {}", action.label()) } else { action.label() },
                duration_ms: 0,
            });
            actions.push(action.clone());
        }
        run.finish();
        plans.push(PlannedRun {
            automation_id: automation.id,
            automation_name: automation.name,
            run,
            actions,
            blocked,
            dry_run,
        });
    }
    plans
}

pub fn due_schedules(schedules: &[(String, Schedule)], now: NaiveDateTime) -> Vec<String> {
    schedules
        .iter()
        .filter(|(_, schedule)| {
            schedule.cron.as_ref().is_some_and(|cron| cron.matches(now))
                && schedule.enabled
                && !schedule.is_blackout(now.date())
        })
        .map(|(id, _)| id.clone())
        .collect()
}

pub fn to_yaml(automations: &[Automation]) -> Result<String, AutomationError> {
    serde_yaml::to_string(automations).map_err(|e| AutomationError::Yaml(e.to_string()))
}

pub fn from_yaml(yaml: &str) -> Result<Vec<Automation>, AutomationError> {
    let automations: Vec<Automation> =
        serde_yaml::from_str(yaml).map_err(|e| AutomationError::Yaml(e.to_string()))?;
    for automation in &automations {
        automation.validate().map_err(AutomationError::Invalid)?;
    }
    Ok(automations)
}

pub fn validate(automation: &Automation) -> Result<(), AutomationError> {
    automation.validate().map_err(AutomationError::Invalid)?;
    if automation.actions.iter().filter(|a| a.is_dangerous()).count() > 3 {
        return Err(AutomationError::Invalid(
            "At most three dangerous actions are allowed in one automation.".into(),
        ));
    }
    Ok(())
}

fn base_engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(20_000);
    engine.set_max_expr_depths(24, 12);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(512);
    engine.set_max_map_size(128);
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    engine
}

fn script_scope(ctx: &EvalContext) -> rhai::Scope<'static> {
    let mut scope = rhai::Scope::new();
    scope.push("now_hour", ctx.now.hour() as i64);
    scope.push("now_minute", ctx.now.minute() as i64);
    scope.push("day_of_week", ctx.now.weekday().num_days_from_monday() as i64);
    scope.push("battery", ctx.battery_percent.map(|b| b as i64).unwrap_or(100));
    scope.push("on_battery", ctx.on_battery);
    scope.push("session_locked", ctx.session_locked);
    scope.push("accounts", ctx.state.accounts.len() as i64);
    scope.push("running", ctx.state.clients.values().filter(|c| c.state.is_running()).count() as i64);
    scope.push("needs_login", ctx.state.accounts.values()        .filter(|a| a.login.kind.needs_attention()).count() as i64);
    let total_cpu: f64 = ctx.metrics.values().map(|m| m.cpu_pct as f64).sum();
    let total_mem: i64 = ctx.metrics.values().map(|m| m.memory_mb as i64).sum();
    scope.push("total_cpu", total_cpu);
    scope.push("total_memory_mb", total_mem);
    scope
}

fn eval_condition_script(source: &str, ctx: &EvalContext) -> Option<bool> {
    if source.len() > 2048 {
        return None;
    }
    let engine = base_engine();
    let scope = script_scope(ctx);
    engine
        .eval_with_scope::<bool>(&mut { scope }, source)
        .ok()
}

pub fn run_console(source: &str, ctx: &EvalContext) -> Result<String, String> {
    if source.len() > 16 * 1024 {
        return Err("Script is too large (16 KB max).".into());
    }
    let engine = base_engine();
    let mut scope = script_scope(ctx);
    match engine.eval_with_scope::<rhai::Dynamic>(&mut scope, source) {
        Ok(value) => Ok(if value.is_unit() {
            "ok".to_string()
        } else {
            format!("{value}")
        }),
        Err(error) => Err(error.to_string()),
    }
}

pub fn describe(automation: &Automation) -> String {
    let trigger = match &automation.trigger {
        Trigger::ClientCrashed { .. } => "When a client crashes".to_string(),
        Trigger::ClientDisconnected { .. } => "When a client disconnects".to_string(),
        Trigger::MemoryAbove { megabytes, .. } => format!("When memory exceeds {megabytes} MB"),
        Trigger::CpuAbove { percent, .. } => format!("When CPU exceeds {percent}%"),
        Trigger::TimeOfDay { cron } => format!("On schedule {cron}"),
        Trigger::FriendOnline { username } => format!("When @{username} comes online"),
        Trigger::BatteryLow { percent } => format!("When battery drops below {percent}%"),
        other => format!("{other:?}"),
    };
    let actions: Vec<String> = automation.actions.iter().map(Action::label).collect();
    format!("{trigger} → {}", actions.join(", "))
}

pub fn perfection_check(accounts: &[Account], clients: &[ClientRuntime], lanes: &BTreeMap<LaneId, rofarm_core::lane::Lane>) -> Vec<String> {
    let mut notes = Vec::new();
    if accounts.iter().any(|a| a.login.kind.needs_attention()) {
        notes.push("Some accounts need to sign in again.".into());
    }
    let running = clients.iter().filter(|c| c.state == ClientState::InGame).count();
    for lane in lanes.values() {
        if let Some(max) = lane.rules.max_running {
            let in_lane = clients.iter().filter(|c| c.lane_id == lane.id && c.state.is_running()).count();
            if in_lane > max as usize {
                notes.push(format!("Lane {} is over its limit ({in_lane}/{max}).", lane.name));
            }
        }
    }
    if running == 0 && !accounts.is_empty() {
        notes.push("No clients are in game. Launch a scene to start farming.".into());
    }
    notes
}

pub fn compare(op: CompareOp, left: f64, right: f64) -> bool {
    op.eval_f64(left, right)
}
