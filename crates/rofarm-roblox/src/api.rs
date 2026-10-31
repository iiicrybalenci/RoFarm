use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

use crate::error::{RobloxError, RobloxResult};
use crate::net::{safe_path, NetPolicy, NetworkLogEntry};
use crate::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl ApiRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self { method: "GET".into(), url: url.into(), headers: Vec::new(), body: None }
    }
    pub fn post(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            method: "POST".into(),
            url: url.into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: Some(body.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl ApiResponse {
    pub fn ok(body: impl Into<String>) -> Self {
        Self { status: 200, headers: BTreeMap::new(), body: body.into() }
    }
    pub fn json(body: impl Into<String>) -> Self {
        let mut headers = BTreeMap::new();
        headers.insert("content-type".into(), "application/json".into());
        Self { status: 200, headers, body: body.into() }
    }
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(&self, request: ApiRequest) -> RobloxResult<ApiResponse>;
}

#[derive(Clone)]
pub struct SessionCookie(SecretString);

impl std::fmt::Debug for SessionCookie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionCookie([redacted])")
    }
}

impl SessionCookie {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let value = value.trim();
        let value = value
            .strip_prefix(".ROBLOSECURITY=")
            .or_else(|| value.strip_prefix("ROBLOSECURITY="))
            .unwrap_or(value)
            .trim()
            .trim_matches('"')
            .to_string();
        Self(SecretString::from(value))
    }

    fn header(&self) -> String {
        format!(".ROBLOSECURITY={}", self.0.expose_secret())
    }

    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

pub struct LiveTransport {
    client: reqwest::Client,
    policy: Arc<NetPolicy>,
}

impl std::fmt::Debug for LiveTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveTransport")
    }
}

impl LiveTransport {
    pub fn new(policy: Arc<NetPolicy>) -> RobloxResult<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("RoFarm/", env!("CARGO_PKG_VERSION"), " (Windows; local-first)"))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25))
            .build()
            .map_err(|_| RobloxError::Transport)?;
        Ok(Self { client, policy })
    }
}

#[async_trait]
impl HttpTransport for LiveTransport {
    async fn send(&self, request: ApiRequest) -> RobloxResult<ApiResponse> {
        let started = Instant::now();
        let url = Url::parse(&request.url).map_err(|_| RobloxError::Blocked("invalid URL".into()))?;
        let (allowed, reason) = self.policy.is_allowed(&url);
        if !allowed {
            self.policy.record(NetworkLogEntry {
                at: chrono::Utc::now(),
                method: request.method.clone(),
                host: url.host_str().unwrap_or_default().into(),
                path: safe_path(&url),
                status: None,
                duration_ms: started.elapsed().as_millis() as u64,
                request_bytes: request.body.as_ref().map(|b| b.len() as u64).unwrap_or(0),
                response_bytes: 0,
                allowed: false,
                reason: reason.clone(),
            });
            return Err(RobloxError::Blocked(reason));
        }
        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| RobloxError::Invalid("method".into()))?;
        let mut builder = self.client.request(method, url.clone());
        for (key, value) in &request.headers {
            builder = builder.header(key, value);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let response = builder.send().await.map_err(|_| RobloxError::Transport)?;
        let status = response.status().as_u16();
        let headers: BTreeMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_ascii_lowercase(), v.to_string())))
            .collect();
        let bytes = response.bytes().await.map_err(|_| RobloxError::Transport)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(RobloxError::Decode);
        }
        self.policy.record(NetworkLogEntry {
            at: chrono::Utc::now(),
            method: request.method.clone(),
            host: url.host_str().unwrap_or_default().into(),
            path: safe_path(&url),
            status: Some(status),
            duration_ms: started.elapsed().as_millis() as u64,
            request_bytes: request.body.as_ref().map(|b| b.len() as u64).unwrap_or(0),
            response_bytes: bytes.len() as u64,
            allowed: true,
            reason,
        });
        let body = String::from_utf8_lossy(&bytes).to_string();
        Ok(ApiResponse { status, headers, body })
    }
}

