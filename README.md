# RoFarm

**A fast, local-first, Windows-only Roblox account manager and multi-client orchestrator.**

RoFarm is an independent project for people who own multiple Roblox accounts and want to run,
observe and protect them from one place. Everything lives on your machine: there is no RoFarm
account, no telemetry, no cloud, and no background analytics.

> **Disclaimer.** RoFarm is not affiliated with, endorsed by, or sponsored by Roblox Corporation.
> Use it only with accounts you own and in accordance with Roblox's Terms of Use. RoFarm does not
> read or write Roblox client memory, does not inject code, does not hook the client, and does not
> attempt to evade detection. It uses documented Windows APIs and public Roblox web endpoints.
> Features can break whenever Roblox changes its client or web API. See [docs/ROBLOX.md](docs/ROBLOX.md).

## What it is

- **Command Deck** — a spatial board of account cards in lanes, a command palette for everything,
  and eleven views that can be popped out to any monitor.
- **Multi-client orchestration** — batch launch with stagger, per-client state machines, crash
  breaker, auto-rejoin, window walls, job-object CPU/memory limits, EcoQoS and GPU preferences.
- **Vault v2** — XChaCha20-Poly1305, Argon2id calibrated to ~500 ms, per-account data keys wrapped by
  a master key, DPAPI/TPM binding, Windows Hello, TOTP, recovery codes, Shamir splits, duress vault,
  encrypted `.rofarm` backups and a hash-chained audit log.
- **Local-only automation** — visual/YAML automations, cron schedules, scenes, lane rotations,
  dependency rules, dry-run, and a sandboxed Rhai script console.
- **Integration without the cloud** — Discord, Telegram, Slack, ntfy, Pushover, SMTP and native
  toasts; an opt-in, authenticated local REST API plus `rofarmd` daemon and `rofarm` CLI.
- **Windows-native** — jump lists, taskbar progress and overlay badges, WinRT toasts with actions,
  `rofarm://` deep links, Task Scheduler autostart with wake timers, DWM thumbnails, audio sessions,
  virtual-desktop parking, Mica/Acrylic, system accent sync.

## Repository layout

```
crates/
  rofarm-core/        domain, event-sourced fleet state, state machines, parsers, scheduling
  rofarm-vault/       crypto, key hierarchy, Windows bindings, .rofarm backup format, audit chain
  rofarm-roblox/      typed Roblox web API client, network allow-list, mock server
  rofarm-win/         Windows platform layer behind a trait (process, jobs, DWM, audio, TPM, …)
  rofarm-automation/  triggers/conditions/actions, cron, scenes, rotations, Rhai sandbox
  rofarm-plugin-host/ sandboxed WASM plugins with a permission manifest
  rofarm-api/         local REST API: pairing, PIN/TOTP, rate limits, TLS, CSRF/Host checks
  rofarm-daemon/      rofarmd headless daemon (shares the core event stream)
  rofarm-cli/         rofarm CLI with JSON output and PowerShell completions
  rofarm-app/         Tauri 2 shell: commands, tray, hotkeys, updater, deep links, windows
ui/                   Solid + TypeScript Command Deck (no heavy UI framework)
docs/                 architecture, threat model, performance, automation, plugin SDK, status matrix
packaging/            WiX, Inno Setup, MSIX, winget manifests
scripts/              signing, SBOM, SHA256SUMS, benchmark gate, vault migration
e2e/                  Playwright UI tests against the browser mock bridge
fuzz/                 cargo-fuzz targets for vault/import/backup parsing
```

## Build

The desktop shell targets **Windows 10 1809+ / Windows 11**, x64 and ARM64, with the MSVC toolchain
and WebView2.

```powershell
# Rust workspace (all libraries, daemon and CLI)
cargo test --workspace --exclude rofarm-app
cargo run -p rofarm-cli -- --help

# Command Deck (browser dev mode uses the mock bridge)
cd ui; bun install; bun run dev

# Desktop shell
cargo build --release -p rofarm-app --features custom-protocol
```

Release builds use LTO, `codegen-units = 1`, `panic = "abort"` and stripped symbols. The UI ships as
lazy-loaded chunks; `bun run build` fails if the gzipped bundle exceeds 400 KB.

## Security

Read [docs/SECURITY.md](docs/SECURITY.md) before trusting RoFarm with accounts. Key points:

- Vault files are authenticated, versioned and bound to your Windows account with DPAPI, optionally
  sealed to TPM 2.0 and unlocked with Windows Hello / FIDO2.
- Secret material is decrypted just-in-time, kept in locked buffers, excluded from crash dumps and
  never written to logs.
- The local API is opt-in, TLS-first, PIN + TOTP, per-device pairing tokens and an IP allow-list.
- RoFarm ships no telemetry. Crash reports exist only if you explicitly build one, preview it, and
  save it yourself.

## Status

This repository is an implementation of the RoFarm product specification. What is fully
implemented, scaffolded, or simulated is tracked honestly in [docs/FEATURES.md](docs/FEATURES.md).
The desktop shell requires MSVC; the rest of the workspace is toolchain-agnostic and tested in CI.

## License

Source-available for personal use with accounts you own. No warranty.
