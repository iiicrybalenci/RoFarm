# RoFarm Security Model and Threat Model

Last reviewed: 2026-10-30 · Applies to vault v2 and later.

RoFarm stores Roblox session cookies. Those cookies are bearer credentials: anyone with one can act
as the account until Roblox expires it. This document states what RoFarm protects against, what it
explicitly does not, and how each defence is built in code.

## 1. Assets

| Asset | Sensitivity | Where it lives |
|---|---|---|
| Roblox `.ROBLOSECURITY` cookies, auth tickets | Critical — full account takeover | Vault records only, encrypted |
| Vault master key | Critical | Memory (locked, zeroized); optional DPAPI/TPM seal |
| Account metadata (names, IDs, notes, friends) | High | Event log + vault metadata (encrypted) |
| Backups (`.rofarm`) | Critical | User-chosen folder; encrypted with a separate passphrase |
| Alerts webhook URLs, SMTP credentials, pairing tokens | High | Vault records / DPAPI |
| Audit log | High integrity | Hash-chained, appended |
| Themes, layouts, settings | Low | Plain JSON |

## 2. Adversaries and goals

### In scope

1. **Local thief with file access but not your Windows session** (stolen laptop, copied AppData).
   Goal: read cookies and account data. Defence: vault is useless without the passphrase or the
   DPAPI/TPM binding; backups are separately encrypted; plaintext is never written to disk.
2. **Malware running as your Windows user** (infostealer). Goal: read RoFarm data, capture screens,
   keylog, or exfiltrate backups. Defence: DPAPI blobs are readable by same-user malware, so RoFarm
   **does not claim protection here** for an unlocked vault. It does: keep secrets out of logs, exclude
   secrets from screen capture on secret screens, auto-lock on idle/session events, restrict the data
   directory ACL, zeroize and lock buffers, and make export/reveal require re-authentication so a
   background process cannot silently harvest. A process running as you with a debugger attached can
   read any process memory; no user-mode app can prevent that.
3. **Shoulder-surfer / screen recording.** Defence: secret views marked
   `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`; secrets masked by default; clipboard cleared
   after 20 s with `ExcludeClipboardContentFromMonitorProcessing` and history opt-out; reveal requires
   re-authentication.
4. **Lost or stolen laptop, offline.** Defence: passphrase-derived key (Argon2id), optional TPM seal,
   optional FIDO2 second factor, auto-lock, duress vault, and backups that are always encrypted.
5. **Malicious or corrupted backup / update file.** Defence: authenticated chunked backup format with
   strict schema/size validation and dry-run verify; updater requires ed25519/minisign signatures over
   a canonical manifest, rejects downgrades, and verifies each chunk hash.
6. **Network attacker on the local network using the remote dashboard.** Defence: opt-in only, TLS with
   pinned self-signed certificate by default, PIN + optional TOTP, per-device pairing tokens, rate
   limiting, Host/Origin/CSRF checks, IP allow-list, and a firewall rule only on explicit click.
7. **Tampering with the audit trail or vault records.** Defence: BLAKE3 hash chain over audit entries
   and over the event log; vault record AAD includes header hash, record id, type and generation, so
   reordering or replaying records fails authentication.
8. **Supply-chain compromise.** Defence: `Cargo.lock` committed, `cargo-vet`/`cargo-deny`/`cargo-audit`
   in CI, pinned GitHub Actions by SHA in the release workflow, signed commits/tags/releases, SBOM.

### Out of scope (stated honestly)

- **Administrator or kernel-level malware.** If an attacker is admin, TPM unsealing, DPAPI and memory
  scraping are all available. RoFarm is not a hypervisor.
- **A compromised WebView2 process with the vault unlocked.** RoFarm limits what the webview can call
  through a capability-scoped IPC surface, but the Rust core must decrypt for the vault window to work.
- **Roblox-side compromise or account bans.** RoFarm cannot protect you from Roblox enforcement.
- **Physical coercion.** The duress passphrase helps with a casual demand but is not coercion-resistant
  cryptography.
- **Memory forensics against a running, unlocked process.** Best-effort only (locked pages, zeroize,
  WER exclusion). Windows offers no user-mode guarantee against same-privilege process inspection.

## 3. Vault v2

### 3.1 Envelope

```
ROFARMV2 | header_len | header (canonical CBOR/JSON) | records
header = {
  vault_id, generation, created_at, updated_at,
  kdf: { algorithm: "argon2id", m_cost_kib, t_cost, p_cost, salt[32] },
  wrapped_master_key: { nonce[24], ct }        // AEAD under passphrase-derived key
  bindings: [dpapi | tpm2 | hello | fido2_hmac_secret],
  recovery_present: bool,
  header_hmac: HMAC-BLAKE3(binding_key, canonical_header_without_hmac)
}
record = len:u32 | nonce[24] | ciphertext+tag
AAD   = BLAKE3(header) || record_id || record_type || generation || account_id
```

