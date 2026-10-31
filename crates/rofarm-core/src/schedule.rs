use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronField {
    pub allowed: BTreeSet<u32>,
    pub wildcard: bool,
}

impl CronField {
    pub fn parse(raw: &str, min: u32, max: u32, names: &[&str]) -> Result<Self> {
        let mut allowed = BTreeSet::new();
        let wildcard = raw.trim() == "*";
        for part in raw.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(CoreError::Invalid(format!("empty cron field: {raw}")));
            }
            let (range_part, step) = match part.split_once('/') {
                Some((r, s)) => {
                    let step: u32 = s
                        .parse()
                        .map_err(|_| CoreError::Invalid(format!("bad cron step: {s}")))?;
                    if step == 0 {
                        return Err(CoreError::Invalid("cron step cannot be 0".into()));
                    }
                    (r, step)
                }
                None => (part, 1),
            };
            let (lo, hi) = if range_part == "*" {
                (min, max)
            } else if let Some((a, b)) = range_part.split_once('-') {
                (parse_value(a, min, max, names)?, parse_value(b, min, max, names)?)
            } else {
                let v = parse_value(range_part, min, max, names)?;
                if step > 1 {
                    (v, max)
                } else {
                    (v, v)
                }
            };
            if lo < min || hi > max || lo > hi {
                return Err(CoreError::Invalid(format!(
                    "cron value out of range ({lo}-{hi}) for {raw}"
                )));
            }
            let mut v = lo;
            while v <= hi {
                allowed.insert(v);
                v += step;
            }
        }
        if allowed.is_empty() {
            return Err(CoreError::Invalid("cron field matched nothing".into()));
        }
        Ok(Self { allowed, wildcard })
    }

    fn matches(&self, value: u32) -> bool {
        self.allowed.contains(&value)
    }
}

