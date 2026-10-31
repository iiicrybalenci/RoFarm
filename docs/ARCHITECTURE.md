# RoFarm Architecture

> Status: living document. It describes both the intended design and how the code in this
> repository realises it. Implementation gaps are listed in [FEATURES.md](FEATURES.md).

## 1. Principles

1. **Local-first, single writer.** One process owns the vault and the fleet event log. The UI, CLI,
   daemon and web dashboard are *readers/producers* through typed boundaries; none of them reimplements
   domain logic.
2. **Event-sourced core.** Every meaningful change is an immutable `DomainEvent` appended to a
   hash-chained log. Fleet state is a deterministic projection. Undo/redo, automation run history,
   session recovery and the audit trail all reuse the same stream.
3. **Windows-native over portable abstractions.** Job objects, DWM thumbnails, IO completion ports,
   WinRT notifications, Task Scheduler, DPAPI/NCrypt, virtual desktops. A trait boundary keeps the
   core testable off-Windows, not to make RoFarm cross-platform.
4. **Event-driven, with honest polling.** Prefer process/window/job notifications; when Windows has
   no event source, use bounded adaptive polling whose interval is driven by observed activity.
5. **No secrets in logs, ever.** Typed errors with stable codes; cookie strings are wrapped in
   redacting types from the moment they are parsed.
6. **Small, boring UI.** Solid + TypeScript, no component library, canvas for the board and charts,
   CSS variables for theming, lazy routes, virtualised lists.

## 2. Crate graph

```
                    ┌────────────────────────────────────────────┐
                    │              rofarm-core                   │
                    │ domain · events · projections · parsers    │
                    │ state machines · cron · scenes · health    │
                    └───────▲──────────▲──────────▲──────────────┘
                            │          │          │
            ┌───────────────┘          │          └───────────────┐
    ┌───────┴────────┐        ┌────────┴───────┐         ┌────────┴────────┐
    │  rofarm-vault  │        │ rofarm-win     │         │ rofarm-roblox   │
    │ crypto · DPAPI │        │ processes/jobs │         │ typed API +     │
    │ backup · audit │        │ DWM/audio/TPM  │         │ allow-list      │
    └───────▲────────┘        └────────▲───────┘         └────────▲────────┘
            │                          │                          │
    ┌───────┴────────┐        ┌────────┴────────┐                 │
    │ rofarm-automation│      │ rofarm-app      │◄────────────────┘
    │ rules · rhai   │        │ Tauri commands  │
    └───────▲────────┘        │ tray/hotkeys    │
            │                 └────────▲────────┘
    ┌───────┴────────┐   ┌─────────────┴──────┐   ┌─────────────────┐
    │ rofarm-plugin- │   │ rofarm-api         │   │ rofarm-daemon   │
    │ host (wasmi)   │   │ axum + TLS + auth  │   │ headless        │
    └───────▲────────┘   └─────────▲──────────┘   └────────▲────────┘
            └──────────────────────┴───────────────────────┘
                                   │
                            rofarm-cli (clap)
```

Dependency rules:

- `rofarm-core` depends on no platform or crypto crate. It is pure and heavily unit/property tested.
- `rofarm-vault` never depends on `rofarm-win`; the Windows binding is supplied as a `KeyBinder`
  trait implementation, enabling deterministic tests.
- `rofarm-app`, `rofarm-api`, `rofarm-daemon`, `rofarm-cli` are thin composition roots. They share
  `rofarm-core`'s `FleetService` API, so a launch from the UI, CLI or API takes the same state-machine
  path and emits the same events.
- `rofarm-win` is compiled only for Windows at runtime; its `platform::mock` module is available on
  every target so tests and the UI mock bridge exercise the same trajectories.

## 3. Event-sourced core

### 3.1 Event log

`EventStore` appends `StoredEvent { seq, at, actor, correlation, event, prev_hash, hash }` as JSONL.
The hash is BLAKE3 over `prev_hash || canonical_json(event_meta)`, giving a tamper-evident chain that
is verified on open. Rotation keeps per-file headers and carries the previous tail hash.

Emitted event families (`rofarm_core::events::DomainEvent`):

