use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::account::Account;
use crate::error::{CoreError, Result};
use crate::settings::ThemeDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportSource {
    Usernames,
    Csv,
    RobloxAccountManager,
    RoFarmBackup,
    Link,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedAccount {
    pub username: String,
    pub roblox_user_id: Option<u64>,
    pub alias: Option<String>,
    pub notes: String,
    pub tags: BTreeSet<String>,
    pub has_cookie: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportError {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub source: ImportSource,
    pub accounts: Vec<ImportedAccount>,
    pub errors: Vec<ImportError>,
    pub duplicates: Vec<String>,
    pub skipped: u32,
}

impl ImportPreview {
    pub fn valid_count(&self) -> usize {
        self.accounts.len()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} account(s), {} duplicate(s), {} error(s)",
            self.accounts.len(),
            self.duplicates.len(),
            self.errors.len()
        )
    }

    pub fn against(&self, existing: &[Account]) -> ImportPreview {
        let names: BTreeSet<String> = existing.iter().map(|a| a.username.to_ascii_lowercase()).collect();
        let mut filtered = self.clone();
        filtered.accounts.retain(|a| {
            let dup = names.contains(&a.username.to_ascii_lowercase());
            if dup {
                filtered.duplicates.push(a.username.clone());
            }
            !dup
        });
        filtered
    }
}

fn valid_username(name: &str) -> bool {
    let n = name.trim();
    (1..=32).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn parse_usernames(text: &str) -> ImportPreview {
    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ',');
        let username = parts.next().unwrap_or("").trim().trim_start_matches('@').to_string();
        if !valid_username(&username) {
            errors.push(ImportError { line: idx + 1, message: "Invalid Roblox username.".into() });
            continue;
        }
        if !seen.insert(username.to_ascii_lowercase()) {
            duplicates.push(username);
            continue;
        }
        let tags = parts
            .next()
            .map(|t| t.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        let notes = parts.next().unwrap_or("").trim().to_string();
        accounts.push(ImportedAccount {
            username,
            roblox_user_id: None,
            alias: None,
            notes,
            tags,
            has_cookie: false,
        });
    }
    ImportPreview { source: ImportSource::Usernames, accounts, errors, duplicates, skipped: 0 }
}