- Cipher: **XChaCha20-Poly1305** (24-byte random nonce per record, 256-bit key).
- Each account has its own 32-byte **data key**, wrapped by the master key. Deleting an account
  destroys the wrapped key (crypto-shredding).
- Header is authenticated (HMAC under the master key) and included in every record's AAD, so editing
  KDF params, generation or bindings invalidates all records.
- Wrong passphrase and tampered header produce the same `VaultError::AuthenticationFailed`; no oracle
  distinguishes them.
- Rollback: `generation` increments on every write; a record tagged with an older generation is
  rejected. Whole-file rollback is visible in the audit log and blocked by the `.rofarm` manifest when
  restoring from backup.

### 3.2 Key derivation

- Argon2id, 32-byte salt from the OS CSPRNG, parameters calibrated at vault creation/unlock on the
  local machine to **500 ms ± 20%** (`rofarm-vault::kdf::calibrate`), with a floor of m=64 MiB, t=3,
  p=min(4, cores). Parameters are stored per vault and validated against sanity limits (max 1 GiB,
  t≤16) before use to prevent a hostile vault from exhausting memory.
- The derived key is used once to unwrap the master key, then zeroized.
- Unlock attempts: 5 failures → 30 s lockout, doubling to a 15-minute cap, persisted in the vault
  header so restarting RoFarm does not reset the counter.

### 3.3 Windows bindings

| Binding | Protects | Fails when |
|---|---|---|
| DPAPI (`CryptProtectData`, user scope) | Vault key at rest, other users | Windows profile reset, another machine |
| DPAPI-NG (CVS) | Optional stronger scoping | Domain changes |
| TPM 2.0 seal (NCrypt Platform Crypto Provider, auth policy) | Offline disk theft, key extraction | Motherboard/TPM clear, Windows reinstall |
| Windows Hello (`NCRYPT_UI_POLICY`, `NCRYPT_PIN_PROMPT`) | Casual access to an unlocked session | Console access as same user with Hello reset |
| FIDO2/YubiKey `hmac-secret` | Offline theft, second factor | Key loss (recovery code required) |

Binding is additive to the passphrase: losing a binding never locks you out if you know the
passphrase or hold the recovery code. Bindings are removed atomically on disable, before the header is
rewritten, so a crash cannot leave a vault bound to a dead TPM.

### 3.4 Recovery paths (exact scope)

| Path | Works after reinstall? | Works on new hardware? | Works if passphrase lost? | Needs other devices? |
|---|---|---|---|---|
| Passphrase | Yes, with the vault file | Yes | — | No |
| Recovery code (9 chars, Crockford base32, 45 bits) | Yes | Yes | Yes | No |
| Shamir 2-of-N shares | Yes | Yes | Yes | Yes, any 2 shares |
| TPM-sealed key | No | **No** — TPM sealing does not survive hardware changes | Only if binding is the only factor | No |
| DPAPI blob | No | No | Only if binding is the only factor | No |
| Windows Hello / FIDO2 | No | No | No — it is a factor, not a recovery path | FIDO2 key |
| `.rofarm` backup + separate passphrase | Yes | Yes | Only if it is the vault backup and you know its passphrase | No |

The UI states this table verbatim at vault creation and in Settings → Vault → Recovery. The recovery
code is shown once and can be re-issued only while unlocked.

### 3.5 Auto-lock

Locks on: 5–60 min idle (configurable), sleep/resume, `WTS_SESSION_LOCK`, session switch, screenshot
affinity loss, N failed unlock attempts, manual lock, and when the last window closes. Lock zeroizes
master and data keys, drops decrypted buffers, and clears clipboard entries RoFarm created. A
"remember for 8 hours" option is deliberately absent.

### 3.6 Duress vault

A second passphrase opens a decoy vault with its own master key, accounts and audit chain, kept in a
separate file. The decoy is created during onboarding or in Settings. RoFarm cannot prove which
passphrase was used, and neither vault reveals the other's existence. Limitation: file timestamps and
storage usage can reveal that a second vault exists; the UI warns about this.

## 4. Memory hygiene

- `secrecy::SecretBox` and `zeroize` for passphrases, cookies, tokens; `Zeroizing<Vec<u8>>` scratch.
- `VirtualLock` on key material with `SetProcessWorkingSetSize` pressure handling; failure is
  tolerated (best effort) and reported in the Doctor panel.
