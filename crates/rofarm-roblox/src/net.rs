use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use url::Url;

use crate::error::{RobloxError, RobloxResult};

pub const ROBLOX_HOSTS: &[&str] = &[
    "roblox.com",
    "www.roblox.com",
    "web.roblox.com",
    "users.roblox.com",
    "auth.roblox.com",
    "presence.roblox.com",
    "friends.roblox.com",
    "thumbnails.roblox.com",
    "economy.roblox.com",
    "economycreatorstats.roblox.com",
    "inventory.roblox.com",
    "groups.roblox.com",
    "games.roblox.com",
    "apis.roblox.com",
    "premiumfeatures.roblox.com",
    "catalog.roblox.com",
    "trades.roblox.com",
    "badges.roblox.com",
    "avatar.roblox.com",
    "accountsettings.roblox.com",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkLogEntry {
    pub at: DateTime<Utc>,
    pub method: String,
    pub host: String,
    pub path: String,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug)]
pub struct NetPolicy {
    extra_hosts: Vec<String>,
    log: Mutex<VecDeque<NetworkLogEntry>>,
    log_limit: usize,
    log_enabled: bool,
}

impl Default for NetPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl NetPolicy {
    pub fn new() -> Self {
        Self { extra_hosts: Vec::new(), log: Mutex::new(VecDeque::new()), log_limit: 1000, log_enabled: true }
    }

    pub fn with_extra_hosts(mut self, hosts: Vec<String>) -> Self {
        self.extra_hosts = hosts
            .into_iter()
            .map(|h| h.trim().trim_start_matches("*.").to_ascii_lowercase())
            .filter(|h| !h.is_empty() && !h.contains('/'))
            .collect();
        self
    }

    pub fn allowed_hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = ROBLOX_HOSTS.iter().map(|h| h.to_string()).collect();
        hosts.extend(self.extra_hosts.iter().cloned());
        hosts.push("updates.rofarm.example".into());
        hosts
    }

    pub fn with_log_limit(mut self, limit: usize, enabled: bool) -> Self {
        self.log_limit = limit.clamp(50, 10_000);
        self.log_enabled = enabled;
        self
    }

    pub fn is_allowed(&self, url: &Url) -> (bool, String) {
        if url.scheme() != "https" {
            return (false, "only HTTPS is allowed".into());
        }
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        let is_roblox = host == "roblox.com"
            || ROBLOX_HOSTS.contains(&host.as_str())
            || host.ends_with(".roblox.com")
            || host == "rbxcdn.com"
            || host.ends_with(".rbxcdn.com");
        if is_roblox {
            return (true, "roblox".into());
        }
        if self.extra_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))) {
            return (true, "user-configured notifier".into());
        }
        (false, format!("host {host} is not on the allow-list"))
    }

    pub fn record(&self, entry: NetworkLogEntry) {
        if !self.log_enabled {
            return;
        }
        let mut log = self.log.lock();
        log.push_back(entry);
        while log.len() > self.log_limit {
            log.pop_front();
        }
    }

    pub fn log(&self) -> Vec<NetworkLogEntry> {
        self.log.lock().iter().cloned().collect()
    }

    pub fn recent(&self, n: usize) -> Vec<NetworkLogEntry> {
        let log = self.log.lock();
        log.iter().rev().take(n).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }

    pub fn clear_log(&self) {
        self.log.lock().clear();
    }

    pub fn check(&self, url: &str) -> RobloxResult<Url> {
        let parsed = Url::parse(url).map_err(|_| RobloxError::Blocked("invalid URL".into()))?;
        let (allowed, reason) = self.is_allowed(&parsed);
        if !allowed {
            return Err(RobloxError::Blocked(reason));
        }
        Ok(parsed)
    }
}

pub fn safe_path(url: &Url) -> String {
    let mut path = url.path().to_string();
    if path.len() > 160 {
        path.truncate(160);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roblox_hosts_are_allowed_but_foreign_hosts_are_not() {
        let policy = NetPolicy::new();
        assert!(policy.check("https://users.roblox.com/v1/users/authenticated").is_ok());
        assert!(policy.check("https://tr.rbxcdn.com/avatar.png").is_ok());
        assert!(policy.check("https://www.roblox.com/games/1").is_ok());
        assert!(policy.check("https://evil.example/steal").is_err());
        assert!(policy.check("http://users.roblox.com/x").is_err());
        assert!(policy.check("https://roblox.com.evil.example/x").is_err());
    }

    #[test]
    fn explicit_notifier_hosts_only() {
        let policy = NetPolicy::new().with_extra_hosts(vec!["discord.com".into(), "*.pushover.net".into()]);
        assert!(policy.check("https://discord.com/api/webhooks/1/x").is_ok());
        assert!(policy.check("https://api.pushover.net/1/messages.json").is_ok());
        assert!(policy.check("https://hooks.slack.com/services/x").is_err());
    }

    #[test]
    fn log_is_bounded_and_clearable() {
        let policy = NetPolicy::new().with_log_limit(50, true);
        for i in 0..120 {
            policy.record(NetworkLogEntry {
                at: Utc::now(),
                method: "GET".into(),
                host: "users.roblox.com".into(),
                path: format!("/v1/{i}"),
                status: Some(200),
                duration_ms: 3,
                request_bytes: 0,
                response_bytes: 10,
                allowed: true,
                reason: "roblox".into(),
            });
        }
        assert_eq!(policy.log().len(), 50);
        assert_eq!(policy.recent(2).last().unwrap().path, "/v1/119");
        policy.clear_log();
        assert!(policy.log().is_empty());
    }
}