pub struct RobloxClient {
    transport: Arc<dyn HttpTransport>,
    policy: Arc<NetPolicy>,
}

impl std::fmt::Debug for RobloxClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RobloxClient")
    }
}

impl RobloxClient {
    pub fn live(policy: Arc<NetPolicy>) -> RobloxResult<Self> {
        let transport = Arc::new(LiveTransport::new(policy.clone())?);
        Ok(Self { transport, policy })
    }

    pub fn with_transport(transport: Arc<dyn HttpTransport>, policy: Arc<NetPolicy>) -> Self {
        Self { transport, policy }
    }

    pub fn policy(&self) -> &Arc<NetPolicy> {
        &self.policy
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str, cookie: &SessionCookie) -> RobloxResult<T> {
        let mut req = ApiRequest::get(url);
        req.headers.push(("cookie".into(), cookie.header()));
        req.headers.push(("accept".into(), "application/json".into()));
        let resp = self.send_checked(req).await?;
        serde_json::from_str(&resp.body).map_err(|_| RobloxError::Decode)
    }

    async fn send_checked(&self, req: ApiRequest) -> RobloxResult<ApiResponse> {
        let resp = self.transport.send(req).await?;
        check_response(resp)
    }

    async fn post_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        cookie: &SessionCookie,
        body: &str,
        referer: Option<&str>,
    ) -> RobloxResult<T> {
        let mut req = ApiRequest::post(url, body);
        req.headers.push(("cookie".into(), cookie.header()));
        req.headers.push(("accept".into(), "application/json".into()));
        if let Some(referer) = referer {
            req.headers.push(("referer".into(), referer.to_string()));
        }
        let resp = self.transport.send(req).await?;
        if resp.status == 403 {
            if let Some(token) = resp.headers.get("x-csrf-token") {
                let mut retry = ApiRequest::post(url, body);
                retry.headers.push(("cookie".into(), cookie.header()));
                retry.headers.push(("accept".into(), "application/json".into()));
                retry.headers.push(("x-csrf-token".into(), token.clone()));
                if let Some(referer) = referer {
                    retry.headers.push(("referer".into(), referer.to_string()));
                }
                let resp = self.send_checked(retry).await?;
                return serde_json::from_str(&resp.body).map_err(|_| RobloxError::Decode);
            }
        }
        let resp = check_response(resp)?;
        serde_json::from_str(&resp.body).map_err(|_| RobloxError::Decode)
    }

    async fn get_bool(&self, url: &str, cookie: &SessionCookie) -> RobloxResult<bool> {
        self.get(url, cookie).await
    }
}

fn check_response(resp: ApiResponse) -> RobloxResult<ApiResponse> {
    match resp.status {
        200..=299 => Ok(resp),
        401 | 403 => Err(RobloxError::Unauthorized),
        404 => Err(RobloxError::NotFound),
        429 => {
            let retry = resp.headers.get("retry-after").and_then(|v| v.parse::<u64>().ok()).unwrap_or(5);
            Err(RobloxError::RateLimited { retry_after_secs: retry })
        }
        status => Err(RobloxError::Http { status }),
    }
}

