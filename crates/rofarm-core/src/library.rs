use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::ids::{AccountId, GameId, PrivateServerId, ServerBookmarkId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub asset_id: u64,
    pub name: String,
    pub item_type: String,
    pub limited: bool,
    pub quantity: u32,
    pub purchase_price: Option<u64>,
    pub recent_average_price: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl InventoryItem {
    pub fn estimated_value(&self) -> u64 {
        self.recent_average_price.or(self.purchase_price).unwrap_or(0) * self.quantity as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Purchase,
    Sale,
    TradeIn,
    TradeOut,
    RobuxGrant,
    Payout,
    GroupPayout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub at: DateTime<Utc>,
    pub kind: TransactionKind,
    pub item_name: Option<String>,
    pub robux_delta: i64,
    pub balance_after: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetWorthPoint {
    pub at: DateTime<Utc>,
    pub robux: u64,
    pub limited_value: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AccountEconomy {
    pub account_id: AccountId,
    pub robux: u64,
    pub pending_robux: u64,
    pub inventory: Vec<InventoryItem>,
    pub transactions: Vec<Transaction>,
    pub history: Vec<NetWorthPoint>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl AccountEconomy {
    pub fn limited_value(&self) -> u64 {
        self.inventory.iter().map(|i| i.estimated_value()).sum()
    }

    pub fn net_worth(&self) -> u64 {
        self.robux + self.limited_value()
    }

    pub fn record(&mut self, at: DateTime<Utc>) {
        self.history.push(NetWorthPoint {
            at,
            robux: self.robux,
            limited_value: self.limited_value(),
            total: self.net_worth(),
        });
        self.history.sort_by_key(|p| p.at);
        if self.history.len() > 2000 {
            let excess = self.history.len() - 2000;
            self.history.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FleetEconomy {
    pub accounts: BTreeMap<AccountId, AccountEconomy>,
}

impl FleetEconomy {
    pub fn total_robux(&self) -> u64 {
        self.accounts.values().map(|a| a.robux).sum()
    }

    pub fn total_limited_value(&self) -> u64 {
        self.accounts.values().map(|a| a.limited_value()).sum()
    }

    pub fn total_net_worth(&self) -> u64 {
        self.total_robux() + self.total_limited_value()
    }

    pub fn to_csv(&self) -> String {
        let mut out = String::from("account_id,robux,pending_robux,limited_value,net_worth,items,updated_at\n");
        for (id, e) in &self.accounts {
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                id,
                e.robux,
                e.pending_robux,
                e.limited_value(),
                e.net_worth(),
                e.inventory.len(),
                e.updated_at.map(|d| d.to_rfc3339()).unwrap_or_default()
            ));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendEntry {
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
    pub presence: crate::account::Presence,
    pub mutual: bool,
    pub pending_incoming: bool,
    pub pending_outgoing: bool,
    pub source_accounts: BTreeSet<AccountId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FriendsSnapshot {
    pub by_user: BTreeMap<u64, FriendEntry>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl FriendsSnapshot {
    pub fn merge_account(&mut self, account_id: AccountId, friends: impl IntoIterator<Item = FriendEntry>) {
        for mut f in friends {
            f.source_accounts.insert(account_id);
            match self.by_user.get_mut(&f.user_id) {
                Some(existing) => {
                    existing.source_accounts.insert(account_id);
                    existing.mutual = existing.mutual || f.mutual;
                    existing.presence = f.presence;
                    existing.pending_incoming |= f.pending_incoming;
                    existing.pending_outgoing |= f.pending_outgoing;
                }
                None => {
                    self.by_user.insert(f.user_id, f);
                }
            }
        }
        self.updated_at = Some(Utc::now());
    }

    pub fn mutuals(&self) -> Vec<&FriendEntry> {
        self.by_user.values().filter(|f| f.mutual || f.source_accounts.len() > 1).collect()
    }

    pub fn search(&self, query: &str) -> Vec<&FriendEntry> {
        let q = query.to_ascii_lowercase();
        let mut hits: Vec<&FriendEntry> = self
            .by_user
            .values()
            .filter(|f| {
                q.is_empty()
                    || f.username.to_ascii_lowercase().contains(&q)
                    || f.display_name.to_ascii_lowercase().contains(&q)
            })
            .collect();
        hits.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
        hits
    }

    pub fn pending_incoming(&self) -> Vec<&FriendEntry> {
        self.by_user.values().filter(|f| f.pending_incoming).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupMembership {
    pub group_id: u64,
    pub name: String,
    pub role: String,
    pub rank: u8,
    pub member_count: u32,
    pub funds_visible: bool,
    pub group_funds: Option<u64>,
    pub owner: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GroupOverview {
    pub by_account: BTreeMap<AccountId, Vec<GroupMembership>>,
}

impl GroupOverview {
    pub fn groups_with_fund_access(&self) -> Vec<(AccountId, &GroupMembership)> {
        self.by_account
            .iter()
            .flat_map(|(id, groups)| groups.iter().filter(|g| g.funds_visible || g.owner).map(move |g| (*id, g)))
            .collect()
    }

    pub fn distinct_groups(&self) -> usize {
        let set: BTreeSet<u64> = self.by_account.values().flatten().map(|g| g.group_id).collect();
        set.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitAsset {
    pub asset_id: u64,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outfit {
    pub id: String,
    pub name: String,
    pub assets: Vec<OutfitAsset>,
    pub created_at: DateTime<Utc>,
    pub source_account: Option<AccountId>,
}

impl Outfit {
    pub fn summary(&self) -> String {
        format!("{} ({} assets)", self.name, self.assets.len())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameEntry {
    pub id: GameId,
    pub universe_id: Option<u64>,
    pub place_id: u64,
    pub name: String,
    pub notes: String,
    pub tags: BTreeSet<String>,
    pub thumbnail_url: Option<String>,
    pub default_profile: Option<crate::ids::LaunchProfileId>,
    pub playtime_seconds: u64,
    pub last_played: Option<DateTime<Utc>>,
    pub tracked_updates: bool,
    pub added_at: DateTime<Utc>,
}

impl GameEntry {
    pub fn new(place_id: u64, name: impl Into<String>) -> Self {
        Self {
            id: GameId::new(),
            universe_id: None,
            place_id,
            name: name.into(),
            notes: String::new(),
            tags: BTreeSet::new(),
            thumbnail_url: None,
            default_profile: None,
            playtime_seconds: 0,
            last_played: None,
            tracked_updates: false,
            added_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlaytimeRecord {
    pub account_id: Option<AccountId>,
    pub game_id: Option<GameId>,
    pub lane_id: Option<crate::ids::LaneId>,
    pub seconds: u64,
    pub sessions: u32,
    pub crashes: u32,
    pub last_session: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub job_id: String,
    pub place_id: u64,
    pub players: u32,
    pub max_players: u32,
    pub ping_ms: Option<u32>,
    pub region: String,
    pub version: String,
    pub private_server: bool,
    pub is_favourite: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub fps_average: Option<u16>,
}

impl ServerInfo {
    pub fn fill_ratio(&self) -> f32 {
        if self.max_players == 0 {
            0.0
        } else {
            self.players as f32 / self.max_players as f32
        }
    }

    pub fn health(&self) -> ServerHealth {
        if self.players == 0 {
            return ServerHealth::Empty;
        }
        match self.ping_ms {
            Some(p) if p > 180 => ServerHealth::Poor,
            Some(p) if p > 90 => ServerHealth::Fair,
            Some(_) => {
                if self.fill_ratio() > 0.9 {
                    ServerHealth::Full
                } else {
                    ServerHealth::Good
                }
            }
            None => ServerHealth::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerHealth {
    Good,
    Fair,
    Poor,
    Full,
    Empty,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ServerFilter {
    pub region: Option<String>,
    pub max_ping_ms: Option<u32>,
    pub min_players: Option<u32>,
    pub max_players: Option<u32>,
    pub version: Option<String>,
    pub include_private: bool,
    pub favourites_only: bool,
}

impl ServerFilter {
    pub fn matches(&self, s: &ServerInfo) -> bool {
        if !self.include_private && s.private_server {
            return false;
        }
        if let Some(region) = &self.region {
            if !s.region.eq_ignore_ascii_case(region) {
                return false;
            }
        }
        if let Some(max_ping) = self.max_ping_ms {
            if s.ping_ms.unwrap_or(u32::MAX) > max_ping {
                return false;
            }
        }
        if let Some(min) = self.min_players {
            if s.players < min {
                return false;
            }
        }
        if let Some(max) = self.max_players {
            if s.players > max {
                return false;
            }
        }
        if let Some(version) = &self.version {
            if &s.version != version {
                return false;
            }
        }
        if self.favourites_only && !s.is_favourite {
            return false;
        }
        true
    }

    pub fn apply(&self, servers: &[ServerInfo]) -> Vec<ServerInfo> {
        servers.iter().filter(|s| self.matches(s)).cloned().collect()
    }

    pub fn best_low_population(&self, servers: &[ServerInfo]) -> Option<ServerInfo> {
        self.apply(servers)
            .into_iter()
            .filter(|s| s.players > 0 && s.players < s.max_players)
            .min_by_key(|s| {
                let crowd = (s.fill_ratio() * 100.0) as u32;
                s.ping_ms.unwrap_or(500) + crowd * 2
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerBookmark {
    pub id: ServerBookmarkId,
    pub name: String,
    pub server: ServerInfo,
    pub bookmarked_at: DateTime<Utc>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivateServerLink {
    pub id: PrivateServerId,
    pub place_id: u64,
    pub name: String,
    pub url: String,
    pub code: String,
    pub note: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub access_checked: Option<DateTime<Utc>>,
    pub accessible_accounts: Vec<AccountId>,
    pub created_at: DateTime<Utc>,
}

impl PrivateServerLink {
    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| e < now)
    }

    pub fn days_until_expiry(&self, now: DateTime<Utc>) -> Option<i64> {
        self.expires_at.map(|e| (e - now).num_days())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeEntry {
    pub badge_id: u64,
    pub name: String,
    pub game_name: String,
    pub awarded_at: Option<DateTime<Utc>>,
    pub icon_url: Option<String>,
    pub rarity_percent: Option<f32>,
}

pub fn badge_summary(badges: &[BadgeEntry]) -> (usize, usize) {
    let earned = badges.iter().filter(|b| b.awarded_at.is_some()).count();
    (earned, badges.len())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn inventory_csv(account: &AccountEconomy) -> String {
    let mut out = String::from("asset_id,name,type,limited,quantity,purchase_price,recent_average_price,estimated_value\n");
    for item in &account.inventory {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            item.asset_id,
            csv_escape(&item.name),
            csv_escape(&item.item_type),
            item.limited,
            item.quantity,
            item.purchase_price.map(|v| v.to_string()).unwrap_or_default(),
            item.recent_average_price.map(|v| v.to_string()).unwrap_or_default(),
            item.estimated_value()
        ));
    }
    out
}

pub fn daily_playtime(records: &[PlaytimeRecord]) -> BTreeMap<NaiveDate, u64> {
    let mut out = BTreeMap::new();
    for r in records {
        if let Some(last) = r.last_session {
            *out.entry(last.date_naive()).or_insert(0) += r.seconds;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Presence;

    fn server(players: u32, ping: u32) -> ServerInfo {
        ServerInfo {
            job_id: format!("job-{players}-{ping}"),
            place_id: 1,
            players,
            max_players: 20,
            ping_ms: Some(ping),
            region: "eu".into(),
            version: "v1".into(),
            private_server: false,
            is_favourite: false,
            last_seen: None,
            fps_average: Some(60),
        }
    }

    #[test]
    fn economy_totals_and_csv() {
        let mut fleet = FleetEconomy::default();
        let id = AccountId::new();
        let mut e = AccountEconomy { account_id: id, robux: 100, ..Default::default() };
        e.inventory.push(InventoryItem {
            asset_id: 1,
            name: "Limited \"Hat\", Blue".into(),
            item_type: "Hat".into(),
            limited: true,
            quantity: 2,
            purchase_price: Some(10),
            recent_average_price: Some(50),
            updated_at: None,
        });
        assert_eq!(e.limited_value(), 100);
        assert_eq!(e.net_worth(), 200);
        e.record(Utc::now());
        assert_eq!(e.history.len(), 1);
        fleet.accounts.insert(id, e);
        assert_eq!(fleet.total_net_worth(), 200);
        let csv = fleet.to_csv();
        assert!(csv.starts_with("account_id"));
        assert!(csv.contains(",100,0,100,200,1,"));
        let inv = inventory_csv(&fleet.accounts[&id]);
        assert!(inv.contains("\"Limited \"\"Hat\"\", Blue\""));
    }

    #[test]
    fn friends_merge_and_find_mutuals() {
        let mut snap = FriendsSnapshot::default();
        let a = AccountId::new();
        let b = AccountId::new();
        let f = |name: &str, id: u64| FriendEntry {
            user_id: id,
            username: name.into(),
            display_name: name.into(),
            presence: Presence::Online,
            mutual: false,
            pending_incoming: false,
            pending_outgoing: false,
            source_accounts: BTreeSet::new(),
        };
        snap.merge_account(a, [f("alice", 1), f("bob", 2)]);
        snap.merge_account(b, [f("alice", 1)]);
        assert_eq!(snap.by_user.len(), 2);
        assert_eq!(snap.mutuals().len(), 1);
        assert_eq!(snap.search("ali").len(), 1);
        assert_eq!(snap.by_user[&1].source_accounts.len(), 2);
    }

    #[test]
    fn server_filter_and_best_low_pop() {
        let servers = vec![server(10, 40), server(18, 30), server(2, 120), server(0, 20)];
        let filter = ServerFilter { max_ping_ms: Some(100), ..Default::default() };
        let matched = filter.apply(&servers);
        assert_eq!(matched.len(), 3);
        let best = filter.best_low_population(&servers).unwrap();
        assert_eq!(best.players, 10);
        let full = ServerFilter { min_players: Some(19), ..Default::default() };
        assert!(full.best_low_population(&servers).is_none());
    }

    #[test]
    fn server_health_buckets() {
        assert_eq!(server(5, 30).health(), ServerHealth::Good);
        assert_eq!(server(5, 100).health(), ServerHealth::Fair);
        assert_eq!(server(5, 250).health(), ServerHealth::Poor);
        assert_eq!(server(20, 30).health(), ServerHealth::Full);
        assert_eq!(server(0, 30).health(), ServerHealth::Empty);
    }

    #[test]
    fn private_server_expiry() {
        let now = Utc::now();
        let link = PrivateServerLink {
            id: PrivateServerId::new(),
            place_id: 1,
            name: "base".into(),
            url: "https://www.roblox.com/share?code=x".into(),
            code: "x".into(),
            note: String::new(),
            expires_at: Some(now + chrono::Duration::days(3)),
            access_checked: None,
            accessible_accounts: vec![],
            created_at: now,
        };
        assert!(!link.expired(now));
        assert_eq!(link.days_until_expiry(now), Some(3));
        assert!(link.expired(now + chrono::Duration::days(4)));
    }

    #[test]
    fn group_overview_counts_distinct_groups() {
        let mut overview = GroupOverview::default();
        let g = GroupMembership {
            group_id: 7,
            name: "Farm".into(),
            role: "Member".into(),
            rank: 1,
            member_count: 10,
            funds_visible: false,
            group_funds: None,
            owner: false,
            updated_at: None,
        };
        overview.by_account.insert(AccountId::new(), vec![g.clone()]);
        overview.by_account.insert(AccountId::new(), vec![g]);
        assert_eq!(overview.distinct_groups(), 1);
        assert!(overview.groups_with_fund_access().is_empty());
    }

    #[test]
    fn csv_escaping_is_quote_aware() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("quote\"x"), "\"quote\"\"x\"");
        assert_eq!(csv_escape("plain"), "plain");
    }
}
