use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

use crate::error::{CoreError, Result};
use crate::ids::{AccountId, LaunchProfileId};
use crate::lane::{GraphicsPreset, ResourceLimits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Place,
    PlaceWithServer,
    PrivateServer,
    ShareCode,
    Profile,
    RawScheme,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchTarget {
    pub raw: String,
    pub kind: TargetKind,
    pub place_id: Option<u64>,
    pub game_instance_id: Option<String>,
    pub private_server_code: Option<String>,
    pub share_code: Option<String>,
    pub user_id: Option<u64>,
    pub username: Option<String>,
    pub confidence: f32,
    pub warnings: Vec<String>,
    pub needs_resolution: bool,
}

impl LaunchTarget {
    pub fn summary(&self) -> String {
        match self.kind {
            TargetKind::Place => format!(
                "Place {} on any public server",
                self.place_id.map(|p| p.to_string()).unwrap_or_default()
            ),
            TargetKind::PlaceWithServer => format!(
                "Place {} on server {}",
                self.place_id.unwrap_or_default(),
                self.game_instance_id.as_deref().unwrap_or("?")
            ),
            TargetKind::PrivateServer => format!(
                "Private server of place {}",
                self.place_id.map(|p| p.to_string()).unwrap_or_default()
            ),
            TargetKind::ShareCode => "Roblox share link (resolved after sign-in)".into(),
            TargetKind::Profile => format!(
                "Profile @{} (pick a place to launch)",
                self.username.as_deref().unwrap_or("?")
            ),
            TargetKind::RawScheme => "Roblox launch URI".into(),
        }
    }

    pub fn launchable(&self) -> bool {
        matches!(
            self.kind,
            TargetKind::Place | TargetKind::PlaceWithServer | TargetKind::PrivateServer | TargetKind::RawScheme
        ) || (self.kind == TargetKind::ShareCode && self.share_code.is_some())
            || (self.kind == TargetKind::Profile && self.user_id.is_some())
    }

    pub fn preview(&self) -> TargetPreview {
        TargetPreview {
            summary: self.summary(),
            kind: self.kind,
            place_id: self.place_id,
            job_id: self.game_instance_id.clone(),
            warnings: self.warnings.clone(),
            launchable: self.launchable(),
            resume_same_server: self.game_instance_id.is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetPreview {
    pub summary: String,
    pub kind: TargetKind,
    pub place_id: Option<u64>,
    pub job_id: Option<String>,
    pub warnings: Vec<String>,
    pub launchable: bool,
    pub resume_same_server: bool,
}

fn allowed_host(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "roblox.com" | "www.roblox.com" | "web.roblox.com" | "m.roblox.com" | "rbxcdn.com"
    )
}

fn parse_id(s: &str) -> Option<u64> {
    s.trim().parse::<u64>().ok().filter(|n| *n > 0)
}

pub fn parse_target(input: &str) -> Result<LaunchTarget> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(CoreError::Invalid("Enter a Roblox game link, place ID, or @username.".into()));
    }
    let lower = raw.to_ascii_lowercase();

    if lower.starts_with("roblox-player:") || lower.starts_with("roblox://") {
        let mut warnings = Vec::new();
        let mut place_id = None;
        let mut job = None;
        if let Some(query) = raw.split('?').nth(1) {
            for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                match k.as_ref() {
                    "placeId" => place_id = parse_id(&v),
                    "gameInstanceId" => job = Some(v.to_string()),
                    _ => {}
                }
            }
        }
        if place_id.is_none() {
            warnings.push("No place ID found in the URI; it will be used as provided.".into());
        }
        return Ok(LaunchTarget {
            raw: raw.to_string(),
            kind: TargetKind::RawScheme,
            place_id,
            game_instance_id: job,
            private_server_code: None,
            share_code: None,
            user_id: None,
            username: None,
            confidence: 0.8,
            warnings,
            needs_resolution: false,
        });
    }

    if raw.starts_with('@') {
        let name = raw.trim_start_matches('@').trim();
        validate_username(name)?;
        return Ok(LaunchTarget {
            raw: raw.into(),
            kind: TargetKind::Profile,
            place_id: None,
            game_instance_id: None,
            private_server_code: None,
            share_code: None,
            user_id: None,
            username: Some(name.to_string()),
            confidence: 0.9,
            warnings: vec!["Profiles have no place to join; choose a game before launching.".into()],
            needs_resolution: true,
        });
    }

    if let Ok(url) = Url::parse(raw) {
        if !allowed_host(url.host_str().unwrap_or("")) {
            return Err(CoreError::Invalid(
                "Only roblox.com and rbxcdn.com links are accepted.".into(),
            ));
        }
        let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
        let mut query: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in url.query_pairs() {
            query.insert(k.into_owned(), v.into_owned());
        }

        if url.path() == "/share" {
            let code = query
                .get("code")
                .cloned()
                .ok_or_else(|| CoreError::Invalid("Share link has no code.".into()))?;
            return Ok(LaunchTarget {
                raw: raw.into(),
                kind: TargetKind::ShareCode,
                place_id: None,
                game_instance_id: None,
                private_server_code: None,
                share_code: Some(code),
                user_id: None,
                username: None,
                confidence: 0.8,
                warnings: vec!["Share codes are resolved by Roblox after sign-in.".into()],
                needs_resolution: true,
            });
        }

        if segments.first() == Some(&"games") || segments.first() == Some(&"experiences") {
            let is_start = segments.get(1) == Some(&"start");
            let place_id = if is_start {
                query.get("placeId").and_then(|v| parse_id(v))
            } else {
                segments.get(1).and_then(|v| parse_id(v))
            };
            let place_id = place_id
                .ok_or_else(|| CoreError::Invalid("Could not find a place ID in that link.".into()))?;
            let job = query.get("gameInstanceId").cloned();
            let private = query.get("privateServerLinkCode").cloned();
            let mut warnings = Vec::new();
            if job.is_none() && private.is_none() {
                warnings.push("No server specified: Roblox will pick a public server.".into());
            }
            let kind = if private.is_some() {
                TargetKind::PrivateServer
            } else if job.is_some() {
                TargetKind::PlaceWithServer
            } else {
                TargetKind::Place
            };
            return Ok(LaunchTarget {
                raw: raw.into(),
                kind,
                place_id: Some(place_id),
                game_instance_id: job,
                private_server_code: private,
                share_code: None,
                user_id: None,
                username: None,
                confidence: 0.97,
                warnings,
                needs_resolution: false,
            });
        }

        if segments.first() == Some(&"users") {
            let mut username = query.get("username").cloned();
            let mut user_id = None;
            if let Some(seg) = segments.get(1) {
                if *seg == "profile" {
                } else {
                    user_id = parse_id(seg);
                    if segments.get(2) == Some(&"profile") || user_id.is_some() {
                    }
                }
            }
            if username.is_none() && user_id.is_none() {
                username = query.get("username").cloned();
            }
            if username.is_some() || user_id.is_some() {
                return Ok(LaunchTarget {
                    raw: raw.into(),
                    kind: TargetKind::Profile,
                    place_id: None,
                    game_instance_id: None,
                    private_server_code: None,
                    share_code: None,
                    user_id,
                    username,
                    confidence: 0.85,
                    warnings: vec!["Profiles have no place to join; choose a game before launching.".into()],
                    needs_resolution: true,
                });
            }
        }

        return Err(CoreError::Invalid(
            "That roblox.com link is not a game, private server, or profile.".into(),
        ));
    }

    if lower.contains("placeid=") || lower.contains("gameinstanceid=") {
        let mut place_id = None;
        let mut job = None;
        for (k, v) in url::form_urlencoded::parse(raw.trim_start_matches('?').as_bytes()) {
            if k.eq_ignore_ascii_case("placeId") {
                place_id = parse_id(&v);
            } else if k.eq_ignore_ascii_case("gameInstanceId") {
                job = Some(v.to_string());
            }
        }
        if let Some(place_id) = place_id {
            return Ok(LaunchTarget {
                raw: raw.into(),
                kind: if job.is_some() { TargetKind::PlaceWithServer } else { TargetKind::Place },
                place_id: Some(place_id),
                game_instance_id: job,
                private_server_code: None,
                share_code: None,
                user_id: None,
                username: None,
                confidence: 0.9,
                warnings: vec![],
                needs_resolution: false,
            });
        }
    }

    if let Some((place, job)) = raw.split_once(['/', ' ']) {
        if let (Some(place_id), true) = (parse_id(place), uuid_like(job.trim())) {
            return Ok(LaunchTarget {
                raw: raw.into(),
                kind: TargetKind::PlaceWithServer,
                place_id: Some(place_id),
                game_instance_id: Some(job.trim().to_string()),
                private_server_code: None,
                share_code: None,
                user_id: None,
                username: None,
                confidence: 0.75,
                warnings: vec!["Assumed '<placeId> <jobId>' format.".into()],
                needs_resolution: false,
            });
        }
    }

    if let Some(place_id) = parse_id(raw) {
        return Ok(LaunchTarget {
            raw: raw.into(),
            kind: TargetKind::Place,
            place_id: Some(place_id),
            game_instance_id: None,
            private_server_code: None,
            share_code: None,
            user_id: None,
            username: None,
            confidence: 0.9,
            warnings: vec!["No server specified: Roblox will pick a public server.".into()],
            needs_resolution: false,
        });
    }

    if looks_like_username(raw) {
        return Ok(LaunchTarget {
            raw: raw.into(),
            kind: TargetKind::Profile,
            place_id: None,
            game_instance_id: None,
            private_server_code: None,
            share_code: None,
            user_id: None,
            username: Some(raw.to_string()),
            confidence: 0.6,
            warnings: vec!["No game link: resolved as a username.".into()],
            needs_resolution: true,
        });
    }

    Err(CoreError::Invalid(
        "Unrecognized target. Use a game link, place ID, server link, or @username.".into(),
    ))
}

