use clap::{Parser, Subcommand};
use rofarm_core::diagnostics::{run_doctor, DoctorContext};
use rofarm_core::events::Actor;
use rofarm_core::ids::AccountId;
use rofarm_core::launch::LaunchProfile;
use rofarm_core::service::FleetService;
use rofarm_core::store::JsonlEventStore;
use rofarm_roblox::api::RobloxClient;
use rofarm_runtime::{AppPaths, FleetRuntime, RuntimeError};
use rofarm_vault::audit::AuditKind;
use rofarm_win::WindowsPlatform;
use serde_json::json;
use std::io::Read;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rofarm", version, about = "RoFarm fleet CLI — local, no telemetry")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true, env = "ROFARM_HOME")]
    home: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    List,
    Status,
    Launch {
        target: String,
        #[arg(long)]
        accounts: String,
        #[arg(long, default_value_t = 2)]
        stagger_ms: u64,
        #[arg(long)]
        profile: Option<String>,
    },
    Kill {
        target: String,
    },
    Scene {
        #[command(subcommand)]
        action: SceneAction,
    },
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    Doctor,
    Network,
    Diagnostics {
        #[arg(long, default_value = "rofarm-diagnostics.json")]
        out: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum SceneAction {
    List,
    Apply { id: String },
}

#[derive(Subcommand)]
enum VaultAction {
    Status,
    Unlock,
    Lock,
}

fn read_passphrase() -> Result<String, RuntimeError> {
    if let Ok(value) = std::env::var("ROFARM_PASSPHRASE") {
        return Ok(value);
    }
    eprintln!("Vault passphrase (input hidden is not available in this terminal; press Ctrl+C to abort):");
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).map_err(RuntimeError::Io)?;
    Ok(buf.trim_end_matches(['\r', '\n']).to_string())
}

fn paths(home: Option<std::path::PathBuf>) -> AppPaths {
    match home {
        Some(root) => AppPaths::at(root),
        None => AppPaths::discover(),
    }
}

fn runtime(paths: &AppPaths) -> Result<FleetRuntime, Box<dyn std::error::Error>> {
    paths.ensure()?;
    let store = Arc::new(JsonlEventStore::open(paths.root.join("events"))?);
    let service = Arc::new(FleetService::open(store)?);
    let api = Arc::new(RobloxClient::live(FleetRuntime::network_policy())?);
    Ok(FleetRuntime::new(service, Arc::new(WindowsPlatform::new()), api, paths.vault()))
}

