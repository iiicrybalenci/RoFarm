use crate::auth::{PairRequest, PairResponse, RemoteAuth};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rofarm_core::events::Actor;
use rofarm_core::ids::{AccountId, ClientId, SceneId};
use rofarm_core::launch::LaunchProfile;
use rofarm_runtime::{FleetRuntime, RuntimeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub port: u16,
    pub bind_all: bool,
    pub tls: bool,
    pub cert_dir: PathBuf,
    pub pin: Option<String>,
    pub totp_secret: Option<Vec<u8>>,
    pub allowlist: Vec<IpAddr>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: 47810,
            bind_all: false,
            tls: true,
            cert_dir: PathBuf::from("."),
            pin: None,
            totp_secret: None,
            allowlist: vec!["127.0.0.1".parse().unwrap(), "::1".parse().unwrap()],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiHandle {
    pub address: SocketAddr,
    pub scheme: &'static str,
    pub certificate_sha256: Option<String>,
    pub dashboard_url: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone)]
struct ApiState {
    runtime: Arc<FleetRuntime>,
    auth: Arc<RemoteAuth>,
    config: ApiConfig,
    started: Instant,
    fingerprint: Option<String>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }
    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "Pair this device first.")
    }
    fn forbidden(message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "code": self.code, "message": self.message }))).into_response()
    }
}

impl From<RuntimeError> for ApiError {
    fn from(error: RuntimeError) -> Self {
        match &error {
            RuntimeError::Locked => Self::new(StatusCode::LOCKED, "vault_locked", "Unlock the vault in RoFarm first."),
            RuntimeError::Core(rofarm_core::CoreError::NotFound(_)) => {
                Self::new(StatusCode::NOT_FOUND, "not_found", error.to_string())
            }
            RuntimeError::Core(rofarm_core::CoreError::Conflict(_)) => {
                Self::new(StatusCode::CONFLICT, "conflict", error.to_string())
            }
            RuntimeError::Core(rofarm_core::CoreError::Invalid(_)) | RuntimeError::Invalid(_) => {
                Self::new(StatusCode::BAD_REQUEST, "invalid", error.to_string())
            }
            _ => {
                tracing::warn!(%error, "API request failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "The request could not be completed.")
            }
        }
    }
}

fn bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn require_auth(
    State(state): State<ApiState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !state.auth.ip_allowed(addr.ip()) {
        return Err(ApiError::forbidden("This address is not allowed."));
    }
    let token = bearer(request.headers()).ok_or_else(ApiError::unauthorized)?;
    let device = state.auth.authenticate(&token).ok_or_else(ApiError::unauthorized)?;
    request.extensions_mut().insert(device);
    Ok(next.run(request).await)
}

fn valid_host(host: &str) -> bool {
    let host = host.trim();
    let host = if host.starts_with('[') {
        host.trim_start_matches('[').split(']').next().unwrap_or(host)
    } else {
        host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host)
    };
    host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok()
}

async fn origin_guard(request: Request, next: Next) -> Result<Response, ApiError> {
    if let Some(host) = request.headers().get(header::HOST).and_then(|v| v.to_str().ok()) {
        if !valid_host(host) {
            return Err(ApiError::forbidden("Use the machine's IP address or localhost, not a DNS name."));
        }
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        if let Some(origin) = request.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
            let Some(origin_host) = url::Url::parse(origin).ok().and_then(|u| u.host_str().map(str::to_string)) else {
                return Err(ApiError::forbidden("Invalid Origin header."));
            };
            let host = request
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .split(':')
                .next()
                .unwrap_or_default();
            if !origin_host.eq_ignore_ascii_case(host) && !valid_host(&origin_host) {
                return Err(ApiError::forbidden("Cross-origin mutations are not allowed."));
            }
        }
    }
    Ok(next.run(request).await)
}

async fn health(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "app": "RoFarm",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.started.elapsed().as_secs(),
        "tls": state.config.tls,
        "certificate_sha256": state.fingerprint,
        "telemetry": false,
    }))
}

async fn dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn pair(
    State(state): State<ApiState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<PairRequest>,
) -> Result<Json<PairResponse>, ApiError> {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    state
        .auth
        .pair(addr.ip(), &request, agent)
        .map(Json)
        .map_err(|message| ApiError::new(StatusCode::UNAUTHORIZED, "pairing_failed", message))
}

async fn status(State(state): State<ApiState>) -> Result<Json<rofarm_runtime::RuntimeSnapshot>, ApiError> {
    Ok(Json(state.runtime.snapshot()?))
}

async fn accounts(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.runtime.accounts()))
}

#[derive(Debug, Deserialize)]
struct LaunchRequest {
    accounts: Vec<AccountId>,
    target: String,
    profile_id: Option<rofarm_core::ids::LaunchProfileId>,
}

async fn launch(
    State(state): State<ApiState>,
    axum::Extension(device): axum::Extension<crate::auth::Device>,
    Json(request): Json<LaunchRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.target.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid", "A target is required."));
    }
    let profile = match request.profile_id {
        Some(id) => state
            .runtime
            .profile(id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "Launch profile not found."))?,
        None => LaunchProfile::new("Remote", request.target.clone()),
    };
    let actor = Actor::Api { device: device.name };
    let outcome = state.runtime.launch(&actor, &request.accounts, profile).await?;
    Ok(Json(serde_json::json!(outcome)))
}

#[derive(Debug, Deserialize)]
struct KillRequest {
    client_id: Option<ClientId>,
    #[serde(default)]
    all: bool,
}

