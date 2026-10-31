use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::ids::AccountId;
use crate::lane::ResourceLimits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    #[default]
    Offline,
    Online,
    InGame,
    Studio,
    Unknown,
}

impl Presence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Presence::Offline => "offline",
            Presence::Online => "online",
            Presence::InGame => "in-game",
            Presence::Studio => "studio",
            Presence::Unknown => "unknown",
        }
    }

    pub fn from_api(value: u8) -> Self {
        match value {
            1 => Presence::Online,
            2 => Presence::InGame,
            3 => Presence::Studio,
            _ => Presence::Offline,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Presence::Online | Presence::InGame | Presence::Studio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoginStateKind {
    #[default]
    Unknown,
    Valid,
    Expired,
    NeedsRelogin,
    TwoStepPrompt,
    RateLimited,
    Banned,
}

impl LoginStateKind {
    pub fn label(&self) -> &'static str {
        match self {
            LoginStateKind::Unknown => "Not checked",
            LoginStateKind::Valid => "Signed in",
            LoginStateKind::Expired => "Session expired",
            LoginStateKind::NeedsRelogin => "Needs re-login",
            LoginStateKind::TwoStepPrompt => "2-step required",
            LoginStateKind::RateLimited => "Rate limited",
            LoginStateKind::Banned => "Unavailable",
        }
    }

    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            LoginStateKind::Expired
                | LoginStateKind::NeedsRelogin
                | LoginStateKind::TwoStepPrompt
                | LoginStateKind::RateLimited
                | LoginStateKind::Banned
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginState {
    pub kind: LoginStateKind,
    pub checked_at: Option<DateTime<Utc>>,
    pub detail: Option<String>,
}

impl Default for LoginState {
    fn default() -> Self {
        Self { kind: LoginStateKind::Unknown, checked_at: None, detail: None }
    }
}

impl LoginState {
    pub fn valid(at: DateTime<Utc>) -> Self {
        Self { kind: LoginStateKind::Valid, checked_at: Some(at), detail: None }
    }

    pub fn issue(kind: LoginStateKind, detail: impl Into<String>, at: DateTime<Utc>) -> Self {
        Self { kind, checked_at: Some(at), detail: Some(detail.into()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthGrade {
    #[default]
    Unknown,
    Excellent,
    Good,
    Fair,
    Poor,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthIssue {
    pub code: String,
    pub severity: u8,
    pub message: String,
    pub suggested_fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthScore {
    pub score: u8,
    pub grade: HealthGrade,
    pub issues: Vec<HealthIssue>,
    pub computed_at: DateTime<Utc>,
}

impl Default for HealthScore {
    fn default() -> Self {
        Self {
            score: 100,
            grade: HealthGrade::Unknown,
            issues: Vec::new(),
            computed_at: Utc::now(),
        }
    }
}

impl HealthScore {
    pub fn from_score(score: u8, issues: Vec<HealthIssue>) -> Self {
        let score = score.min(100);
        let grade = match score {
            90..=100 => HealthGrade::Excellent,
            75..=89 => HealthGrade::Good,
            55..=74 => HealthGrade::Fair,
            30..=54 => HealthGrade::Poor,
            _ => HealthGrade::Critical,
        };
        Self { score, grade, issues, computed_at: Utc::now() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TwoStep {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub roblox_user_id: Option<u64>,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub lane_id: crate::ids::LaneId,
    pub alias: Option<String>,
    pub notes: String,
    pub tags: BTreeSet<String>,
    pub color: String,
    pub favourite: bool,
    pub custom_fields: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub last_validated: Option<DateTime<Utc>>,
    pub cookie_expires_at: Option<DateTime<Utc>>,
    pub cookie_fingerprint: Option<String>,
    pub presence: Presence,
    pub robux: Option<u64>,
    pub premium: bool,
    pub friend_count: u32,
    pub follower_count: u32,
    pub two_step: TwoStep,
    pub account_age_days: Option<u32>,
    pub health: HealthScore,
    pub login: LoginState,
    pub playtime_seconds: u64,
    pub sessions: u32,
    pub limits: Option<ResourceLimits>,
}

impl Account {
    pub fn new(username: impl Into<String>, lane_id: crate::ids::LaneId) -> Self {
        let now = Utc::now();
        let username = username.into();
        Self {
            id: AccountId::new(),
            roblox_user_id: None,
            display_name: username.clone(),
            username,
            avatar_url: None,
            lane_id,
            alias: None,
            notes: String::new(),
            tags: BTreeSet::new(),
            color: "#6c8cff".into(),
            favourite: false,
            custom_fields: BTreeMap::new(),
            created_at: now,
            updated_at: now,
            last_seen: None,
            last_validated: None,
            cookie_expires_at: None,
            cookie_fingerprint: None,
            presence: Presence::Offline,
            robux: None,
            premium: false,
            friend_count: 0,
            follower_count: 0,
            two_step: TwoStep::default(),
            account_age_days: None,
            health: HealthScore::default(),
            login: LoginState::default(),
            playtime_seconds: 0,
            sessions: 0,
            limits: None,
        }
    }

    pub fn display(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.username)
    }

    pub fn dedupe_key(&self) -> String {
        self.roblox_user_id
            .map(|id| format!("id:{id}"))
            .unwrap_or_else(|| format!("name:{}", self.username.to_ascii_lowercase()))
    }

    pub fn age(&self, now: DateTime<Utc>) -> Duration {
        now - self.created_at
    }

    pub fn forecast_cookie_expiry(&self) -> Option<DateTime<Utc>> {
        if let Some(explicit) = self.cookie_expires_at {
            return Some(explicit);
        }
        self.last_validated.map(|at| at + Duration::days(365))
    }

    pub fn cookie_health(&self, now: DateTime<Utc>) -> CookieHealth {
        let Some(expiry) = self.forecast_cookie_expiry() else {
            return CookieHealth::Unknown;
        };
        let remaining = expiry - now;
        if remaining <= Duration::zero() {
            CookieHealth::Expired
        } else if remaining <= Duration::days(14) {
            CookieHealth::ExpiringSoon(remaining.num_days().max(0) as u32)
        } else {
            CookieHealth::Healthy(remaining.num_days() as u32)
        }
    }

    pub fn recompute_health(&mut self, now: DateTime<Utc>) -> &HealthScore {
        let mut score: i32 = 100;
        let mut issues = Vec::new();
        if self.login.kind == LoginStateKind::Expired {
            score -= 35;
            issues.push(HealthIssue {
                code: "session_expired".into(),
                severity: 3,
                message: "Roblox session has expired.".into(),
                suggested_fix: "Re-add the account with browser sign-in to refresh its cookie.".into(),
            });
        } else if self.login.kind == LoginStateKind::NeedsRelogin {
            score -= 25;
            issues.push(HealthIssue {
                code: "needs_relogin".into(),
                severity: 3,
                message: "Cookie was rejected by Roblox.".into(),
                suggested_fix: "Sign in again for this account.".into(),
            });
        } else if self.login.kind == LoginStateKind::TwoStepPrompt {
            score -= 15;
            issues.push(HealthIssue {
                code: "two_step_required".into(),
                severity: 2,
                message: "Roblox is asking for a 2-step verification code.".into(),
                suggested_fix: "Complete verification in the browser and re-import the session.".into(),
            });
        } else if self.login.kind == LoginStateKind::RateLimited {
            score -= 10;
            issues.push(HealthIssue {
                code: "rate_limited".into(),
                severity: 2,
                message: "Validation was rate limited.".into(),
                suggested_fix: "RoFarm will retry with backoff; avoid force-refreshing.".into(),
            });
        }
        if let Some(last) = self.last_validated {
            let stale_days = (now - last).num_days();
            if stale_days > 14 {
                score -= 10;
                issues.push(HealthIssue {
                    code: "stale_validation".into(),
                    severity: 1,
                    message: format!("Not validated for {stale_days} days."),
                    suggested_fix: "Run background validation now.".into(),
                });
            }
        } else if self.login.kind != LoginStateKind::Unknown {
            score -= 5;
        }
        if !self.two_step.enabled {
            score -= 15;
            issues.push(HealthIssue {
                code: "no_two_step".into(),
                severity: 2,
                message: "2-step verification is not enabled on this account.".into(),
                suggested_fix: "Enable 2-step verification in Roblox account settings.".into(),
            });
        }
        if matches!(self.cookie_health(now), CookieHealth::ExpiringSoon(_)) {
            score -= 8;
            issues.push(HealthIssue {
                code: "cookie_expiring".into(),
                severity: 1,
                message: "Forecast cookie expiry is within 14 days.".into(),
                suggested_fix: "Revalidate and refresh the cookie proactively.".into(),
            });
        }
        if self.roblox_user_id.is_none() {
            score -= 5;
            issues.push(HealthIssue {
                code: "no_profile".into(),
                severity: 1,
                message: "Account profile has not been resolved yet.".into(),
                suggested_fix: "Validate the account to fetch profile data.".into(),
            });
        }
        issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
        self.health = HealthScore::from_score(score.clamp(0, 100) as u8, issues);
        &self.health
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "days")]
pub enum CookieHealth {
    Unknown,
    Healthy(u32),
    ExpiringSoon(u32),
    Expired,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountFilter {
    pub text: String,
    pub lane_ids: Vec<crate::ids::LaneId>,
    pub tags: Vec<String>,
    pub favourite_only: bool,
    pub login: Option<LoginStateKind>,
    pub min_robux: Option<u64>,
    pub health_below: Option<u8>,
    pub presence: Option<Presence>,
}

impl AccountFilter {
    pub fn matches(&self, a: &Account) -> bool {
        if self.favourite_only && !a.favourite {
            return false;
        }
        if !self.lane_ids.is_empty() && !self.lane_ids.contains(&a.lane_id) {
            return false;
        }
        if !self.tags.is_empty() && !self.tags.iter().all(|t| a.tags.contains(t)) {
            return false;
        }
        if let Some(login) = self.login {
            if a.login.kind != login {
                return false;
            }
        }
        if let Some(min) = self.min_robux {
            if a.robux.unwrap_or(0) < min {
                return false;
            }
        }
        if let Some(max) = self.health_below {
            if a.health.score >= max {
                return false;
            }
        }
        if let Some(presence) = self.presence {
            if a.presence != presence {
                return false;
            }
        }
        if !self.text.trim().is_empty() {
            let needle = self.text.to_ascii_lowercase();
            let hay = format!(
                "{} {} {} {} {}",
                a.username,
                a.display_name,
                a.alias.as_deref().unwrap_or(""),
                a.notes,
                a.tags.iter().cloned().collect::<Vec<_>>().join(" ")
            )
            .to_ascii_lowercase();
            if !hay.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub name: String,
    pub filter: AccountFilter,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BulkEdit {
    pub alias_prefix: Option<String>,
    pub alias_suffix: Option<String>,
    pub add_tags: BTreeSet<String>,
    pub remove_tags: BTreeSet<String>,
    pub lane_id: Option<crate::ids::LaneId>,
    pub launch_profile: Option<crate::ids::LaunchProfileId>,
    pub favourite: Option<bool>,
    pub color: Option<String>,
}

impl BulkEdit {
    pub fn apply(&self, account: &mut Account) {
        if let Some(prefix) = &self.alias_prefix {
            let current = account.alias.clone().unwrap_or_else(|| account.username.clone());
            account.alias = Some(format!("{prefix}{current}"));
        }
        if let Some(suffix) = &self.alias_suffix {
            let current = account.alias.clone().unwrap_or_else(|| account.username.clone());
            account.alias = Some(format!("{current}{suffix}"));
        }
        for tag in &self.add_tags {
            account.tags.insert(tag.clone());
        }
        for tag in &self.remove_tags {
            account.tags.remove(tag);
        }
        if let Some(lane) = self.lane_id {
            account.lane_id = lane;
        }
        if let Some(fav) = self.favourite {
            account.favourite = fav;
        }
        if let Some(color) = &self.color {
            account.color = color.clone();
        }
        account.updated_at = Utc::now();
    }
}

pub fn find_duplicates(accounts: &[Account]) -> Vec<Vec<AccountId>> {
    let mut by_key: BTreeMap<String, Vec<AccountId>> = BTreeMap::new();
    for a in accounts {
        by_key.entry(a.dedupe_key()).or_default().push(a.id);
    }
    by_key.into_values().filter(|ids| ids.len() > 1).collect()
}

pub fn merge_accounts(primary: &mut Account, duplicate: &Account) {
    if primary.roblox_user_id.is_none() {
        primary.roblox_user_id = duplicate.roblox_user_id;
    }
    for tag in &duplicate.tags {
        primary.tags.insert(tag.clone());
    }
    if !duplicate.notes.is_empty() {
        if !primary.notes.is_empty() {
            primary.notes.push('\n');
        }
        primary.notes.push_str(&duplicate.notes);
    }
    for (k, v) in &duplicate.custom_fields {
        primary.custom_fields.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if duplicate.last_validated > primary.last_validated {
        primary.last_validated = duplicate.last_validated;
        primary.cookie_expires_at = duplicate.cookie_expires_at;
        primary.login = duplicate.login.clone();
        primary.cookie_fingerprint = duplicate.cookie_fingerprint.clone();
    }
    if primary.avatar_url.is_none() {
        primary.avatar_url = duplicate.avatar_url.clone();
    }
    primary.updated_at = Utc::now();
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteTemplate {
    pub name: String,
    pub body: String,
    pub checklist: Vec<String>,
}

impl NoteTemplate {
    pub fn builtin() -> Vec<Self> {
        vec![
            NoteTemplate {
                name: "New farming account".into(),
                body: "Created for {game}.\nGoal: {goal}.\nCheck after 3 days.".into(),
                checklist: vec![
                    "Verify email".into(),
                    "Enable 2-step verification".into(),
                    "Set display name".into(),
                    "Join group".into(),
                    "Set launch profile".into(),
                ],
            },
            NoteTemplate {
                name: "Trade mule".into(),
                body: "Holds limiteds. Never trade without checking net worth first.".into(),
                checklist: vec!["Confirm PIN".into(), "Lock trades in Roblox settings".into()],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::builtin;

    fn account() -> Account {
        Account::new("AltOne", builtin::idle())
    }

    #[test]
    fn fresh_account_is_not_healthy_until_validated() {
        let mut a = account();
        a.two_step.enabled = true;
        let h = a.recompute_health(Utc::now()).clone();
        assert!(h.score < 100);
        assert!(h.issues.iter().any(|i| i.code == "no_profile"));
    }

    #[test]
    fn expired_login_drives_critical_health_and_fix_hint() {
        let mut a = account();
        a.two_step.enabled = true;
        a.roblox_user_id = Some(1);
        a.last_validated = Some(Utc::now());
        a.login = LoginState {
            kind: LoginStateKind::Expired,
            checked_at: Some(Utc::now()),
            detail: None,
        };
        let h = a.recompute_health(Utc::now()).clone();
        assert!(h.score <= 65, "score was {}", h.score);
        assert_eq!(h.issues[0].code, "session_expired");
        assert!(h.issues[0].suggested_fix.contains("browser sign-in"));
    }

    #[test]
    fn cookie_forecast_flags_expiring_and_expired() {
        let mut a = account();
        let now = Utc::now();
        a.cookie_expires_at = Some(now + Duration::days(3));
        assert!(matches!(a.cookie_health(now), CookieHealth::ExpiringSoon(3)));
        a.cookie_expires_at = Some(now - Duration::days(1));
        assert_eq!(a.cookie_health(now), CookieHealth::Expired);
        a.cookie_expires_at = None;
        assert_eq!(a.cookie_health(now), CookieHealth::Unknown);
    }

    #[test]
    fn duplicates_group_by_roblox_id_and_username() {
        let mut a = account();
        let mut b = account();
        b.username = "altone".into();
        let mut c = account();
        c.username = "other".into();
        a.roblox_user_id = Some(42);
        b.roblox_user_id = Some(42);
        let groups = find_duplicates(&[a, b, c]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn merge_keeps_union_of_metadata_and_newest_login() {
        let mut a = Account::new("primary", builtin::farming());
        a.roblox_user_id = Some(7);
        a.last_validated = Some(Utc::now() - Duration::days(2));
        a.tags.insert("old".into());
        let mut b = Account::new("dup", builtin::idle());
        b.roblox_user_id = Some(7);
        b.last_validated = Some(Utc::now());
        b.tags.insert("new".into());
        b.notes = "hello".into();
        b.cookie_fingerprint = Some("fp".into());
        merge_accounts(&mut a, &b);
        assert!(a.tags.contains("old") && a.tags.contains("new"));
        assert_eq!(a.cookie_fingerprint.as_deref(), Some("fp"));
        assert_eq!(a.notes, "hello");
    }

    #[test]
    fn filter_applies_tags_text_and_health() {
        let mut a = account();
        a.tags.insert("trade".into());
        a.robux = Some(500);
        a.health = HealthScore::from_score(50, vec![]);
        let f = AccountFilter {
            text: "alt".into(),
            tags: vec!["trade".into()],
            min_robux: Some(100),
            health_below: Some(60),
            ..Default::default()
        };
        assert!(f.matches(&a));
        let f2 = AccountFilter { min_robux: Some(600), ..f.clone() };
        assert!(!f2.matches(&a));
    }

    #[test]
    fn bulk_edit_composes_aliases_and_tags() {
        let mut a = account();
        let edit = BulkEdit {
            alias_prefix: Some("[F] ".into()),
            add_tags: ["farm".to_string()].into(),
            favourite: Some(true),
            ..Default::default()
        };
        edit.apply(&mut a);
        edit.apply(&mut a);
        assert_eq!(a.alias.as_deref(), Some("[F] [F] AltOne"));
        assert!(a.favourite);
    }
}