pub fn parse_ram_export(json: &str) -> Result<ImportPreview> {
    #[derive(Deserialize)]
    struct RamAccount {
        #[serde(default, alias = "Username")]
        username: String,
        #[serde(default, alias = "Alias")]
        alias: String,
        #[serde(default, alias = "UserId", alias = "UserID")]
        user_id: Option<u64>,
        #[serde(default, alias = "Note")]
        note: String,
        #[serde(default, alias = "Group")]
        group: String,
        #[serde(default, alias = "Tags")]
        tags: Vec<String>,
        #[serde(default, alias = "Description")]
        description: String,
    }
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| CoreError::Invalid("Not valid JSON.".into()))?;
    let list = value
        .get("Accounts")
        .or_else(|| value.get("accounts"))
        .cloned()
        .unwrap_or(value);
    let raw: Vec<RamAccount> = serde_json::from_value(list).map_err(|_| CoreError::Invalid("Unrecognized RAM export shape.".into()))?;
    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (idx, entry) in raw.into_iter().enumerate() {
        if !valid_username(&entry.username) {
            errors.push(ImportError { line: idx + 1, message: "Invalid username.".into() });
            continue;
        }
        if !seen.insert(entry.username.to_ascii_lowercase()) {
            duplicates.push(entry.username);
            continue;
        }
        let mut tags: BTreeSet<String> = entry.tags.into_iter().filter(|t| !t.trim().is_empty()).collect();
        if !entry.group.trim().is_empty() {
            tags.insert(entry.group.trim().to_string());
        }
        let notes = [entry.note.trim(), entry.description.trim()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
        accounts.push(ImportedAccount {
            username: entry.username,
            roblox_user_id: entry.user_id,
            alias: if entry.alias.trim().is_empty() { None } else { Some(entry.alias.trim().to_string()) },
            notes,
            tags,
            has_cookie: false,
        });
    }
    Ok(ImportPreview { source: ImportSource::RobloxAccountManager, accounts, errors, duplicates, skipped: 0 })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleKind {
    Theme,
    ThemePack,
    LaunchProfiles,
    Automations,
    Scenes,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareBundle<T> {
    pub schema_version: u32,
    pub kind: BundleKind,
    pub name: String,
    pub author: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub items: Vec<T>,
    pub notes: String,
}

impl<T: Serialize> ShareBundle<T> {
    pub fn new(kind: BundleKind, name: impl Into<String>, items: Vec<T>) -> Self {
        Self {
            schema_version: 1,
            kind,
            name: name.into(),
            author: "RoFarm user".into(),
            created_at: chrono::Utc::now(),
            items,
            notes: String::new(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl<T: for<'de> Deserialize<'de>> ShareBundle<T> {
    pub fn from_json(json: &str, expected: BundleKind) -> Result<Self> {
        let bundle: Self = serde_json::from_str(json).map_err(|_| CoreError::Invalid("Bundle JSON is invalid.".into()))?;
        if bundle.schema_version != 1 {
            return Err(CoreError::Invalid("Unsupported bundle version.".into()));
        }
        if bundle.kind != expected {
            return Err(CoreError::Invalid(format!("Expected a {:?} bundle.", expected)));
        }
        if bundle.items.len() > 500 {
            return Err(CoreError::Invalid("Bundle is too large.".into()));
        }
        Ok(bundle)
    }
}

pub fn merge_theme_packs(existing: &[ThemeDefinition], incoming: Vec<ThemeDefinition>) -> Result<Vec<ThemeDefinition>> {
    let mut by_id: BTreeMap<String, ThemeDefinition> = existing.iter().cloned().map(|t| (t.id.clone(), t)).collect();
    let mut out: Vec<ThemeDefinition> = Vec::new();
    for theme in incoming {
        theme.validate()?;
        by_id.insert(theme.id.clone(), theme);
    }
    out.extend(by_id.into_values());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::builtin;

    #[test]
    fn parses_usernames_with_tags_and_notes() {
        let preview = parse_usernames("# header\nAlpha, farm;main, primary account\nBeta\nbad name\nalpha\n");
        assert_eq!(preview.accounts.len(), 2);
        assert_eq!(preview.accounts[0].tags.len(), 2);
        assert_eq!(preview.accounts[0].notes, "primary account");
        assert_eq!(preview.errors.len(), 1);
        assert_eq!(preview.duplicates, vec!["alpha"]);
    }

    #[test]
    fn ram_import_never_reads_cookies() {
        let json = r#"{"Accounts":[
            {"Username":"Alpha","Alias":"Farm 1","UserId":42,"Note":"hi","Group":"Main","Tags":["x"],"Cookie":"SECRET_SHOULD_NOT_APPEAR"},
            {"Username":"Alpha"}
        ]}"#;
        let preview = parse_ram_export(json).unwrap();
        assert_eq!(preview.accounts.len(), 1);
        assert_eq!(preview.accounts[0].roblox_user_id, Some(42));
        assert!(preview.accounts[0].tags.contains("Main"));
        assert!(!preview.accounts[0].has_cookie);
        assert_eq!(preview.duplicates, vec!["Alpha"]);
        let debug = format!("{preview:?}");
        assert!(!debug.contains("SECRET_SHOULD_NOT_APPEAR"));
    }

    #[test]
    fn preview_filters_existing_accounts() {
        let preview = parse_usernames("Alpha\nBeta\n");
        let mut existing = Account::new("alpha", builtin::farming());
        existing.username = "Alpha".into();
        let filtered = preview.against(&[existing]);
        assert_eq!(filtered.accounts.len(), 1);
        assert_eq!(filtered.accounts[0].username, "Beta");
        assert_eq!(filtered.duplicates, vec!["Alpha"]);
    }

    #[test]
    fn share_bundle_round_trip_and_type_check() {
        let pack = vec![ThemeDefinition::default()];
        let bundle = ShareBundle::new(BundleKind::ThemePack, "My pack", pack.clone());
        let json = bundle.to_json().unwrap();
        let parsed: ShareBundle<ThemeDefinition> = ShareBundle::from_json(&json, BundleKind::ThemePack).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert!(ShareBundle::<ThemeDefinition>::from_json(&json, BundleKind::Scenes).is_err());
    }

    #[test]
    fn theme_pack_merge_is_deduplicated() {
        let mut a = ThemeDefinition::default();
        a.id = "x".into();
        a.name = "X".into();
        for k in ["bg", "panel", "panel-raised", "text", "text-muted", "accent", "accent-contrast", "border", "danger", "warning", "success", "in-game", "idle", "failed"] {
            a.colors.insert(k.into(), "#111111".into());
        }
        let mut b = a.clone();
        b.name = "X2".into();
        let merged = merge_theme_packs(&[a.clone()], vec![b]).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "X2");
    }
}