fn parse_value(raw: &str, min: u32, max: u32, names: &[&str]) -> Result<u32> {
    let trimmed = raw.trim();
    if let Ok(v) = trimmed.parse::<u32>() {
        return Ok(v);
    }
    let upper = trimmed.to_ascii_uppercase();
    if let Some(idx) = names.iter().position(|n| n.eq_ignore_ascii_case(&upper)) {
        return Ok(min + idx as u32);
    }
    let _ = max;
    Err(CoreError::Invalid(format!("bad cron value: {raw}")))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronExpr {
    pub source: String,
    pub minutes: CronField,
    pub hours: CronField,
    pub days_of_month: CronField,
    pub months: CronField,
    pub days_of_week: CronField,
}

impl CronExpr {
    pub fn parse(source: &str) -> Result<Self> {
        let parts: Vec<&str> = source.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(CoreError::Invalid("cron expression needs exactly 5 fields".into()));
        }
        Ok(Self {
            source: source.trim().to_string(),
            minutes: CronField::parse(parts[0], 0, 59, &[])?,
            hours: CronField::parse(parts[1], 0, 23, &[])?,
            days_of_month: CronField::parse(parts[2], 1, 31, &[])?,
            months: CronField::parse(
                parts[3],
                1,
                12,
                &["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"],
            )?,
            days_of_week: CronField::parse(parts[4], 0, 6, &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"])?,
        })
    }

    fn day_matches(&self, date: NaiveDate) -> bool {
        let dow = date.weekday().num_days_from_sunday();
        let dom = self.days_of_month.matches(date.day());
        let dow_ok = self.days_of_week.matches(dow);
        let day_ok = if !self.days_of_month.wildcard && !self.days_of_week.wildcard {
            dom || dow_ok
        } else if !self.days_of_month.wildcard {
            dom
        } else if !self.days_of_week.wildcard {
            dow_ok
        } else {
            true
        };
        day_ok && self.months.matches(date.month())
    }

    pub fn next_after(&self, after: NaiveDateTime) -> Option<NaiveDateTime> {
        let start = after + Duration::minutes(1);
        let start = start.with_second(0)?.with_nanosecond(0)?;
        let mut date = start.date();
        let limit = start.date() + Duration::days(366 * 5);
        while date <= limit {
            if self.day_matches(date) {
                let is_start_day = date == start.date();
                for hour in &self.hours.allowed {
                    if is_start_day && *hour < start.hour() {
                        continue;
                    }
                    for minute in &self.minutes.allowed {
                        if is_start_day && *hour == start.hour() && *minute < start.minute() {
                            continue;
                        }
                        let candidate = date.and_hms_opt(*hour, *minute, 0)?;
                        if candidate > after {
                            return Some(candidate);
                        }
                    }
                }
            }
            date += Duration::days(1);
        }
        None
    }

    pub fn matches(&self, at: NaiveDateTime) -> bool {
        self.minutes.matches(at.minute())
            && self.hours.matches(at.hour())
            && self.day_matches(at.date())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWindow {
    pub days: BTreeSet<u8>,
    pub start_minute: u32,
    pub end_minute: u32,
}

impl SessionWindow {
    pub fn every_day(start_minute: u32, end_minute: u32) -> Self {
        Self { days: (0..7).collect(), start_minute, end_minute }
    }

    pub fn validate(&self) -> Result<()> {
        if self.start_minute >= 24 * 60 || self.end_minute >= 24 * 60 {
            return Err(CoreError::Invalid("session window minutes out of range".into()));
        }
        if self.days.is_empty() || self.days.iter().any(|d| *d > 6) {
            return Err(CoreError::Invalid("session window days out of range".into()));
        }
        Ok(())
    }

    fn day_index(date: NaiveDate) -> u8 {
        date.weekday().num_days_from_monday() as u8
    }

    pub fn contains(&self, at: NaiveDateTime) -> bool {
        let minute = at.hour() * 60 + at.minute();
        if self.start_minute <= self.end_minute {
            self.days.contains(&Self::day_index(at.date()))
                && (self.start_minute..self.end_minute).contains(&minute)
        } else {
            let today = Self::day_index(at.date());
            if minute >= self.start_minute && self.days.contains(&today) {
                return true;
            }
            let yesterday = at.date() - Duration::days(1);
            minute < self.end_minute && self.days.contains(&Self::day_index(yesterday))
        }
    }

    pub fn next_start(&self, after: NaiveDateTime) -> Option<NaiveDateTime> {
        let mut date = after.date();
        for _ in 0..=8 {
            if self.days.contains(&Self::day_index(date)) {
                let start = date.and_hms_opt(self.start_minute / 60, self.start_minute % 60, 0)?;
                if start > after {
                    return Some(start);
                }
            }
            date += Duration::days(1);
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blackout {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub reason: String,
}

impl Blackout {
    pub fn contains(&self, date: NaiveDate) -> bool {
        date >= self.from && date <= self.to
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Schedule {
    pub cron: Option<CronExpr>,
    pub window: Option<SessionWindow>,
    pub daily_budget_minutes: Option<u32>,
    pub blackouts: Vec<Blackout>,
    pub wake_from_sleep: bool,
    pub stop_at_window_end: bool,
    pub enabled: bool,
}

impl Schedule {
    pub fn always() -> Self {
        Self { enabled: true, ..Default::default() }
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(w) = &self.window {
            w.validate()?;
        }
        if let Some(budget) = self.daily_budget_minutes {
            if budget == 0 || budget > 24 * 60 {
                return Err(CoreError::Invalid("daily budget must be 1..1440 minutes".into()));
            }
        }
        for b in &self.blackouts {
            if b.to < b.from {
                return Err(CoreError::Invalid("blackout range ends before it starts".into()));
            }
        }
        Ok(())
    }

    pub fn is_blackout(&self, date: NaiveDate) -> bool {
        self.blackouts.iter().any(|b| b.contains(date))
    }

    pub fn allows(&self, now: NaiveDateTime, used_today_minutes: u32) -> bool {
        if !self.enabled || self.is_blackout(now.date()) {
            return false;
        }
        if let Some(cron) = &self.cron {
            let started_today = cron.next_after(now - Duration::hours(24)).is_some_and(|t| t.date() == now.date());
            if !started_today && !cron.matches(now) {
                return false;
            }
        }
        if let Some(window) = &self.window {
            if !window.contains(now) {
                return false;
            }
        }
        if let Some(budget) = self.daily_budget_minutes {
            if used_today_minutes >= budget {
                return false;
            }
        }
        true
    }

    pub fn next_wake(&self, after: NaiveDateTime) -> Option<NaiveDateTime> {
        if !self.enabled {
            return None;
        }
        let mut cursor = after;
        for _ in 0..370 {
            let mut candidates: Vec<NaiveDateTime> = Vec::new();
            if let Some(cron) = &self.cron {
                if let Some(next) = cron.next_after(cursor) {
                    candidates.push(next);
                }
            }
            if let Some(window) = &self.window {
                if let Some(next) = window.next_start(cursor) {
                    candidates.push(next);
                }
            }
            let next = candidates.into_iter().min()?;
            if !self.is_blackout(next.date()) {
                return Some(next);
            }
            cursor = next;
        }
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunBudgetTracker {
    pub used_minutes: std::collections::BTreeMap<NaiveDate, u32>,
}

impl RunBudgetTracker {
    pub fn used_on(&self, date: NaiveDate) -> u32 {
        self.used_minutes.get(&date).copied().unwrap_or(0)
    }

    pub fn record(&mut self, date: NaiveDate, minutes: u32) {
        *self.used_minutes.entry(date).or_default() += minutes;
        let cutoff = date - Duration::days(60);
        self.used_minutes.retain(|d, _| *d >= cutoff);
    }

    pub fn remaining(&self, date: NaiveDate, schedule: &Schedule) -> Option<u32> {
        schedule
            .daily_budget_minutes
            .map(|budget| budget.saturating_sub(self.used_on(date)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn parses_fields_aliases_and_steps() {
        let c = CronExpr::parse("*/15 8-10 * * MON-FRI").unwrap();
        assert!(c.minutes.matches(0) && c.minutes.matches(45) && !c.minutes.matches(7));
        assert!(c.hours.matches(8) && c.hours.matches(10) && !c.hours.matches(11));
        assert!(c.days_of_week.matches(1) && c.days_of_week.matches(5));
        assert!(!c.days_of_week.matches(0) && !c.days_of_week.matches(6));
        assert_eq!(c.minutes.allowed.len(), 4);
    }

    #[test]
    fn rejects_bad_cron() {
        assert!(CronExpr::parse("* * *").is_err());
        assert!(CronExpr::parse("60 * * * *").is_err());
        assert!(CronExpr::parse("*/0 * * * *").is_err());
        assert!(CronExpr::parse("* * * * FUNDAY").is_err());
        assert!(CronExpr::parse("5-1 * * * *").is_err());
    }

    #[test]
    fn next_after_finds_the_expected_instant() {
        let c = CronExpr::parse("30 20 * * *").unwrap();
        let after = dt(2026, 10, 30, 21, 0);
        assert_eq!(c.next_after(after), Some(dt(2026, 10, 31, 20, 30)));
        let after2 = dt(2026, 10, 30, 19, 0);
        assert_eq!(c.next_after(after2), Some(dt(2026, 10, 30, 20, 30)));
    }

    #[test]
    fn cron_dom_or_dow_semantics() {
        let c = CronExpr::parse("0 0 1 * MON").unwrap();
        assert!(c.day_matches(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));
        assert!(c.day_matches(NaiveDate::from_ymd_opt(2026, 6, 8).unwrap()));
        assert!(c.day_matches(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()));
        assert!(!c.day_matches(NaiveDate::from_ymd_opt(2026, 6, 9).unwrap()));
    }

    #[test]
    fn session_window_handles_overnight_and_days() {
        let w = SessionWindow { days: [0, 1, 2, 3, 4].into(), start_minute: 20 * 60, end_minute: 6 * 60 };
        assert!(w.contains(dt(2026, 10, 30, 21, 0)));
        assert!(w.contains(dt(2026, 10, 31, 5, 0)));
        assert!(!w.contains(dt(2026, 10, 31, 21, 0)));
        assert!(!w.contains(dt(2026, 10, 30, 7, 0)));
    }

    #[test]
    fn next_wake_respects_blackouts() {
        let schedule = Schedule {
            enabled: true,
            window: Some(SessionWindow::every_day(20 * 60, 23 * 60)),
            blackouts: vec![Blackout {
                from: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
                to: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
                reason: "holiday".into(),
            }],
            ..Default::default()
        };
        let wake = schedule.next_wake(dt(2026, 10, 30, 23, 30)).unwrap();
        assert_eq!(wake, dt(2026, 11, 1, 20, 0));
    }

    #[test]
    fn budget_blocks_after_limit() {
        let schedule = Schedule {
            enabled: true,
            daily_budget_minutes: Some(60),
            ..Default::default()
        };
        let now = dt(2026, 10, 30, 12, 0);
        assert!(schedule.allows(now, 59));
        assert!(!schedule.allows(now, 60));
        let mut tracker = RunBudgetTracker::default();
        tracker.record(now.date(), 25);
        assert_eq!(tracker.remaining(now.date(), &schedule), Some(35));
    }

    #[test]
    fn disabled_schedule_never_allows() {
        assert!(!Schedule::default().allows(dt(2026, 1, 1, 0, 0), 0));
    }
}
