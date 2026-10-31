use rofarm_core::events::Actor;
use rofarm_core::launch::LaunchProfile;
use rofarm_core::service::{FleetService, NewAccount};
use rofarm_core::store::MemoryEventStore;
use rofarm_roblox::api::RobloxClient;
use rofarm_roblox::mock::MockTransport;
use rofarm_roblox::net::NetPolicy;
use rofarm_runtime::FleetRuntime;
use rofarm_win::{MockPlatform, Platform};
use std::sync::Arc;

fn auth_url(place: u64) -> String {
    let referer = format!("https://www.roblox.com/games/{place}/");
    let encoded: String = url::form_urlencoded::byte_serialize(referer.as_bytes()).collect();
    format!("https://auth.roblox.com/v1/authentication-ticket?url={encoded}")
}

fn setup() -> (tempfile::TempDir, Arc<FleetRuntime>, Arc<MockPlatform>, rofarm_core::ids::AccountId) {
    let temp = tempfile::tempdir().unwrap();
    let service = Arc::new(FleetService::open(Arc::new(MemoryEventStore::new())).unwrap());
    let account = service
        .add_account(
            &Actor::User,
            NewAccount { username: "test_account".into(), ..Default::default() },
        )
        .unwrap();
    let transport = Arc::new(MockTransport::default().with_post_json(
        &auth_url(123),
        r#"{"ticket":"ticket-for-test"}"#,
    ));
    let api = Arc::new(RobloxClient::with_transport(transport, Arc::new(NetPolicy::default())));
    let platform = Arc::new(MockPlatform::new());
    let runtime = Arc::new(FleetRuntime::new(service, platform.clone(), api, temp.path().join("v.vault")));
    runtime.create_vault("correct horse battery staple").unwrap();
    runtime.store_cookie(account.id, "_|WARNING:-DO-NOT-SHARE|_|testcookievalue").unwrap();
    (temp, runtime, platform, account.id)
}

#[tokio::test]
async fn launch_attaches_process_and_reports_in_game() {
    let (_temp, runtime, platform, account_id) = setup();
    let profile = LaunchProfile::new("Test", "123");
    let outcome = runtime.launch(&Actor::User, &[account_id], profile).await.unwrap();
    assert_eq!(outcome.outcomes.len(), 1);
    let first = &outcome.outcomes[0];
    assert!(first.error.is_none(), "{:?}", first.error);
    assert_eq!(runtime.tracked_count(), 1);
    assert!(platform.launched.lock().len() >= 1);
    let live = runtime.live_clients();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].account_id, account_id);
    assert!(live[0].pid.is_some());
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(snapshot.totals.running, 1);
    let killed = runtime.kill_all(&Actor::User).unwrap();
    assert_eq!(killed, 1);
    assert_eq!(runtime.tracked_count(), 0);
}

#[tokio::test]
async fn launch_is_refused_while_vault_is_locked() {
    let (_temp, runtime, _platform, account_id) = setup();
    runtime.lock_vault();
    let error = runtime
        .launch(&Actor::User, &[account_id], LaunchProfile::new("Test", "123"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("locked"));
}

#[test]
fn tick_detects_a_process_that_disappeared() {
    let (_temp, runtime, platform, account_id) = setup();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        runtime
            .launch(&Actor::User, &[account_id], LaunchProfile::new("Test", "123"))
            .await
            .unwrap();
    });
    let pid = runtime.pids()[0];
    platform.terminate_process(pid).unwrap();
    let report = runtime.tick().unwrap();
    assert_eq!(report.exited.len(), 1);
    assert_eq!(runtime.tracked_count(), 0);
    let client = &runtime.service().snapshot().clients.values().next().unwrap().clone();
    assert!(!client.state.is_running());
}

#[test]
fn target_preview_rejects_non_roblox_hosts() {
    assert!(FleetRuntime::target_preview("https://evil.example/games/1").is_err());
    let preview = FleetRuntime::target_preview("https://www.roblox.com/games/920587237/Adopt-Me").unwrap();
    assert_eq!(preview.place_id, Some(920587237));
}