#[async_trait]
pub trait RobloxApi: Send + Sync {
    async fn authenticated_user(&self, cookie: &SessionCookie) -> RobloxResult<AuthenticatedUser>;
    async fn user_profile(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<User>;
    async fn validate_cookie(&self, cookie: &SessionCookie) -> RobloxResult<ValidationResult>;
    async fn presence(&self, cookie: &SessionCookie, user_ids: &[u64]) -> RobloxResult<Vec<Presence>>;
    async fn friends(&self, cookie: &SessionCookie, user_id: u64, cursor: Option<String>) -> RobloxResult<FriendsResponse>;
    async fn servers(&self, cookie: &SessionCookie, place_id: u64, cursor: Option<String>) -> RobloxResult<ServersResponse>;
    async fn inventory(&self, cookie: &SessionCookie, user_id: u64, asset_type: u32) -> RobloxResult<InventoryResponse>;
    async fn groups(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<GroupsResponse>;
    async fn resolve_share_link(&self, cookie: &SessionCookie, code: &str) -> RobloxResult<ShareLinkResolution>;
    async fn auth_ticket(&self, cookie: &SessionCookie, place_id: u64) -> RobloxResult<String>;
    async fn username_to_id(&self, cookie: &SessionCookie, username: &str) -> RobloxResult<Option<u64>>;
    async fn avatar_url(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<Option<String>>;
}

#[async_trait]
impl RobloxApi for RobloxClient {
    async fn authenticated_user(&self, cookie: &SessionCookie) -> RobloxResult<AuthenticatedUser> {
        self.get("https://users.roblox.com/v1/users/authenticated", cookie).await
    }

    async fn user_profile(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<User> {
        self.get(&format!("https://users.roblox.com/v1/users/{user_id}"), cookie).await
    }

    async fn presence(&self, cookie: &SessionCookie, user_ids: &[u64]) -> RobloxResult<Vec<Presence>> {
        let body = serde_json::json!({ "userIds": user_ids }).to_string();
        let resp: PresenceResponse = self
            .post_json("https://presence.roblox.com/v1/presence/users", cookie, &body, Some("https://www.roblox.com/"))
            .await?;
        Ok(resp.user_presences)
    }

    async fn validate_cookie(&self, cookie: &SessionCookie) -> RobloxResult<ValidationResult> {
        let auth = self.authenticated_user(cookie).await?;
        if auth.id == 0 {
            return Err(RobloxError::Unauthorized);
        }
        let profile = self.user_profile(cookie, auth.id).await.ok();
        let robux = self
            .get::<Currency>(&format!("https://economy.roblox.com/v1/users/{}/currency", auth.id), cookie)
            .await
            .ok()
            .map(|c| c.robux);
        let premium = self
            .get_bool(&format!("https://premiumfeatures.roblox.com/v1/users/{}/validate-membership", auth.id), cookie)
            .await
            .unwrap_or(false);
        let presence = self.presence(cookie, &[auth.id]).await.ok().and_then(|mut p| p.pop());
        let follower_count = self
            .get::<CountResponse>(&format!("https://friends.roblox.com/v1/users/{}/followers/count", auth.id), cookie)
            .await
            .ok()
            .map(|c| c.count);
        let friend_count = self
            .get::<CountResponse>(&format!("https://friends.roblox.com/v1/users/{}/friends/count", auth.id), cookie)
            .await
            .ok()
            .map(|c| c.count);
        let avatar_url = self.avatar_url(cookie, auth.id).await.ok().flatten();
        let age_days = profile.as_ref().and_then(|p| p.account_age_days());
        let user = profile.unwrap_or(User {
            id: auth.id,
            name: auth.name.clone(),
            display_name: auth.display_name.clone(),
            ..Default::default()
        });
        Ok(ValidationResult {
            user,
            robux,
            premium,
            presence,
            follower_count,
            friend_count,
            avatar_url,
            two_step: false,
            two_step_known: false,
            age_days,
        })
    }

    async fn friends(&self, cookie: &SessionCookie, user_id: u64, cursor: Option<String>) -> RobloxResult<FriendsResponse> {
        let mut url = format!("https://friends.roblox.com/v1/users/{user_id}/friends?limit=100");
        if let Some(cursor) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding(&cursor));
        }
        self.get(&url, cookie).await
    }

    async fn servers(&self, cookie: &SessionCookie, place_id: u64, cursor: Option<String>) -> RobloxResult<ServersResponse> {
        let mut url = format!("https://games.roblox.com/v1/games/{place_id}/servers/Public?limit=100&excludeFullGames=false");
        if let Some(cursor) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding(&cursor));
        }
        self.get(&url, cookie).await
    }

