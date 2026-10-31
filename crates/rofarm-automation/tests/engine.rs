use chrono::{NaiveDate, Timelike, Utc};
use rofarm_automation::{
    describe, due_schedules, evaluate, from_yaml, run_console, to_yaml, EvalContext, EvalPolicy, EngineEvent,
};
use rofarm_core::account::Account;
use rofarm_core::automation::{Action, Automation, Condition, NotifyChannel, Trigger};
use rofarm_core::ids::{builtin, LaneId};
use rofarm_core::lane::Lane;
use rofarm_core::schedule::{CronExpr, Schedule};
use std::collections::BTreeSet;

fn now() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 30).unwrap().and_hms_opt(20, 30, 0).unwrap()
}

fn context() -> EvalContext {
    let mut state = rofarm_core::fleet::FleetState::default();
    let account = Account::new("alpha", builtin::farming());
    let account_id = account.id;
    state.accounts.insert(account_id, account);
    let mut ctx = EvalContext::from_state(state, now());
    ctx.battery_percent = Some(18);
    ctx.on_battery = true;
    ctx
}

fn notify_automation(trigger: Trigger) -> Automation {
    let mut automation = Automation::new("Alert", trigger);
    automation.actions.push(Action::Notify {
        channel: NotifyChannel::Native,
        message: "hello".into(),
        severity: 1,
    });
    automation
}

#[test]
fn trigger_and_condition_are_evaluated() {
    let ctx = context();
    let mut automation = notify_automation(Trigger::BatteryLow { percent: 20 });
    automation.conditions.push(Condition::OnBattery);
    let plans = evaluate(
        vec![automation],
        &EngineEvent::BatteryLow { percent: 18 },
        &ctx,
        &EvalPolicy::default(),
    );
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].actions.len(), 1);
    assert!(!plans[0].dry_run);
}

#[test]
fn failed_condition_blocks_the_run() {
    let ctx = context();
    let mut automation = notify_automation(Trigger::SessionLocked);
    automation.conditions.push(Condition::BatteryBelow { percent: 10 });
    let plans = evaluate(vec![automation], &EngineEvent::SessionLocked, &ctx, &EvalPolicy::default());
    assert!(plans.is_empty());
}

#[test]
fn dangerous_actions_are_blocked_without_confirmation() {
    let ctx = context();
    let mut automation = notify_automation(Trigger::Manual);
    automation.actions.push(Action::KillAll);
    let plans = evaluate(vec![automation.clone()], &EngineEvent::Manual { automation: None }, &ctx, &EvalPolicy::default());
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].blocked.len(), 1);
    assert!(plans[0].actions.iter().all(|a| *a != Action::KillAll));
    let allowed = evaluate(
        vec![automation],
        &EngineEvent::Manual { automation: None },
        &ctx,
        &EvalPolicy { allow_dangerous_actions: true, dry_run: false },
    );
    assert!(allowed[0].actions.iter().any(|a| *a == Action::KillAll));
}

#[test]
fn dry_run_marks_steps_without_actions() {
    let ctx = context();
    let mut automation = notify_automation(Trigger::Manual);
    automation.dry_run = true;
    let plans = evaluate(vec![automation], &EngineEvent::Manual { automation: None }, &ctx, &EvalPolicy::default());
    assert!(plans[0].dry_run);
    assert_eq!(plans[0].actions.len(), 1);
    assert!(plans[0].run.steps.iter().all(|s| s.status == rofarm_core::automation::RunStatus::DryRun));
}

#[test]
fn yaml_round_trip_preserves_rules() {
    let automation = notify_automation(Trigger::FriendOnline { username: "bob".into() });
    let yaml = to_yaml(std::slice::from_ref(&automation)).unwrap();
    let parsed = from_yaml(&yaml).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].trigger, automation.trigger);
    assert_eq!(describe(&parsed[0]).contains("bob"), true);
    assert!(from_yaml("[]").unwrap().is_empty());
    assert!(from_yaml("not: [valid").is_err());
}

#[test]
fn schedules_are_due_at_the_matching_minute() {
    let cron = CronExpr::parse("30 20 * * *").unwrap();
    let schedule = Schedule { enabled: true, cron: Some(cron), ..Default::default() };
    let due = due_schedules(&[("nightly".into(), schedule.clone())], now());
    assert_eq!(due, vec!["nightly".to_string()]);
    let later = now().with_hour(21).unwrap();
    assert!(due_schedules(&[("nightly".into(), schedule)], later).is_empty());
}

#[test]
fn script_console_is_sandboxed_and_read_only() {
    let ctx = context();
    assert_eq!(run_console("accounts", &ctx).unwrap(), "1");
    assert!(run_console("this is not rhai", &ctx).is_err());
    let long = "x".repeat(20_000);
    assert!(run_console(&long, &ctx).is_err());
    assert_eq!(run_console("if battery < 20 { \"low\" } else { \"ok\" }", &ctx).unwrap(), "low");
}

#[test]
fn time_window_condition_handles_overnight() {
    let ctx = context();
    let mut automation = notify_automation(Trigger::Manual);
    automation.conditions.push(Condition::TimeWindow {
        start_minute: 20 * 60,
        end_minute: 6 * 60,
        days: BTreeSet::new(),
    });
    let plans = evaluate(vec![automation], &EngineEvent::Manual { automation: None }, &ctx, &EvalPolicy::default());
    assert_eq!(plans.len(), 1);
    let _ = Utc::now();
    let _ = Lane::seed();
    let _: LaneId = builtin::farming();
}