- Just-in-time decryption: cookies decrypt into a 2-minute cache per account, dropped after use.
- `SetProcessMitigationPolicy` best-effort, WER `ExcludedApplications` registration under HKCU on
  first run (opt-in, shown in Settings → Privacy).
- Structured logging redacts values through `Redacted<T>`; a unit test asserts cookie-shaped strings
  never survive `Debug` or `Display`.
- Panic handler never prints environment, arguments or vault paths.

## 5. Encrypted backups (`.rofarm`)

```
ROFBK1 | header (Argon2id params, salt, kdf, manifest hash, chunk size)
chunk i: u32 len | u64 index | nonce[24] | ciphertext+tag   AAD = header_hash || index || total
manifest: total_size, file_count, per-chunk BLAKE3, schema version, created_at, app_version
```

- Separate passphrase (may differ from the vault passphrase), Argon2id calibrated the same way.
- Streaming; 1 MiB chunks; verification mode decrypts and hashes everything without touching disk.
- Restore validates: schema version, chunk order, total size limit, path traversal (`..`, absolute,
  drive-relative), duplicate names, and per-file size. Only known record types are applied; unknown
  types are rejected, not ignored.
- Backups are written to `*.partial` and renamed on success; retention `keep N / keep days`, with a
  minimum of 1.

## 6. IPC and web surface

- Tauri capability files grant the minimum per window. Vault reveal/copy/export commands also require
  a re-authentication token with a 60 s lifetime, checked in the Rust command.
- CSP: no remote scripts, no `eval`, no remote frames; `img-src` limited to self/data and `*.rbxcdn.com`.
- All command inputs are typed and validated (lengths, ranges, IDs are UUIDs, URLs parsed by `url`).
- Errors are `{code, message}` with no paths, cookie fragments, or SQL/backtraces.
- Optional AppContainer/low-integrity sandbox for the dashboard and plugin host with an ACL-restricted
  data directory (`icacls` note in the installer; the daemon validates DACL at start and warns).

## 7. Remote dashboard

Off by default. Enabling requires: set PIN (6 digits, Argon2id-hashed), optional TOTP, choose TLS
(self-signed pinned or user certificate), optional IP allow-list, then an explicit "open firewall" click
which triggers UAC. Every request validates Host and Origin, requires a pairing token issued by a
6-digit one-time code shown on the desktop, and is rate-limited per IP and per token (token bucket).
Pairing tokens are 32-byte random, stored hashed, revocable individually, and listed with last-used IP
and time. Dashboard cookies are `HttpOnly; Secure; SameSite=Strict`. No dashboard path can reveal a
cookie value; only status and control actions.

## 8. Tamper-evident audit log

`audit.jsonl` entries:

```
{ seq, at, kind, subject, detail_redacted, prev_hash, hash }
hash = BLAKE3(prev_hash || canonical_json(entry_without_hash))
```

Recorded: unlock, lock, failed unlock, reveal, copy, export, restore, backup created/verified, vault
settings change, binding added/removed, remote pairing/revocation, remote login success/failure, plugin
permission grant, automation created/modified/deleted, kill-all, and updater applied/rolled back.
Verification walks the chain and reports the first broken index. The log can be exported (redacted)
for a disclosure or bug report. It is not appendable by other processes without detection, but a
same-user attacker can delete the tail; the vault header stores the last known sequence to detect
truncation.

## 9. Supply chain and release integrity

- `Cargo.lock` and `bun.lock` committed; dependency versions pinned in CI with `--locked`.
- `cargo-audit`, `cargo-deny` (licenses, bans, sources), `cargo-vet` exemptions reviewed per release.
- GitHub Actions pinned to commit SHAs in release jobs; `permissions: contents: read` by default.
- Releases: Authenticode-signed MSI/MSIX/EXE, `SHA256SUMS`, CycloneDX SBOM, ed25519 signatures for the
  updater manifest and each artifact, reproducible-build comparison in CI.
- The updater public key is compiled in; the private key never touches CI logs and lives in an HSM.

## 10. Disclosure policy

Report vulnerabilities privately to `security@rofarm.invalid` (placeholder; replace before release)
with steps and impact. Do not open a public issue for a vulnerability. We aim to acknowledge within
72 hours, provide a remediation plan within 14 days, and credit reporters unless they ask otherwise.
RoFarm has no bug bounty. Please test only against accounts and machines you own.

## 11. Security checklist for contributors

- No `println!`/`tracing` of values typed `Secret*`.
- New IPC command: typed input, capability added to exactly one window, error mapped through `AppError`.
- New crypto: use `rofarm-vault` primitives only; no custom AEAD composition; add property tests.
- New file format: version byte, strict parser, size cap, fuzz target, and a dry-run validator.
- New network host: add to `NetPolicy` and the visible network log, with a reason string.