| Family | Examples |
|---|---|
| account | `AccountAdded`, `AccountUpdated`, `AccountRemoved`, `AccountMerged`, `LoginStateChanged` |
| lane | `LaneAdded`, `LaneRulesChanged`, `AccountMoved` |
| client | `ClientQueued`, `ClientLaunched`, `ClientStateChanged`, `ClientExited`, `ClientKilled` |
| launch | `LaunchProfileSaved`, `BatchStarted`, `BatchFinished` |
| vault | `VaultUnlocked`, `VaultLocked`, `SecretRevealed`, `ExportWritten`, `RestoreApplied` |
| automation | `AutomationSaved`, `AutomationFired`, `ActionRefused`, `RunRecorded` |
| scene | `SceneSaved`, `SceneApplied` |
| settings | `SettingChanged`, `ProfileSwitched`, `WorkspaceSwitched` |
| system | `FleetStopped`, `RecoveryOffered`, `NoticeRaised` |

`FleetProjection` folds events into `FleetState { accounts, lanes, clients, profiles, scenes,
automations, settings, metrics, notices }`. Projection is pure; `test_projection_is_deterministic`
replays a shuffled-forbidden stream twice and compares.

### 3.2 Undo/redo

Destructive commands are wrapped in `UndoStack` entries. Undo does not rewind the event log;
it appends a **compensating event** (`AccountAdded` after `AccountRemoved`, with the original payload)
so history stays append-only and the audit trail describes what actually happened. The stack is
bounded (default 50) and persisted with the vault.

### 3.3 State machines

`ClientState` implements the lifecycle from the spec:

```
Queued → Launching → InGame ⇄ Disconnected → Restarting → Launching
   │          │           │            │           │
   │          │           └────────────┴───────────┴──→ Failed
   └──────────┴────────────────────────────────────────→ Stopped
```

`ClientSupervisor` owns per-client policy: crash-breaker window, exponential backoff with jitter,
"allowed to stay disconnected", memory-hungry restart, and freeze detection. Invalid transitions
return `TransitionError` rather than silently coercing, and are unit tested exhaustively.

## 4. Windows platform layer

`rofarm-win::platform::Platform` is a trait; the Windows implementation lives in submodules:

| Concern | Implementation | Event source |
|---|---|---|
| Process discovery | Toolhelp32 snapshot, `QueryFullProcessImageNameW` | job notifications + `RegisterWaitForSingleObject` exit waits, fallback 5 s poll |
| Launch | `CreateProcessW` with `PROC_THREAD_ATTRIBUTE_JOB_LIST`, suspended start then resume | — |
| CPU/memory limits | Job object `JOB_OBJECT_LIMIT_*`, `SetInformationJobObject` | `JOB_OBJECT_MSG_*` completion port |
| Efficiency mode | `SetProcessInformation(ProcessPowerThrottling)` | — |
| CPU sets / affinity | `SetProcessDefaultCpuSets`, `SetProcessAffinityMask` | — |
| Memory trim | `SetProcessWorkingSetSizeEx`, `EmptyWorkingSet` | — |
| Windows | `EnumWindows`, DWM thumbnails, `SetWindowDisplayAffinity`, virtual desktops | `SetWinEventHook` |
| Audio | WASAPI session enumeration/mute/volume | `IAudioSessionNotification` |
| QoS | `QOSStartTrackingClient` / `QOSAddSocketToFlow` (UAC only on click) | — |
| TPM | NCrypt Platform Crypto Provider, `NCRYPT_UI_POLICY` for Hello | — |
| Toasts | WinRT `ToastNotificationManager` with actions | `ToastActivated` callback |
| Autostart | Task Scheduler (`IRegisteredTask`) with wake timers | — |
| GPU preference | `HKCU\…\DirectX\UserGpuPreferences` | — |
| ETW | provider assembly documented; process events currently use job handles (lower overhead) | adaptive |

Because ETW kernel consumers need elevated real-time sessions, RoFarm's default process tracking
uses per-process wait handles plus job-object notifications — event-driven, ~0% idle CPU — and keeps
an ETW adapter interface for privileged builds. This is a deliberate deviation from the "ETW process
events" bullet; correctness and non-admin operation win. `docs/FEATURES.md` records it.

### Adaptive polling

`AdaptivePoller` starts at 4 s while idle, drops to 250 ms for 10 s after a detected change, and backs
off on repeated empty polls. All loops run on one Tokio timer thread per concern, coalescing
subscribers through a broadcast channel.

## 5. Vault v2

Full design in [SECURITY.md](SECURITY.md). Envelope format:

```
Header (AEAD-associated):
  magic "ROFARMV2" · version · vault_id · kdf params · salt · wrapped master key
  · bindings (DPAPI / TPM / Hello / FIDO2) · recovery verifier · created_at · generation
Records:
  [len][nonce][ciphertext+tag] where AAD = header_hash || record_id || record_type || generation
```

- Master key is 32 random bytes.
- Account secrets use per-account 32-byte data keys, stored wrapped (XChaCha20-Poly1305) by the master
  key. Deleting an account destroys the wrapped key; records become unrecoverable even if ciphertext
  survives.
- Argon2id parameters are calibrated on the machine with `argon2::Params` targeting 500 ms, floor
  64 MiB / t=3, and stored per vault. Migration rewrites the header and re-wraps keys.
- Generation counters in headers + AAD defeat rollback of individual records.
- `.rofarm` backups re-encrypt under a *separate* passphrase with 1 MiB chunk framing, an integrity
  manifest (per-chunk hash + total), and a strict size/path schema on restore.

## 6. IPC and the frontend

- Tauri commands are generated from Rust types with `specta` (`tauri-specta` in the shell, a checked-in
  `ui/src/bindings.ts` in this repo). Every command has a typed `AppError { code, message, details? }`
  that never contains secret material.
- State updates are **coalesced**: the core sends `FleetDelta` batches at most every 100 ms (16 ms when
  a view requests interactive cadence), serialised as compact JSON; the metrics channel uses a binary
  layout (`u32` count, fixed-size records) decoded in a worker to avoid main-thread churn.
- Each window receives only the capabilities it needs (`capabilities/*.json`): the vault window gets
  `vault:reveal`, the OBS overlay gets nothing but `fleet:read`, mini mode gets `fleet:read` + `kill`.
- The UI runs against `Bridge` interface. In Tauri, `TauriBridge`; in a browser, `MockBridge` with a
  deterministic fleet — same TypeScript types, used by Playwright and local design work.
- CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://*.rbxcdn.com; connect-src 'self' ipc: http://ipc.localhost`. No remote content, no eval.

## 7. Performance design

Budgets are in [PERFORMANCE.md](PERFORMANCE.md). Techniques:

- Frontend: Solid fine-grained reactivity, route-level `import()` chunks, `content-visibility` and
  canvas board rendering (only visible cards materialised as DOM), IntersectionObserver-gated
  thumbnails, CSS transforms only, `prefers-reduced-motion`, timers parked on `visibilitychange`,
  and a hard stop for RAF loops in hidden windows.
- Backend: one metrics aggregation thread per 1 s tick; per-client samples stored in fixed ring
  buffers; IPC coalescing; `parking_lot` where contention is real; no per-tick allocation of the fleet
  snapshot (Arc + structural sharing).
- Tray/mini: the main webview is destroyed (not hidden) when the window is closed to tray; mini mode
  is a second 220×96 always-on-top webview that only reads the fleet summary at 1 Hz.
- Eco mode: 4 s UI cadence, no thumbnails, no sparkline history, metrics at 5 s, background clients
  throttled via EcoQoS, and RoFarm itself enters EcoQoS while hidden.

## 8. Roblox integration boundaries

- `rofarm-roblox` speaks only to `*.roblox.com` and `*.rbxcdn.com` through an allow-list `NetPolicy`.
  Every request is logged to the in-app network log with method, host, path, status, duration and a
  redacted body digest. Redirects out of the allow-list are refused.
- Authentication: RoLogin cookie validation via the authenticated user endpoint, CSRF token refresh,
  and a session store that keeps cookies only as vault records. QR login uses the public
  `AuthenticationTicket` flow and is marked experimental in the UI.
- Launching uses the documented `roblox-player:`/`roblox://` URI with an authentication ticket
  obtained per account, or `-b`/`-j` style arguments via `RobloxPlayerBeta.exe` when the user has
  enabled direct launch. Both paths are isolated behind `Launcher` so changes are one function.
- ClientAppSettings.json is written to the per-version client folder before launch from a backup of
  the original, and restored on exit; RoFarm never touches client memory.
- Fragile assumptions are listed in [ROBLOX.md](ROBLOX.md) and surfaced in the UI's Doctor panel.

## 9. Automations

