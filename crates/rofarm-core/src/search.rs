use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::fleet::FleetState;

pub fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().collect();
    let needle_lower: Vec<char> = needle.to_lowercase().chars().collect();
    let hay_lower: Vec<char> = haystack.to_lowercase().chars().collect();
    if hay_lower.len() < needle_lower.len() {
        return None;
    }
    let mut score = 0i64;
    let mut hi = 0usize;
    let mut last_match: Option<usize> = None;
    for (ni, nc) in needle_lower.iter().enumerate() {
        let mut found = None;
        while hi < hay_lower.len() {
            if hay_lower[hi] == *nc {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let idx = found?;
        if let Some(last) = last_match {
            if idx == last + 1 {
                score += 12;
            } else {
                score -= (idx - last - 1).min(10) as i64;
            }
        }
        let is_word_start = idx == 0
            || !hay[idx - 1].is_alphanumeric()
            || (hay[idx].is_uppercase() && hay[idx - 1].is_lowercase());
        if is_word_start {
            score += 8;
        }
        if ni == 0 {
            score += 4;
        }
        score += 2;
        last_match = Some(idx);
        hi = idx + 1;
    }
    score -= (hay_lower.len() as i64).min(40) / 4;
    if hay_lower.starts_with(needle_lower.as_slice()) {
        score += 20;
    }
    Some(score)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Account,
    Lane,
    Action,
    Setting,
    Game,
    Server,
    Note,
    Log,
    Doc,
    Automation,
    Scene,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub score: i64,
}

impl SearchHit {
    pub fn new(kind: SearchKind, id: impl Into<String>, title: impl Into<String>, subtitle: impl Into<String>, score: i64) -> Self {
        Self { kind, id: id.into(), title: title.into(), subtitle: subtitle.into(), score }
    }
}

const DOC_ENTRIES: &[(&str, &str, &str)] = &[
    ("account", "Accounts", "Add, validate, tag and organise accounts"),
    ("vault", "Vault & security", "Auto-lock, bindings, recovery, duress vault"),
    ("launch", "Launch profiles", "Targets, FPS caps, graphics presets, limits"),
    ("automation", "Automations", "Triggers, conditions, actions, dry-run"),
    ("scene", "Scenes", "Save and restore the entire fleet"),
    ("backup", "Backups", "Encrypted .rofarm backups and verification"),
    ("remote", "Remote dashboard", "Pairing, PIN, TOTP, TLS, firewall"),
    ("cli", "CLI & daemon", "rofarm commands and rofarmd"),
    ("plugin", "Plugins", "Sandboxed WASM plugins and permissions"),
    ("limits", "Resource limits", "CPU, memory, EcoQoS, GPU, QoS"),
    ("log", "Logs & diagnostics", "Log rotation, Doctor, diagnostics bundle"),
    ("privacy", "Privacy", "Network allow-list, no telemetry"),
    ("server", "Servers", "Server browser, bookmarks, private servers"),
    ("shortcut", "Keyboard shortcuts", "Rebind every action"),
];

pub fn global_search(state: &FleetState, query: &str, logs: &[String]) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let q = query.trim();
    if q.is_empty() {
        return hits;
    }
    let ql = q.to_ascii_lowercase();
    for a in state.accounts.values() {
        let hay = format!("{} {} {} {}", a.username, a.display_name, a.alias.clone().unwrap_or_default(), a.notes);
        if let Some(score) = fuzzy_score(&hay, q) {
            hits.push(SearchHit::new(
                SearchKind::Account,
                a.id.to_string(),
                a.display().to_string(),
                format!("@{} · {} · {}", a.username, a.login.kind.label(), a.tags.iter().cloned().collect::<Vec<_>>().join(", ")),
                score,
            ));
        } else if !ql.is_empty() && a.notes.to_ascii_lowercase().contains(&ql) {
            hits.push(SearchHit::new(SearchKind::Note, a.id.to_string(), a.display().to_string(), a.notes.chars().take(80).collect::<String>(), 10));
        }
    }
    for lane in state.lanes.values() {
        if let Some(score) = fuzzy_score(&lane.name, q) {
            hits.push(SearchHit::new(SearchKind::Lane, lane.id.to_string(), lane.name.clone(), "Lane", score));
        }
    }
    for game in state.games.values() {
        if let Some(score) = fuzzy_score(&format!("{} {}", game.name, game.tags.iter().cloned().collect::<Vec<_>>().join(" ")), q) {
            hits.push(SearchHit::new(SearchKind::Game, game.id.to_string(), game.name.clone(), format!("Place {}", game.place_id), score));
        }
    }
    for automation in state.automations.values() {
        if let Some(score) = fuzzy_score(&format!("{} {}", automation.name, automation.description), q) {
            hits.push(SearchHit::new(SearchKind::Automation, automation.id.to_string(), automation.name.clone(), "Automation", score));
        }
    }
    for scene in state.scenes.values() {
        if let Some(score) = fuzzy_score(&scene.name, q) {
            hits.push(SearchHit::new(SearchKind::Scene, scene.id.to_string(), scene.name.clone(), "Scene", score));
        }
    }
    for profile in state.profiles.values() {
        if let Some(score) = fuzzy_score(&format!("{} {}", profile.name, profile.target), q) {
            hits.push(SearchHit::new(SearchKind::Profile, profile.id.to_string(), profile.name.clone(), profile.target.clone(), score));
        }
    }
    for (key, title, subtitle) in DOC_ENTRIES {
        if let Some(score) = fuzzy_score(&format!("{title} {subtitle} {key}"), q) {
            hits.push(SearchHit::new(SearchKind::Doc, *key, *title, *subtitle, score.saturating_sub(4)));
        }
    }
    for (idx, line) in logs.iter().enumerate() {
        if let Some(score) = fuzzy_score(line, q) {
            hits.push(SearchHit::new(
                SearchKind::Log,
                idx.to_string(),
                line.chars().take(120).collect::<String>(),
                "Log",
                score.saturating_sub(20),
            ));
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    let mut seen = BTreeSet::new();
    hits.retain(|h| seen.insert((h.kind, h.id.clone())));
    hits.truncate(80);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequence_with_bonuses() {
        assert!(fuzzy_score("Command Palette", "cp").unwrap() > 0);
        assert!(fuzzy_score("Command Palette", "cpx").is_none());
        let prefix = fuzzy_score("launch profile", "launch").unwrap();
        let acronym = fuzzy_score("launch profile", "lpf").unwrap();
        assert!(prefix > acronym);
        assert!(fuzzy_score("force restart", "restart").unwrap() > fuzzy_score("force restart", "frst").unwrap());
        assert_eq!(fuzzy_score("anything", ""), Some(0));
        assert!(fuzzy_score("short", "longer").is_none());
    }

    #[test]
    fn global_search_finds_accounts_and_docs() {
        let mut state = FleetState::default();
        let mut a = crate::account::Account::new("FarmingAlpha", crate::ids::builtin::farming());
        a.notes = "needs a new cookie soon".into();
        state.accounts.insert(a.id, a);
        let hits = global_search(&state, "farm", &[]);
        assert!(hits.iter().any(|h| h.kind == SearchKind::Account));
        let docs = global_search(&state, "vault", &[]);
        assert!(docs.iter().any(|h| h.kind == SearchKind::Doc));
        let logs = vec!["client Beta disconnected from server".to_string()];
        let hit = global_search(&state, "disconnected", &logs);
        assert!(hit.iter().any(|h| h.kind == SearchKind::Log));
    }

    #[test]
    fn results_are_deduped_and_bounded() {
        let state = FleetState::default();
        let hits = global_search(&state, "account", &[]);
        assert!(hits.len() <= 80);
        let mut seen = std::collections::BTreeSet::new();
        for h in &hits {
            assert!(seen.insert((h.kind, h.id.clone())));
        }
    }
}
