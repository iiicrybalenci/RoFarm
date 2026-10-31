use rofarm_api::{start, ApiConfig};
use rofarm_core::service::FleetService;
use rofarm_core::store::MemoryEventStore;
use rofarm_roblox::api::RobloxClient;
use rofarm_roblox::mock::MockTransport;
use rofarm_roblox::net::NetPolicy;
use rofarm_runtime::FleetRuntime;
use rofarm_win::MockPlatform;
use serde_json::{json, Value};
use std::sync::Arc;

async fn server() -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().unwrap();
    let service = Arc::new(FleetService::open(Arc::new(MemoryEventStore::new())).unwrap());
    let api = Arc::new(RobloxClient::with_transport(Arc::new(MockTransport::default()), Arc::new(NetPolicy::default())));
    let runtime = Arc::new(FleetRuntime::new(service, Arc::new(MockPlatform::new()), api, temp.path().join("v.vault")));
    let config = ApiConfig {
        port: 0,
        bind_all: false,
        tls: false,
        cert_dir: temp.path().join("tls"),
        pin: Some("482913".into()),
        totp_secret: None,
        allowlist: vec![],
    };
    let (handle, http) = start(runtime, config).await.unwrap();
    let base = format!("http://{}", handle.address);
    let _ = http;
    (temp, base)
}

#[tokio::test]
async fn health_pair_and_authorized_status() {
    let (_temp, base) = server().await;
    let client = reqwest::Client::new();
    let health: Value = client.get(format!("{base}/health")).send().await.unwrap().json().await.unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["telemetry"], false);

    let denied = client
        .post(format!("{base}/api/v1/pair"))
        .json(&json!({ "pin": "000000", "device_name": "test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 401);

    let paired: Value = client
        .post(format!("{base}/api/v1/pair"))
        .json(&json!({ "pin": "482913", "device_name": "test" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = paired["token"].as_str().unwrap().to_string();
    assert!(token.len() > 20);

    let status = client
        .get(format!("{base}/api/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let body: Value = status.json().await.unwrap();
    assert_eq!(body["vault"]["unlocked"], false);

    let unauth = client.get(format!("{base}/api/v1/status")).send().await.unwrap();
    assert_eq!(unauth.status(), 401);
}

#[tokio::test]
async fn cross_origin_mutations_are_rejected() {
    let (_temp, base) = server().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/api/v1/pair"))
        .header("origin", "https://evil.example")
        .json(&json!({ "pin": "482913" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn dashboard_is_served_with_csp() {
    let (_temp, base) = server().await;
    let response = reqwest::get(format!("{base}/")).await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.headers().contains_key("content-security-policy"));
    assert!(response.text().await.unwrap().contains("RoFarm Remote"));
}
