# Changelog

All notable changes to RoFarm are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- New README: badges, Command Deck screenshot, command palette GIF, feature overview, quick start,
  CLI reference, architecture diagram, status and release sections.
- AGPL-3.0 license text, matching `license = "AGPL-3.0-or-later"` in the workspace manifest.
- Changelog covering the initial import and this documentation update.
- Tag-triggered release workflow that builds and packages `rofarm` and `rofarmd` for Windows x64.

## [0.1.0] - 2026-10-31

### Added

- `rofarm-core`: event-sourced fleet state, hash-chained JSONL event log, lane rules, launch state
  machines, cron scheduling, scenes, undo/redo and typed identifiers.
- `rofarm-vault`: Argon2id key derivation, XChaCha20-Poly1305 records, DPAPI/TPM binding, recovery
  codes, Shamir splits, encrypted `.rofarm` backups and a hash-chained audit log.
- `rofarm-roblox`: typed Roblox web client with a host allow-list and a mock server for tests.
- `rofarm-win`: process supervision, job objects, window/DWM helpers, audio sessions, TPM and
  notification wrappers behind a platform trait.
- `rofarm-automation`: triggers, conditions, actions, cron, rotations and a sandboxed Rhai console.
- `rofarm-plugin-host`: sandboxed WASM plugins with manifest permissions.
- `rofarm-api`: opt-in local REST API with pairing, PIN + TOTP, rate limits and TLS.
- `rofarm-daemon` and `rofarm-cli`: headless daemon plus a JSON-friendly command line.
- Command Deck: Solid + TypeScript UI with a lane board, command palette, themes, densities and a
  demo bridge for browser development.
- `fuzz/`: cargo-fuzz targets for the vault header and import parsers.

[Unreleased]: https://github.com/iiicrybalenci/RoFarm/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/iiicrybalenci/RoFarm/releases/tag/v0.1.0