`Trigger` (27 variants), `Condition` (metric comparisons, account predicates, time windows),
`Action` (18 variants). The engine evaluates triggers on the core event bus, builds a `Plan` of
actions, and applies it through the same `FleetService` used by the UI. `dry_run` short-circuits at
the plan stage and records what *would* happen. Every automation has a cooldown, a daily budget, and
a rate limiter. External I/O actions go through `ActionBroker` so the UI can show a confirmation
for anything marked dangerous. Rhai scripts run with an operation cap, no filesystem module, and a
host facade exposing only `fleet.*`, `notify.*`, `log.*`.

## 10. Packaging and updates

- **Installer:** WiX v4 per-user MSI (no elevation), Inno Setup fallback, MSIX for managed devices,
  portable zip with a fixed-version WebView2 runtime.
- **Updater:** `rofarm://` deep link + in-app check against a signed feed. Manifest includes version,
  channel, min-supported version, SHA-256, ed25519/minisign signatures per artifact. Deltas are
  chunk-based (`ROFD` format) with a full-artifact fallback. Downgrade rejection compares semver
  against the installed version unless a signed `allow_downgrade` flag is present. Rollback keeps the
  previous app directory and restores it if the new build fails its health check.
- **Signing:** Authenticode via Azure Trusted Signing (or EV cert) in CI; artifacts signed with
  `minisign`/`ed25519`; SHA256SUMS and CycloneDX SBOM published next to each release.
- **CI:** windows-latest + windows-11-arm, `cargo build --locked`, `cargo test`, clippy `-D warnings`,
  `cargo-audit`, `cargo-deny`, `cargo-vet`, `tsc --noEmit`, Playwright, bundle budget gate,
  reproducible-build check, SBOM + checksums + signing.

## 11. Milestones

| # | Milestone | Exit criteria |
|---|---|---|
| 1 | Windows platform layer + vault v2 | `rofarm-win` mock/real trait complete; vault crypto vectors, tamper/rollback tests, backup round-trip and fuzz seed corpora |
| 2 | Accounts + Roblox client | add/validate/import paths; typed API with allow-list and mock server; cookie/QR login behind stable interfaces |
| 3 | Launching + multi-instance | parser + preview, launch profiles, stagger/cooldown, state machine, mutex take-over, kill paths |
| 4 | Limits + monitoring + Eco | job-object limits, metrics ring buffers, thumbnails, profiles, EcoQoS, cost estimator |
| 5 | Command Deck UI | all 11 views, palette, board, themes, i18n, a11y, mini mode, widget, Playwright suite |
| 6 | Automation + scheduler + scenes | rule engine, dry-run, history, cron windows, rotations, dependencies, Rhai sandbox |
| 7 | Account intelligence | inventory/friends/badges/groups/avatars/health/notes |
| 8 | Alerts + integrations | 7 notifier transports, rules, mute/quiet hours/rate limits, two-way bots |
| 9 | CLI/daemon/API/plugins | JSON CLI, headless daemon, authenticated REST + TLS, WASM plugin host |
| 10 | Updater/signing/package/release | signed updater with delta + rollback, MSI/MSIX/zip/winget, CI release pipeline |

## 12. Architectural decisions

| # | Decision | Rationale |
|---|---|---|
| AD-1 | Event log is JSONL with BLAKE3 chain, not a database | zero-dependency portability, human-inspectable, easy backup; projection in memory is fast at fleet scale |
| AD-2 | Cookies live only in the vault, accounts in the event log | keeps the primary log free of secrets so it can be shared in diagnostics |
| AD-3 | Separate process for the headless daemon | tray/UI crashes must not stop automations; single-writer lock prevents split brain |
| AD-4 | Tauri commands + specta over a custom socket | smallest surface, typed, capability-scoped per window |
| AD-5 | Chunk-based delta updates | no external bsdiff binary, streaming-friendly, verifiable per chunk |
| AD-6 | Rhai for custom logic, WASM for plugins | Rhai is safe, embeddable and scriptable by users; WASM isolates third-party code |
| AD-7 | Adaptive polling where ETW needs elevation | RoFarm must not require admin; event-driven handles cover the hot paths |
| AD-8 | Compensating events for undo | preserves an append-only, auditable truth |
| AD-9 | MockBridge in the UI | Playwright without a Windows host, and a fast design loop |
| AD-10 | Roblox protocol isolated in one crate | Roblox changes frequently; blast radius stays small |