    async fn inventory(&self, cookie: &SessionCookie, user_id: u64, asset_type: u32) -> RobloxResult<InventoryResponse> {
        self.get(
            &format!("https://inventory.roblox.com/v2/users/{user_id}/inventory/{asset_type}?limit=100&sortOrder=Asc"),
            cookie,
        )
        .await
    }

    async fn groups(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<GroupsResponse> {
        self.get(&format!("https://groups.roblox.com/v2/users/{user_id}/groups/roles"), cookie).await
    }

    async fn resolve_share_link(&self, cookie: &SessionCookie, code: &str) -> RobloxResult<ShareLinkResolution> {
        let url = format!(
            "https://apis.roblox.com/sharelinks/v1/resolve-link?linkId={}&linkType=Server",
            urlencoding(code)
        );
        self.get(&url, cookie).await
    }

    async fn auth_ticket(&self, cookie: &SessionCookie, place_id: u64) -> RobloxResult<String> {
        let referer = format!("https://www.roblox.com/games/{place_id}/");
        let url = format!("https://auth.roblox.com/v1/authentication-ticket?url={}", urlencoding(&referer));
        let ticket: AuthTicket = self.post_json(&url, cookie, "{}", Some(&referer)).await?;
        if ticket.ticket.is_empty() {
            return Err(RobloxError::Unauthorized);
        }
        Ok(ticket.ticket)
    }

    async fn username_to_id(&self, cookie: &SessionCookie, username: &str) -> RobloxResult<Option<u64>> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            data: Vec<AuthenticatedUser>,
        }
        let body = serde_json::json!({ "usernames": [username], "excludeBannedUsers": false }).to_string();
        let resp: Resp = self
            .post_json("https://users.roblox.com/v1/usernames/users", cookie, &body, Some("https://www.roblox.com/"))
            .await?;
        Ok(resp.data.first().map(|u| u.id))
    }

    async fn avatar_url(&self, cookie: &SessionCookie, user_id: u64) -> RobloxResult<Option<String>> {
        let url = format!(
            "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={user_id}&size=150x150&format=Png&isCircular=false"
        );
        let resp: ThumbnailResponse = self.get(&url, cookie).await?;
        Ok(resp.data.into_iter().find(|t| t.state == "Completed").map(|t| t.image_url))
    }
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub fn server_region_placeholder() -> &'static str {
    "auto"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;
    use crate::launch::build_launch_uri;

    fn cc(policy: Arc<NetPolicy>, transport: MockTransport) -> RobloxClient {
        let transport = transport.with_policy(policy.clone());
        RobloxClient::with_transport(Arc::new(transport), policy)
    }

    #[test]
    fn cookie_header_is_built_and_debug_redacted() {
        let c = SessionCookie::new("  _|WARNING:-DO-NOT-SHARE|_abc  ");
        assert_eq!(c.expose(), "_|WARNING:-DO-NOT-SHARE|_abc");
        assert!(!format!("{c:?}").contains("DO-NOT-SHARE"));
        let d = SessionCookie::new(".ROBLOSECURITY=CA123");
        assert_eq!(d.expose(), "CA123");
    }

    #[tokio::test]
    async fn validates_cookie_end_to_end_against_mock() {
        let policy = Arc::new(NetPolicy::new());
        let transport = MockTransport::default()
            .with_json("https://users.roblox.com/v1/users/authenticated", r#"{"id":42,"name":"Tester","displayName":"Tester"}"#)
            .with_json("https://users.roblox.com/v1/users/42", r#"{"id":42,"name":"Tester","displayName":"Tester","created":"2020-01-01T00:00:00Z"}"#)
            .with_json("https://economy.roblox.com/v1/users/42/currency", r#"{"robux":1234}"#)
            .with_json("https://premiumfeatures.roblox.com/v1/users/42/validate-membership", "true")
            .with_post_json("https://presence.roblox.com/v1/presence/users", r#"{"userPresences":[{"userId":42,"userPresenceType":2,"placeId":7}]}"#)
            .with_json("https://friends.roblox.com/v1/users/42/followers/count", r#"{"count":11}"#)
            .with_json("https://friends.roblox.com/v1/users/42/friends/count", r#"{"count":5}"#)
            .with_json("https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds=42&size=150x150&format=Png&isCircular=false", r#"{"data":[{"targetId":42,"state":"Completed","imageUrl":"https://tr.rbxcdn.com/a.png"}]}"#);
        let client = cc(policy.clone(), transport);
        let result = client.validate_cookie(&SessionCookie::new("cookie")).await.unwrap();
        assert_eq!(result.user.name, "Tester");
        assert_eq!(result.robux, Some(1234));
        assert!(result.premium);
        assert_eq!(result.presence.unwrap().user_presence_type, 2);
        assert_eq!(result.follower_count, Some(11));
        assert!(!result.two_step_known);
        assert!(policy.log().len() >= 8);
        assert!(policy.log().iter().all(|e| e.allowed));
    }

    #[tokio::test]
    async fn unauthorized_and_rate_limit_map_to_typed_errors() {
        let policy = Arc::new(NetPolicy::new());
        let transport = MockTransport::default()
            .with_status("https://users.roblox.com/v1/users/authenticated", 401, "{}");
        let client = cc(policy, transport);
        assert!(matches!(
            client.authenticated_user(&SessionCookie::new("bad")).await,
            Err(RobloxError::Unauthorized)
        ));
    }

    #[tokio::test]
    async fn auth_ticket_retries_with_csrf_token() {
        let policy = Arc::new(NetPolicy::new());
        let mut csrf = BTreeMap::new();
        csrf.insert("x-csrf-token".to_string(), "token-value".to_string());
        let transport = MockTransport::default()
            .with_response("https://auth.roblox.com/v1/authentication-ticket?url=https%3A%2F%2Fwww.roblox.com%2Fgames%2F123%2F", 403, "{}", csrf)
            .with_post_json("https://auth.roblox.com/v1/authentication-ticket?url=https%3A%2F%2Fwww.roblox.com%2Fgames%2F123%2F", r#"{"ticket":"ticket-abc"}"#);
        let client = cc(policy, transport);
        let ticket = client.auth_ticket(&SessionCookie::new("cookie"), 123).await.unwrap();
        assert_eq!(ticket, "ticket-abc");
        let uri = build_launch_uri(&ticket, 123, None, None, 1_700_000_000_000);
        assert!(uri.starts_with("roblox-player:1+launchmode:play+gameinfo:ticket-abc"));
    }

    #[tokio::test]
    async fn share_link_and_servers_parse() {
        let policy = Arc::new(NetPolicy::new());
        let transport = MockTransport::default()
            .with_json(
                "https://apis.roblox.com/sharelinks/v1/resolve-link?linkId=abc&linkType=Server",
                r#"{"resolvedShareLinkType":"Server","placeId":99,"gameInstanceId":"job-1","privateServerLinkCode":null}"#,
            )
            .with_json(
                "https://games.roblox.com/v1/games/99/servers/Public?limit=100&excludeFullGames=false",
                r#"{"data":[{"id":"job-1","playing":4,"maxPlayers":10,"ping":55}],"nextPageCursor":null}"#,
            );
        let client = cc(policy, transport);
        let resolved = client.resolve_share_link(&SessionCookie::new("c"), "abc").await.unwrap();
        assert_eq!(resolved.place_id, Some(99));
        let servers = client.servers(&SessionCookie::new("c"), 99, None).await.unwrap();
        assert_eq!(servers.data[0].ping, Some(55));
    }

    #[test]
    fn region_is_not_invented() {
        assert_eq!(server_region_placeholder(), "auto");
    }
}
