use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created: Option<DateTime<Utc>>,
}

impl User {
    pub fn account_age_days(&self) -> Option<u32> {
        self.created.map(|c| (Utc::now() - c).num_days().max(0) as u32)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    #[serde(default, alias = "userId")]
    pub user_id: u64,
    #[serde(default, alias = "userPresenceType")]
    pub user_presence_type: u8,
    #[serde(default, alias = "placeId")]
    pub place_id: Option<u64>,
    #[serde(default, alias = "gameId")]
    pub game_id: Option<u64>,
    #[serde(default, alias = "gameInstanceId")]
    pub game_instance_id: Option<String>,
    #[serde(default, alias = "lastOnline")]
    pub last_online: Option<DateTime<Utc>>,
    #[serde(default, alias = "lastLocation")]
    pub last_location: String,
    #[serde(default, alias = "rootPlaceId")]
    pub root_place_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresenceResponse {
    #[serde(default, alias = "userPresences")]
    pub user_presences: Vec<Presence>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Currency {
    #[serde(default)]
    pub robux: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Friend {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub is_online: bool,
    #[serde(default)]
    pub presence_type: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FriendsResponse {
    #[serde(default)]
    pub data: Vec<Friend>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CountResponse {
    #[serde(default)]
    pub count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerInstance {
    #[serde(default, alias = "id")]
    pub job_id: String,
    #[serde(default, alias = "playing")]
    pub playing: u32,
    #[serde(default, alias = "maxPlayers")]
    pub max_players: u32,
    #[serde(default, alias = "ping")]
    pub ping: Option<u32>,
    #[serde(default, alias = "fps")]
    pub fps: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServersResponse {
    #[serde(default, alias = "data")]
    pub data: Vec<ServerInstance>,
    #[serde(default, alias = "nextPageCursor")]
    pub next_page_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UniverseIdResponse {
    #[serde(default)]
    pub universe_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThumbnailEntry {
    #[serde(default, alias = "targetId")]
    pub target_id: u64,
    #[serde(default)]
    pub state: String,
    #[serde(default, alias = "imageUrl")]
    pub image_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThumbnailResponse {
    #[serde(default)]
    pub data: Vec<ThumbnailEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InventoryEntry {
    #[serde(default, alias = "assetId")]
    pub asset_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "assetType")]
    pub asset_type: String,
    #[serde(default)]
    pub quantity: u32,
    #[serde(default, alias = "recentAveragePrice")]
    pub recent_average_price: Option<u64>,
    #[serde(default, alias = "originalPrice")]
    pub original_price: Option<u64>,
    #[serde(default)]
    pub limited: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InventoryResponse {
    #[serde(default)]
    pub data: Vec<InventoryEntry>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupRole {
    #[serde(default)]
    pub group: GroupInfo,
    #[serde(default)]
    pub role: RoleInfo,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupInfo {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub member_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoleInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rank: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupsResponse {
    #[serde(default)]
    pub data: Vec<GroupRole>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareLinkResolution {
    #[serde(default)]
    pub resolved_share_link_type: String,
    #[serde(default)]
    pub place_id: Option<u64>,
    #[serde(default)]
    pub game_instance_id: Option<String>,
    #[serde(default)]
    pub private_server_link_code: Option<String>,
    #[serde(default)]
    pub user_id: Option<u64>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthTicket {
    #[serde(default)]
    pub ticket: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PremiumInfo {
    #[serde(default, alias = "isPremium")]
    pub is_premium: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Badge {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub awarded_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BadgesResponse {
    #[serde(default)]
    pub data: Vec<Badge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub user: User,
    pub robux: Option<u64>,
    pub premium: bool,
    pub presence: Option<Presence>,
    pub follower_count: Option<u64>,
    pub friend_count: Option<u64>,
    pub avatar_url: Option<String>,
    pub age_days: Option<u32>,
    pub two_step: bool,
    pub two_step_known: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_age_is_derived() {
        let u = User { created: Some(Utc::now() - chrono::Duration::days(400)), ..Default::default() };
        assert_eq!(u.account_age_days(), Some(400));
        assert_eq!(User::default().account_age_days(), None);
    }

    #[test]
    fn parses_roblox_presence_payload() {
        let json = r#"{"userPresences":[{"userId":42,"userPresenceType":2,"placeId":1,"gameInstanceId":"abc","lastOnline":"2026-10-30T12:00:00Z","lastLocation":"Game"}]}"#;
        let parsed: PresenceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.user_presences.len(), 1);
        assert_eq!(parsed.user_presences[0].user_id, 42);
        assert_eq!(parsed.user_presences[0].user_presence_type, 2);
        assert!(parsed.user_presences[0].last_online.is_some());
    }

    #[test]
    fn server_payload_aliases_parse() {
        let json = r#"{"data":[{"id":"job-1","playing":5,"maxPlayers":20,"ping":42,"fps":59.9}],"nextPageCursor":"c2"}"#;
        let parsed: ServersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.data[0].job_id, "job-1");
        assert_eq!(parsed.data[0].playing, 5);
        assert_eq!(parsed.next_page_cursor.as_deref(), Some("c2"));
    }

    #[test]
    fn partial_responses_do_not_panic() {
        let r: Result<FriendsResponse, _> = serde_json::from_str("{}");
        assert!(r.is_ok());
        let thumbnails: ThumbnailResponse = serde_json::from_str(r#"{"data":[{"targetId":1,"state":"Completed","imageUrl":"https://tr.rbxcdn.com/x"}]}"#).unwrap();
        assert_eq!(thumbnails.data[0].target_id, 1);
    }
}
