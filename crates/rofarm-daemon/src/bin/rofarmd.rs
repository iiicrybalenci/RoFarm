use clap::Parser;
use rofarm_core::service::FleetService;
use rofarm_core::store::JsonlEventStore;
use rofarm_roblox::api::RobloxClient;
use rofarm_runtime::{AppPaths, FleetRuntime};
use rofarm_win::WindowsPlatform;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "rofarmd", version, about = "RoFarm headless daemon")]
struct Args {
    #[arg(long, env = "ROFARM_HOME")]
    home: Option<std::path::PathBuf>,
    #[arg(long, default_value_t = 1000)]
    tick_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let paths = match args.home.clone() {
        Some(root) => AppPaths::at(root),
        None => AppPaths::discover(),
    };
    paths.ensure()?;
    paths.harden()?;
    let store = Arc::new(JsonlEventStore::open(paths.root.join("events"))?);
    let service = Arc::new(FleetService::open(store)?);
    let api = Arc::new(RobloxClient::live(FleetRuntime::network_policy())?);
    let runtime = Arc::new(FleetRuntime::new(service, Arc::new(WindowsPlatform::new()), api, paths.vault()));
    tracing::info!(root = %paths.root.display(), "rofarmd started (local only, no telemetry)");
    let mut tick = tokio::time::interval(Duration::from_millis(args.tick_ms.max(250)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                match runtime.tick() {
                    Ok(report) if report.changed => {
                        tracing::debug!(exited = report.exited.len(), "fleet changed");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "fleet tick failed"),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown requested");
                let _ = runtime.kill_all(&rofarm_core::events::Actor::System);
                break;
            }
        }
    }
    Ok(())
}
