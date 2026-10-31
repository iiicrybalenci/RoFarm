use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AccountId, LaneId, LaunchProfileId, LayoutId, SceneId};
use crate::lane::{AntiAfkConfig, ResourceLimits, ResourceProfile};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneAccount {
    pub account_id: AccountId,
    pub target: Option<String>,
    pub profile_id: Option<LaunchProfileId>,
    pub lane_id: LaneId,
    pub limits: Option<ResourceLimits>,
    pub auto_launch: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SceneSnapshot {
    pub accounts: Vec<SceneAccount>,
    pub layout: Option<LayoutId>,
    pub resource_profile: ResourceProfile,
    pub anti_afk: AntiAfkConfig,
    pub window_layout: Vec<crate::window::Rect>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub description: String,
    pub hotkey: Option<String>,
    pub snapshot: SceneSnapshot,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_applied: Option<DateTime<Utc>>,
    pub apply_count: u64,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: SceneId::new(),
            name: name.into(),
            description: String::new(),
            hotkey: None,
            snapshot: SceneSnapshot::default(),
            created_at: now,
            updated_at: now,
            last_applied: None,
            apply_count: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Scene needs a name".into());
        }
        if self.snapshot.accounts.len() > 500 {
            return Err("Scene has too many accounts".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.snapshot.accounts {
            if !seen.insert(a.account_id) {
                return Err("Scene contains the same account twice".into());
            }
            if let Some(target) = &a.target {
                crate::launch::parse_target(target).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        format!(
            "{} account{}, {} auto-launch",
            self.snapshot.accounts.len(),
            if self.snapshot.accounts.len() == 1 { "" } else { "s" },
            self.snapshot.accounts.iter().filter(|a| a.auto_launch).count()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SceneDiff {
    pub accounts_added: Vec<AccountId>,
    pub accounts_missing: Vec<AccountId>,
    pub lane_changes: Vec<(AccountId, LaneId)>,
    pub target_changes: Vec<AccountId>,
    pub profile_changes: Vec<AccountId>,
}

impl SceneDiff {
    pub fn is_empty(&self) -> bool {
        self.accounts_added.is_empty()
            && self.accounts_missing.is_empty()
            && self.lane_changes.is_empty()
            && self.target_changes.is_empty()
            && self.profile_changes.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "Scene already matches the fleet.".into();
        }
        let mut parts = Vec::new();
        if !self.accounts_added.is_empty() {
            parts.push(format!("{} missing account(s)", self.accounts_added.len()));
        }
        if !self.accounts_missing.is_empty() {
            parts.push(format!("{} account(s) not in scene", self.accounts_missing.len()));
        }
        if !self.lane_changes.is_empty() {
            parts.push(format!("{} lane change(s)", self.lane_changes.len()));
        }
        if !self.target_changes.is_empty() {
            parts.push(format!("{} target change(s)", self.target_changes.len()));
        }
        parts.join(", ")
    }
}

pub fn diff_scene(scene: &Scene, live_accounts: &[Account]) -> SceneDiff {
    use std::collections::{BTreeMap, BTreeSet};
    let scene_ids: BTreeSet<AccountId> = scene.snapshot.accounts.iter().map(|a| a.account_id).collect();
    let live: BTreeMap<AccountId, &Account> = live_accounts.iter().map(|a| (a.id, a)).collect();
    let mut diff = SceneDiff::default();
    for sa in &scene.snapshot.accounts {
        match live.get(&sa.account_id) {
            None => diff.accounts_added.push(sa.account_id),
            Some(account) => {
                if account.lane_id != sa.lane_id {
                    diff.lane_changes.push((sa.account_id, sa.lane_id));
                }
                if scene.snapshot.accounts.iter().any(|s| s.account_id == sa.account_id && s.target.is_some()) {
                    diff.target_changes.push(sa.account_id);
                }
                if sa.profile_id.is_some() {
                    diff.profile_changes.push(sa.account_id);
                }
            }
        }
    }
    for id in live.keys() {
        if !scene_ids.contains(id) {
            diff.accounts_missing.push(*id);
        }
    }
    diff
}

use crate::account::Account;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::builtin;

    #[test]
    fn scene_validation_rejects_duplicates_and_bad_targets() {
        let mut scene = Scene::new("Night farm");
        let mut account = Account::new("a", builtin::farming());
        scene.snapshot.accounts.push(SceneAccount {
            account_id: account.id,
            target: Some("123".into()),
            profile_id: None,
            lane_id: builtin::farming(),
            limits: None,
            auto_launch: true,
        });
        assert!(scene.validate().is_ok());
        scene.snapshot.accounts.push(scene.snapshot.accounts[0].clone());
        assert!(scene.validate().is_err());
        scene.snapshot.accounts.truncate(1);
        scene.snapshot.accounts[0].target = Some("https://evil.example/1".into());
        assert!(scene.validate().is_err());
        let _ = &mut account;
    }

    #[test]
    fn diff_reports_lane_and_missing_accounts() {
        let mut scene = Scene::new("s");
        let a = Account::new("a", builtin::farming());
        let b = Account::new("b", builtin::idle());
        scene.snapshot.accounts.push(SceneAccount {
            account_id: a.id,
            target: None,
            profile_id: None,
            lane_id: builtin::farming(),
            limits: None,
            auto_launch: true,
        });
        let mut moved = a.clone();
        moved.lane_id = builtin::testing();
        let diff = diff_scene(&scene, &[moved, b]);
        assert_eq!(diff.lane_changes.len(), 1);
        assert_eq!(diff.accounts_missing.len(), 1);
        assert!(!diff.is_empty());
        assert!(diff.summary().contains("lane change"));
    }

    #[test]
    fn summary_counts_auto_launch() {
        let mut scene = Scene::new("s");
        scene.snapshot.accounts.push(SceneAccount {
            account_id: AccountId::new(),
            target: None,
            profile_id: None,
            lane_id: builtin::idle(),
            limits: None,
            auto_launch: true,
        });
        assert!(scene.summary().contains("1 account"));
        assert!(scene.summary().contains("1 auto-launch"));
    }
}