fn print_json(value: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let paths = paths(cli.home.clone());
    let rt = runtime(&paths)?;
    let actor = Actor::Cli;
    match cli.command {
        Commands::List => {
            let accounts = rt.accounts();
            if cli.json {
                print_json(json!(accounts));
            } else {
                for a in accounts {
                    println!(
                        "{:<24} {:<20} {}",
                        a.display(),
                        a.lane_id,
                        format!("{:?}", a.login)
                    );
                }
            }
        }
        Commands::Status => {
            let snapshot = rt.snapshot()?;
            if cli.json {
                print_json(json!(snapshot));
            } else {
                println!(
                    "{} accounts · {} running · {:.1}% CPU · {} MB",
                    snapshot.fleet.accounts.len(),
                    snapshot.totals.running,
                    snapshot.totals.cpu_pct,
                    snapshot.totals.mem_mb
                );
                for c in &snapshot.live {
                    println!(
                        "{:<14} {:<12} pid={:?} cpu={:.1}% mem={}MB",
                        c.client_id.to_string()[..8].to_string(),
                        format!("{:?}", c.state),
                        c.pid,
                        c.cpu_pct,
                        c.mem_mb
                    );
                }
            }
        }
        Commands::Launch { target, accounts, stagger_ms, profile } => {
            let pass = read_passphrase()?;
            rt.open_vault(&pass)?;
            let ids: Vec<AccountId> = accounts
                .split(',')
                .filter_map(|s| s.trim().parse::<AccountId>().ok())
                .collect();
            if ids.is_empty() {
                return Err("No valid account IDs. Use `rofarm list --json`.".into());
            }
            let mut launch_profile = if let Some(profile_id) = profile {
                rt.profile(profile_id.parse().map_err(|_| RuntimeError::Invalid("bad profile id".into()))?)
                    .ok_or_else(|| RuntimeError::Invalid("Profile not found.".into()))?
            } else {
                LaunchProfile::new("CLI", target.clone())
            };
            launch_profile.target = target;
            launch_profile.stagger_ms = stagger_ms;
            let outcome = rt.launch(&actor, &ids, launch_profile).await?;
            if cli.json {
                print_json(json!(outcome));
            } else {
                for o in outcome.outcomes {
                    println!("{} -> {:?} {:?}", o.account_id, o.state, o.error);
                }
            }
        }
        Commands::Kill { target } => {
            let count = if target == "all" {
                rt.kill_all(&actor)?
            } else {
                let id = target.parse().map_err(|_| RuntimeError::Invalid("bad client id".into()))?;
                u32::from(rt.kill_client(&actor, id)?)
            };
            if cli.json {
                print_json(json!({ "killed": count }));
            } else {
                println!("Killed {count} client(s).");
            }
        }
        Commands::Scene { action } => match action {
            SceneAction::List => {
                let scenes: Vec<_> = rt.service().snapshot().scenes.values().cloned().collect();
                print_json(json!(scenes));
            }
            SceneAction::Apply { id } => {
                let scene_id = id.parse().map_err(|_| RuntimeError::Invalid("bad scene id".into()))?;
                let applied = rt.service().apply_scene(&actor, scene_id)?;
                let _ = rt.audit_entries();
                print_json(json!({ "applied": applied }));
            }
        },
        Commands::Vault { action } => match action {
            VaultAction::Status => {
                let status = rt.status()?;
                if cli.json {
                    print_json(json!(status));
                } else {
                    println!(
                        "{} — {}",
                        status.path.display(),
                        if status.unlocked { "unlocked" } else { "locked" }
                    );
                }
            }
            VaultAction::Unlock => {
                let pass = read_passphrase()?;
                let status = rt.open_vault(&pass)?;
                print_json(json!(status));
            }
            VaultAction::Lock => {
                rt.lock_vault();
                print_json(json!({ "locked": true }));
            }
        },
        Commands::Doctor => {
            let status = rt.status()?;
            let ctx = DoctorContext {
                vault_exists: status.exists,
                vault_unlocked: status.unlocked,
                event_chain_ok: rt.service().verify().is_ok(),
                data_dir_writable: std::fs::metadata(&paths.root).is_ok(),
                telemetry_free: true,
                ..Default::default()
            };
            let report = run_doctor(&ctx, chrono::Utc::now());
            if cli.json {
                print_json(json!(report));
            } else {
                for check in report.checks {
                    println!("[{:?}] {} — {}", check.status, check.label, check.detail);
                }
            }
        }
        Commands::Network => {
            let policy = FleetRuntime::network_policy();
            print_json(json!({
                "hosts": policy.allowed_hosts(),
                "note": "RoFarm only contacts these hosts, plus user-configured notifier endpoints."
            }));
        }
        Commands::Diagnostics { out } => {
            let snapshot = rt.snapshot()?;
            let status = rt.status()?;
            let ctx = DoctorContext {
                vault_exists: status.exists,
                vault_unlocked: status.unlocked,
                event_chain_ok: rt.service().verify().is_ok(),
                data_dir_writable: true,
                telemetry_free: true,
                ..Default::default()
            };
            let report = run_doctor(&ctx, chrono::Utc::now());
            let bundle = rofarm_core::diagnostics::build_bundle(
                &snapshot.fleet,
                report,
                env!("CARGO_PKG_VERSION"),
                &[],
            );
            std::fs::write(&out, serde_json::to_vec_pretty(&bundle)?)?;
            let _ = rt.audit_entries();
            let _ = AuditKind::Export;
            println!("Wrote {}", out.display());
        }
    }
    Ok(())
}