fn looks_like_username(s: &str) -> bool {
    let len = s.chars().count();
    (3..=20).contains(&len)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.chars().any(|c| c.is_ascii_alphabetic())
}

fn validate_username(s: &str) -> Result<()> {
    if looks_like_username(s) {
        Ok(())
    } else {
        Err(CoreError::Invalid("That username is not valid Roblox format.".into()))
    }
}

fn uuid_like(s: &str) -> bool {
    s.len() >= 8 && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServerMode {
    #[default]
    Any,
    SameServer,
    FollowFriend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_backoff_secs: u32,
    pub max_backoff_secs: u32,
    pub retry_on_disconnect: bool,
    pub retry_on_crash: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff_secs: 20,
            max_backoff_secs: 600,
            retry_on_disconnect: true,
            retry_on_crash: true,
        }
    }
}

impl RetryPolicy {
    pub fn backoff_secs(&self, attempt: u8) -> u32 {
        let exp = self.base_backoff_secs.saturating_mul(1u32 << attempt.min(10).saturating_sub(1));
        exp.min(self.max_backoff_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchProfile {
    pub id: LaunchProfileId,
    pub name: String,
    pub target: String,
    pub args: String,
    pub fps_cap: Option<u16>,
    pub graphics: GraphicsPreset,
    pub limits: ResourceLimits,
    pub window: crate::window::WindowPlacement,
    pub server_mode: ServerMode,
    pub follow_username: Option<String>,
    pub stagger_ms: u64,
    pub cooldown_ms: u64,
    pub retry: RetryPolicy,
    pub anti_afk_override: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LaunchProfile {
    pub fn new(name: impl Into<String>, target: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: LaunchProfileId::new(),
            name: name.into(),
            target: target.into(),
            args: String::new(),
            fps_cap: Some(30),
            graphics: GraphicsPreset::Low,
            limits: ResourceLimits::default(),
            window: crate::window::WindowPlacement::default(),
            server_mode: ServerMode::Any,
            follow_username: None,
            stagger_ms: 2500,
            cooldown_ms: 8000,
            retry: RetryPolicy::default(),
            anti_afk_override: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(CoreError::Invalid("Launch profile needs a name.".into()));
        }
        self.limits.validate().map_err(CoreError::Invalid)?;
        parse_target(&self.target)?;
        if self.server_mode == ServerMode::FollowFriend
            && self.follow_username.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(CoreError::Invalid("Follow-friend mode needs a username.".into()));
        }
        if self.stagger_ms > 120_000 || self.cooldown_ms > 600_000 {
            return Err(CoreError::Invalid("Stagger or cooldown is too large.".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClientSettings {
    pub fps_cap: Option<u16>,
    pub graphics: GraphicsPreset,
    pub no_grass: bool,
    pub gray_sky: bool,
    pub msaa: Option<u8>,
    pub texture_quality: Option<u8>,
    pub rendering_mode: Option<u8>,
}

impl ClientSettings {
    pub fn from_profile(profile: &LaunchProfile) -> Self {
        Self {
            fps_cap: profile.fps_cap,
            graphics: profile.graphics,
            no_grass: matches!(profile.graphics, GraphicsPreset::Lowest | GraphicsPreset::Low),
            gray_sky: profile.graphics == GraphicsPreset::Lowest,
            msaa: if profile.graphics == GraphicsPreset::Lowest { Some(0) } else { None },
            texture_quality: match profile.graphics {
                GraphicsPreset::Lowest => Some(1),
                GraphicsPreset::Low => Some(3),
                GraphicsPreset::Balanced => Some(6),
                GraphicsPreset::High | GraphicsPreset::Custom => None,
            },
            rendering_mode: if profile.graphics == GraphicsPreset::Lowest { Some(1) } else { None },
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(fps) = self.fps_cap {
            map.insert("DFIntTaskSchedulerTargetFps".into(), serde_json::json!(fps));
        }
        if let Some(q) = self.texture_quality {
            map.insert("DFIntDebugTextureQualityOverride".into(), serde_json::json!(q));
        }
        if let Some(m) = self.msaa {
            map.insert("DFIntDebugMSAALevel".into(), serde_json::json!(m));
        }
        if let Some(mode) = self.rendering_mode {
            map.insert("DFIntDebugRenderingMode".into(), serde_json::json!(mode));
            map.insert("FFlagDebugUseGeometryMuchFaster".into(), serde_json::json!(true));
        }
        map.insert("FFlagDebugSkyGray".into(), serde_json::json!(self.gray_sky));
        map.insert("FFlagDebugDisableGrass".into(), serde_json::json!(self.no_grass));
        map.insert("FFlagDebugPauseVoxelizer".into(), serde_json::json!(self.graphics == GraphicsPreset::Lowest));
        serde_json::Value::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchSlot {
    pub account_id: AccountId,
    pub offset_ms: u64,
    pub wave: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchPlan {
    pub profile_id: LaunchProfileId,
    pub slots: Vec<LaunchSlot>,
    pub total_ms: u64,
    pub same_server: bool,
    pub warnings: Vec<String>,
}

pub fn plan_batch(
    accounts: &[AccountId],
    profile: &LaunchProfile,
    lane_of: impl Fn(AccountId) -> Option<crate::ids::LaneId>,
    max_running_by_lane: &BTreeMap<crate::ids::LaneId, u32>,
) -> BatchPlan {
    let mut warnings = Vec::new();
    if profile.server_mode == ServerMode::SameServer {
        warnings.push(
            "Same-server mode joins the first successful server; later clients may wait for capacity."
                .into(),
        );
    }
    let mut per_lane: BTreeMap<crate::ids::LaneId, u32> = BTreeMap::new();
    let mut slots = Vec::new();
    let mut offset = 0u64;
    let mut wave = 0u32;
    for account in accounts {
        if let Some(lane) = lane_of(*account) {
            let cap = max_running_by_lane.get(&lane).copied().unwrap_or(u32::MAX);
            let used = per_lane.entry(lane).or_insert(0);
            if cap != u32::MAX && *used >= cap {
                wave += 1;
                offset = (wave as u64) * profile.cooldown_ms.max(profile.stagger_ms);
                per_lane.clear();
            }
            *per_lane.entry(lane).or_insert(0) += 1;
        }
        slots.push(LaunchSlot { account_id: *account, offset_ms: offset, wave });
        offset += profile.stagger_ms;
    }
    let total_ms = slots.iter().map(|s| s.offset_ms).max().unwrap_or(0) + profile.stagger_ms;
    BatchPlan { profile_id: profile.id, slots, total_ms, same_server: profile.server_mode == ServerMode::SameServer, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{builtin, LaneId};

    #[test]
    fn parses_game_links_with_slug_and_query() {
        let t = parse_target("https://www.roblox.com/games/4483381587/Guess-the-Number").unwrap();
        assert_eq!(t.kind, TargetKind::Place);
        assert_eq!(t.place_id, Some(4483381587));
        assert!(t.launchable());

        let t = parse_target(
            "https://www.roblox.com/games/start?placeId=920587237&gameInstanceId=abcDEF123456",
        )
        .unwrap();
        assert_eq!(t.kind, TargetKind::PlaceWithServer);
        assert_eq!(t.game_instance_id.as_deref(), Some("abcDEF123456"));

        let t = parse_target(
            "https://www.roblox.com/games/4483381587/Name?privateServerLinkCode=ABCDEF123456",
        )
        .unwrap();
        assert_eq!(t.kind, TargetKind::PrivateServer);
        assert_eq!(t.private_server_code.as_deref(), Some("ABCDEF123456"));
    }

    #[test]
    fn parses_share_links_and_profiles() {
        let t = parse_target("https://www.roblox.com/share?code=abc123&type=Server").unwrap();
        assert_eq!(t.kind, TargetKind::ShareCode);
        assert_eq!(t.share_code.as_deref(), Some("abc123"));
        assert!(t.needs_resolution);

        let t = parse_target("https://www.roblox.com/users/1/profile").unwrap();
        assert_eq!(t.kind, TargetKind::Profile);
        assert_eq!(t.user_id, Some(1));

        let t = parse_target("@Builderman").unwrap();
        assert_eq!(t.username.as_deref(), Some("Builderman"));
        assert!(!t.launchable());
    }

    #[test]
    fn parses_place_ids_and_raw_scheme() {
        let t = parse_target("4483381587").unwrap();
        assert_eq!(t.place_id, Some(4483381587));
        let t = parse_target("placeId=123&gameInstanceId=deadbeef").unwrap();
        assert_eq!(t.game_instance_id.as_deref(), Some("deadbeef"));
        let t = parse_target("roblox-player:1+launchmode:play+gameinfo:ticket+placelauncherurl:https%3A%2F%2Fwww.roblox.com%2Fgames%2F1").unwrap();
        assert_eq!(t.kind, TargetKind::RawScheme);
    }

    #[test]
    fn rejects_foreign_hosts_and_garbage() {
        assert!(parse_target("https://evil.example/games/1").is_err());
        assert!(parse_target("not a target!!!").is_err());
        assert!(parse_target("https://www.roblox.com/upgrades").is_err());
        assert!(parse_target("").is_err());
    }

    #[test]
    fn client_settings_include_fps_and_lowest_flags() {
        let profile = LaunchProfile::new("farm", "123");
        let settings = ClientSettings::from_profile(&profile);
        let json = settings.to_json();
        assert_eq!(json["DFIntTaskSchedulerTargetFps"], serde_json::json!(30));
        assert_eq!(json["FFlagDebugSkyGray"], serde_json::json!(false));
        let mut p = profile.clone();
        p.graphics = GraphicsPreset::Lowest;
        p.fps_cap = Some(15);
        let json = ClientSettings::from_profile(&p).to_json();
        assert_eq!(json["DFIntTaskSchedulerTargetFps"], serde_json::json!(15));
        assert_eq!(json["FFlagDebugSkyGray"], serde_json::json!(true));
        assert_eq!(json["FFlagDebugDisableGrass"], serde_json::json!(true));
    }

    #[test]
    fn batch_plan_staggers_and_respects_lane_caps() {
        let accounts: Vec<AccountId> = (0..5).map(|_| AccountId::new()).collect();
        let mut profile = LaunchProfile::new("p", "1");
        profile.stagger_ms = 1000;
        profile.cooldown_ms = 5000;
        let lane: LaneId = builtin::farming();
        let mut caps = BTreeMap::new();
        caps.insert(lane, 2);
        let plan = plan_batch(&accounts, &profile, |_| Some(lane), &caps);
        assert_eq!(plan.slots.len(), 5);
        assert_eq!(plan.slots[0].offset_ms, 0);
        assert_eq!(plan.slots[1].offset_ms, 1000);
        assert_eq!(plan.slots[2].wave, 1);
        assert!(plan.slots[2].offset_ms >= 5000);
        assert_eq!(plan.slots.len(), 5);
    }

    #[test]
    fn retry_backoff_is_exponential_and_capped() {
        let policy = RetryPolicy { base_backoff_secs: 10, max_backoff_secs: 60, ..Default::default() };
        assert_eq!(policy.backoff_secs(1), 10);
        assert_eq!(policy.backoff_secs(2), 20);
        assert_eq!(policy.backoff_secs(3), 40);
        assert_eq!(policy.backoff_secs(4), 60);
        assert_eq!(policy.backoff_secs(9), 60);
    }

    #[test]
    fn profile_validation_catches_missing_follow_user() {
        let mut p = LaunchProfile::new("x", "123");
        p.server_mode = ServerMode::FollowFriend;
        assert!(p.validate().is_err());
        p.follow_username = Some("friend".into());
        assert!(p.validate().is_ok());
    }
}