async fn kill(
    State(state): State<ApiState>,
    axum::Extension(device): axum::Extension<crate::auth::Device>,
    Json(request): Json<KillRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let actor = Actor::Api { device: device.name };
    if request.all {
        let killed = state.runtime.kill_all(&actor)?;
        return Ok(Json(serde_json::json!({ "killed": killed })));
    }
    let id = request
        .client_id
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "invalid", "Provide client_id or all=true."))?;
    let killed = state.runtime.kill_client(&actor, id)?;
    Ok(Json(serde_json::json!({ "killed": u32::from(killed) })))
}

async fn apply_scene(
    State(state): State<ApiState>,
    Path(id): Path<SceneId>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let applied = state
        .runtime
        .service()
        .apply_scene(&Actor::Api { device: "remote".into() }, id)
        .map_err(|error| ApiError::from(RuntimeError::Core(error)))?;
    Ok(Json(serde_json::json!({ "applied": applied })))
}

async fn devices(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.auth.devices()))
}

async fn revoke_device(State(state): State<ApiState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "revoked": state.auth.revoke(&id) }))
}

async fn audit(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "entries": state.runtime.audit_entries(),
        "chain_ok": state.runtime.verify_audit(),
    }))
}

async fn network() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "entries": [],
        "note": "The full network log is available in RoFarm → Logs → Network.",
    }))
}

async fn thumbnail(Path(_id): Path<ClientId>) -> ApiError {
    ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "not_supported",
        "Thumbnails are captured by the desktop app, not the remote API.",
    )
}

async fn events(
    State(state): State<ApiState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    if !state.auth.ip_allowed(addr.ip()) {
        return Err(ApiError::forbidden("This address is not allowed."));
    }
    let token = query.get("token").cloned().unwrap_or_default();
    state.auth.authenticate(&token).ok_or_else(ApiError::unauthorized)?;
    let interval = tokio::time::interval(Duration::from_secs(2));
    let stream = IntervalStream::new(interval).map(move |_| {
        let payload = state.runtime.snapshot().ok();
        let event = match payload {
            Some(snapshot) => Event::default().event("snapshot").json_data(snapshot).unwrap_or_default(),
            None => Event::default().event("error").data("snapshot unavailable"),
        };
        Ok(event)
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive")))
}

fn build_router(state: ApiState) -> Router {
    let protected = Router::new()
        .route("/api/v1/status", get(status))
        .route("/api/v1/accounts", get(accounts))
        .route("/api/v1/launch", post(launch))
        .route("/api/v1/kill", post(kill))
        .route("/api/v1/scene/{id}/apply", post(apply_scene))
        .route("/api/v1/devices", get(devices))
        .route("/api/v1/devices/{id}/revoke", post(revoke_device))
        .route("/api/v1/audit", get(audit))
        .route("/api/v1/network", get(network))
        .route("/api/v1/thumbnail/{id}", get(thumbnail))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/", get(dashboard))
        .route("/health", get(health))
        .route("/api/v1/pair", post(pair))
        .route("/api/v1/events", get(events))
        .merge(protected)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
        ))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")))
        .layer(middleware::from_fn(origin_guard))
        .with_state(state)
}

fn ensure_certificate(dir: &std::path::Path) -> Result<(Vec<u8>, Vec<u8>, String), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let cert_path = dir.join("remote-cert.pem");
    let key_path = dir.join("remote-key.pem");
    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(&cert_path).map_err(|e| e.to_string())?;
        let key = std::fs::read(&key_path).map_err(|e| e.to_string())?;
        let digest = Sha256::digest(&cert);
        return Ok((cert, key, hex::encode(digest)));
    }
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
        .map_err(|e| e.to_string())?;
    let cert = certified.cert.pem().into_bytes();
    let key = certified.key_pair.serialize_pem().into_bytes();
    std::fs::write(&cert_path, &cert).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, &key).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(&cert);
    Ok((cert, key, hex::encode(digest)))
}

pub async fn start(
    runtime: Arc<FleetRuntime>,
    config: ApiConfig,
) -> Result<(ApiHandle, axum_server::Handle), String> {
    let auth = Arc::new(RemoteAuth::new(
        config.pin.as_deref(),
        config.totp_secret.clone(),
        config.allowlist.clone(),
    ));
    let bind = if config.bind_all { "0.0.0.0" } else { "127.0.0.1" };
    let listener = std::net::TcpListener::bind((bind, config.port)).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let (tls_config, fingerprint) = if config.tls {
        let (cert, key, fingerprint) = ensure_certificate(&config.cert_dir)?;
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert, key)
            .await
            .map_err(|e| e.to_string())?;
        (Some(tls), Some(fingerprint))
    } else {
        (None, None)
    };
    let state = ApiState {
        runtime,
        auth,
        config: config.clone(),
        started: Instant::now(),
        fingerprint: fingerprint.clone(),
    };
    let router = build_router(state);
    let handle = axum_server::Handle::new();
    let server_handle = handle.clone();
    let scheme = if config.tls { "https" } else { "http" };
    let serve = move || async move {
        let result = if let Some(tls) = tls_config {
            axum_server::from_tcp_rustls(listener, tls)
                .handle(server_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
        } else {
            axum_server::from_tcp(listener)
                .handle(server_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
        };
        if let Err(error) = result {
            tracing::error!(%error, "remote API stopped");
        }
    };
    tokio::spawn(serve());
    let dashboard_url = format!("{scheme}://{address}/");
    Ok((
        ApiHandle {
            address,
            scheme,
            certificate_sha256: fingerprint,
            dashboard_url,
            started_at: Utc::now(),
        },
        handle,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_validation_rejects_dns_names() {
        assert!(valid_host("127.0.0.1:47810"));
        assert!(valid_host("localhost:47810"));
        assert!(valid_host("[::1]:47810"));
        assert!(!valid_host("evil.example:47810"));
    }
}
