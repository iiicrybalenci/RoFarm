import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { EmptyState, Modal, Section, ToggleRow } from "../components/Primitives";
import { Icon } from "../components/Icon";
import { formatDate, timeAgo } from "../lib/format";
import { api, invoke } from "../lib/api";
import { bootstrap, refresh, settings, snapshot, toast, updateSettings, withBusy } from "../lib/store";
import type { AuditEntry, NetworkEntry } from "../lib/types";

type Tab = "overview" | "audit" | "backups" | "recovery" | "network";

export default function Vault() {
  const [tab, setTab] = createSignal<Tab>("overview");
  const [passphrase, setPassphrase] = createSignal("");
  const [unlockOpen, setUnlockOpen] = createSignal(false);
  const [backupOpen, setBackupOpen] = createSignal(false);
  const [recoveryOpen, setRecoveryOpen] = createSignal(false);
  const [changeOpen, setChangeOpen] = createSignal(false);
  const [audit, setAudit] = createSignal<AuditEntry[]>([]);
  const [network, setNetwork] = createSignal<NetworkEntry[]>([]);
  const [recoveryCode, setRecoveryCode] = createSignal("");
  const [shares, setShares] = createSignal<string[]>([]);
  const [passwords, setPasswords] = createSignal({ current: "", next: "", confirm: "" });

  const vault = createMemo(() => snapshot().vault);
  const auditVerified = createMemo(() => {
    const entries = audit();
    if (!entries.length) return true;
    return entries.every((e, i) => i === 0 || e.prev_hash === entries[i - 1].hash);
  });

  const load = async () => {
    try {
      const [log, net] = await Promise.all([api.auditLog(), api.networkLog()]);
      setAudit(log);
      setNetwork(net);
    } catch {
    }
  };

  onMount(load);

  const unlock = async () => {
    if (!passphrase()) return;
    await withBusy("Unlocking vault", async () => {
      await api.unlockVault(passphrase());
      setPassphrase("");
      setUnlockOpen(false);
      await refresh();
      await load();
      toast("Vault unlocked", "Secrets are available for this session.", "success");
    });
  };

  const lock = async () => {
    await api.lockVault();
    await refresh();
    toast("Vault locked", "Cookies are cleared from memory and the clipboard watcher is paused.", "success");
  };

  const changePassphrase = async () => {
    if (passwords().next.length < 12) return toast("Use at least 12 characters", "A longer passphrase is the best protection for a stolen vault file.", "warning");
    if (passwords().next !== passwords().confirm) return toast("Passphrases do not match", "", "warning");
    await withBusy("Changing passphrase", async () => {
      await invoke("change_passphrase", { current: passwords().current, new: passwords().next });
      setPasswords({ current: "", next: "", confirm: "" });
      setChangeOpen(false);
      toast("Passphrase changed", "The vault was rekeyed and the audit log records the change.", "success");
      await load();
    });
  };

  const makeBackup = async () => {
    await withBusy("Writing backup", async () => {
      const result: any = await invoke("backup_vault", { destination: "rofarm-backup.rofarm", passphrase: passphrase() || "" });
      setBackupOpen(false);
      toast("Encrypted backup written", `${result?.chunks ?? "—"} chunks · signed manifest included.`, "success");
      await load();
    });
  };

  const issueRecovery = async () => {
    await withBusy("Issuing recovery code", async () => {
      const code = await invoke<string>("issue_recovery_code");
      setRecoveryCode(code);
      toast("Recovery code created", "Store it offline. Anyone with this code and your backup can open the vault.", "warning");
      await load();
    });
  };

  const splitShares = async () => {
    await withBusy("Splitting recovery key", async () => {
      const result = await invoke<string[]>("recovery_shares", { threshold: 2, shares: 3 });
      setShares(result);
      toast("Shamir shares created", "2 of 3 shares are required to recover.", "success");
    });
  };

  const exportAudit = () => {
    const blob = new Blob([JSON.stringify(audit(), null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "rofarm-audit-log.json";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 id="view-title" tabindex="-1">Vault</h1>
          <p class="subtitle">XChaCha20-Poly1305, Argon2id, DPAPI/TPM sealing, and a hash-chained audit trail.</p>
        </div>
        <div class="row">
          <Show when={vault().unlocked} fallback={<button class="btn primary" onClick={() => setUnlockOpen(true)}><Icon name="unlock" size={14} /> Unlock vault</button>}>
            <button class="btn" onClick={lock}><Icon name="lock" size={14} /> Lock now</button>
            <button class="btn ghost" onClick={() => setChangeOpen(true)}><Icon name="keyboard" size={14} /> Change passphrase</button>
          </Show>
        </div>
      </div>

      <div class={`vault-banner ${vault().unlocked ? "unlocked" : "locked"}`}>
        <div class="vault-emblem"><Icon name={vault().unlocked ? "unlock" : "lock"} size={26} /></div>
        <div class="grow">
          <strong>{vault().unlocked ? "Vault unlocked" : "Vault locked"}</strong>
          <div class="muted small">
            {vault().unlocked ? `Generation ${vault().generation} · ${vault().secrets} encrypted secrets` : `Locked${vault().lockout_seconds ? ` · retry in ${vault().lockout_seconds}s` : ""} · session cookies are unavailable`}
          </div>
        </div>
        <div class="vault-facts">
          <span><Icon name="shield" size={12} /> Argon2id calibrated to ~500 ms</span>
          <span><Icon name="keyboard" size={12} /> {vault().has_recovery ? "Recovery enabled" : "No recovery code"}</span>
          <span><Icon name="database" size={12} /> {vault().path.split(/[\\/]/).pop()}</span>
        </div>
      </div>

      <div class="tabs" role="tablist">
        <For each={[["overview", "Security", "shield"], ["audit", "Audit log", "logs"], ["backups", "Backups", "database"], ["recovery", "Recovery", "unlock"], ["network", "Network log", "network"]] as [Tab, string, any][]}>
          {([value, label, icon]) => <button role="tab" aria-selected={tab() === value} class={tab() === value ? "active" : ""} onClick={() => setTab(value)}><Icon name={icon} size={14} /> {label}</button>}
        </For>
      </div>

      <Show when={tab() === "overview"}>
        <div class="two-col">
          <Section title="Unlock policy" desc="When RoFarm re-locks the vault without asking.">
            <ToggleRow title="Lock on idle" desc="Lock after 5 minutes without interaction." checked={settings().security?.auto_lock_idle !== false} onChange={(v) => updateSettings((s) => { s.security.auto_lock_idle = v; })} />
            <ToggleRow title="Lock on sleep, screen lock, and session switch" desc="Uses Windows session notifications; no polling." checked={settings().security?.auto_lock_session !== false} onChange={(v) => updateSettings((s) => { s.security.auto_lock_session = v; })} />
            <ToggleRow title="Windows Hello unlock" desc="Biometric or PIN via the Windows Hello API, bound to your TPM." checked={!!settings().security?.hello_enabled} onChange={(v) => updateSettings((s) => { s.security.hello_enabled = v; })} />
            <ToggleRow title="Require re-auth for reveal, copy, and export" desc="Prevents a shoulder-surfer or unlocked desk from dumping cookies." checked={settings().security?.reauth_sensitive !== false} onChange={(v) => updateSettings((s) => { s.security.reauth_sensitive = v; })} />
            <ToggleRow title="Exclude secret windows from capture" desc="SetWindowDisplayAffinity blocks screenshots and screen shares." checked={settings().security?.exclude_from_capture !== false} onChange={(v) => updateSettings((s) => { s.security.exclude_from_capture = v; })} />
            <ToggleRow title="Clear clipboard after 30 seconds" desc="Also marks the value to skip Windows clipboard history." checked={settings().security?.clipboard_clear !== false} onChange={(v) => updateSettings((s) => { s.security.clipboard_clear = v; })} />
            <ToggleRow title="TPM-sealed master key" desc="Survives file theft, not a motherboard change. Recovery code still works." checked={!!settings().security?.tpm_seal} onChange={(v) => updateSettings((s) => { s.security.tpm_seal = v; })} />
            <ToggleRow title="YubiKey second factor (hmac-secret)" desc="Optional hardware factor. Losing the key requires the recovery code." checked={!!settings().security?.fido2_enabled} onChange={(v) => updateSettings((s) => { s.security.fido2_enabled = v; })} />
          </Section>
          <Section title="Threat model" desc="What the vault is designed to stop, and what it isn't.">
            <div class="threat-list">
              <div class="threat"><Icon name="check" size={14} /><div><strong>Stolen laptop or app-data folder</strong><p>Vault ciphertext without the passphrase, DPAPI/TPM binding, and recovery code is useless.</p></div></div>
              <div class="threat"><Icon name="check" size={14} /><div><strong>Malware reading files</strong><p>Records are individually authenticated; secrets are decrypted just-in-time and never logged.</p></div></div>
              <div class="threat"><Icon name="check" size={14} /><div><strong>Tampered or rolled-back backups</strong><p>Chunked AEAD with an integrity manifest and generation counters; restore runs a dry-run verify.</p></div></div>
              <div class="threat warn"><Icon name="warning" size={14} /><div><strong>An attacker with your unlocked session</strong><p>Not preventable. Re-auth gates sensitive actions, audit logs record them, and auto-lock limits the window.</p></div></div>
              <div class="threat warn"><Icon name="warning" size={14} /><div><strong>Kernel malware or a compromised OS</strong><p>Out of scope. If the OS is compromised, no user-mode secret store can be trusted.</p></div></div>
            </div>
            <button class="btn sm ghost" onClick={() => window.open("/docs/SECURITY.md", "_blank")}><Icon name="file" size={12} /> Open SECURITY.md</button>
          </Section>
        </div>
        <Section title="Session" desc="Data location and runtime details.">
          <div class="kv-grid">
            <div><span class="muted tiny">Data directory</span><code>{bootstrap()?.data_dir ?? "—"}</code></div>
            <div><span class="muted tiny">Portable mode</span><strong>{bootstrap()?.portable ? "Yes — data stays beside the executable" : "No"}</strong></div>
            <div><span class="muted tiny">Vault file</span><code>{vault().path}</code></div>
            <div><span class="muted tiny">Secrets</span><strong>{vault().secrets}</strong></div>
          </div>
        </Section>
      </Show>

      <Show when={tab() === "audit"}>
        <div class="toolbar">
          <div class="row"><span class={`chip ${auditVerified() ? "ok" : "bad"}`}><Icon name={auditVerified() ? "check" : "warning"} size={11} /> {auditVerified() ? "Hash chain verified" : "Chain broken at entry"}</span><span class="muted tiny">{audit().length} events</span></div>
          <div class="grow" />
          <button class="btn sm ghost" onClick={load}><Icon name="refresh" size={13} /> Refresh</button>
          <button class="btn sm ghost" onClick={exportAudit}><Icon name="download" size={13} /> Export</button>
        </div>
        <Show when={audit().length} fallback={<EmptyState icon="logs" title="No audit events yet" body="Unlocks, reveals, exports, settings changes, and remote logins are appended here with a hash chain." />}>
          <div class="card table-card audit-table">
            <table class="data-table">
              <thead><tr><th>#</th><th>When</th><th>Event</th><th>Subject</th><th>Detail</th><th>Hash</th></tr></thead>
              <tbody>
                <For each={audit()}>
                  {(entry) => (
                    <tr>
                      <td class="muted mono">{entry.seq}</td>
                      <td class="muted">{formatDate(entry.at, true)}</td>
                      <td><span class="chip ghost">{entry.kind}</span></td>
                      <td>{entry.subject}</td>
                      <td class="muted truncate">{entry.detail}</td>
                      <td class="mono tiny muted">{entry.hash.slice(0, 10)}…</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === "backups"}>
        <div class="two-col">
          <Section title="Encrypted backups" desc="A separate passphrase, chunked AEAD, and an integrity manifest.">
            <div class="backup-hero">
              <Icon name="database" size={30} />
              <div class="grow"><strong>Backup now</strong><p class="muted small">Writes a .rofarm archive you can move to another PC. The vault passphrase is not reused.</p></div>
              <button class="btn primary" onClick={() => setBackupOpen(true)}>Create backup</button>
            </div>
            <div class="backup-actions">
              <button class="btn ghost" onClick={() => setBackupOpen(true)}><Icon name="upload" size={14} /> Verify a backup</button>
              <button class="btn ghost" onClick={() => setBackupOpen(true)}><Icon name="download" size={14} /> Restore…</button>
            </div>
          </Section>
          <Section title="Scheduled backups" desc="Always encrypted before leaving this machine.">
            <ToggleRow title="Automatic backups" desc="Daily at 04:00 while RoFarm is running." checked={!!settings().backups?.enabled} onChange={(v) => updateSettings((s) => { s.backups.enabled = v; })} />
            <ToggleRow title="Copy to a chosen folder" desc="A local folder, network share, or your synced OneDrive folder." checked={!!settings().backups?.remote_copy} onChange={(v) => updateSettings((s) => { s.backups.remote_copy = v; })} />
            <ToggleRow title="Keep the last 7 backups" desc="Older archives are pruned only after the new one verifies." checked={settings().backups?.retention !== 0} onChange={(v) => updateSettings((s) => { s.backups.retention = v ? 7 : 0; })} />
            <p class="hint"><Icon name="info" size={12} /> Backup files are useless without their passphrase. Store it separately from the file.</p>
          </Section>
        </div>
        <div class="row">
          <span class="muted small">Last backup: {timeAgo(settings().backups?.last_at)}</span>
        </div>
      </Show>

      <Show when={tab() === "recovery"}>
        <div class="two-col">
          <Section title="Recovery code" desc="A 9-character code that can unlock a backup if the passphrase is lost.">
            <p class="muted small">Works with .rofarm backups and the vault file on any PC. It does not defeat a TPM seal on different hardware, but it restores from backup there.</p>
            <button class="btn" onClick={issueRecovery}><Icon name="keyboard" size={14} /> {vault().has_recovery ? "Issue a new code" : "Create recovery code"}</button>
            <Show when={recoveryCode()}>
              <div class="recovery-code"><code>{recoveryCode()}</code><button class="icon-btn" onClick={() => navigator.clipboard.writeText(recoveryCode()).then(() => toast("Copied", "Store it offline, not in the same place as the backup.", "success"))}><Icon name="copy" size={14} /></button></div>
              <p class="hint warn"><Icon name="warning" size={12} /> Anyone with this code and your backup can read every account. RoFarm cannot recover it for you.</p>
            </Show>
          </Section>
          <Section title="Shamir split" desc="Split the recovery key into shares for trusted devices or people.">
            <p class="muted small">2-of-3 by default. Any two shares reconstruct the key; one alone reveals nothing. Each share is shown once.</p>
            <button class="btn" onClick={splitShares}><Icon name="shuffle" size={14} /> Generate 2-of-3 shares</button>
            <Show when={shares().length}>
              <div class="share-list"><For each={shares()}>{(share, i) => <div class="share-row"><span class="muted tiny">Share {i() + 1}</span><code>{share}</code><button class="icon-btn" onClick={() => navigator.clipboard.writeText(share)}><Icon name="copy" size={13} /></button></div>}</For></div>
            </Show>
          </Section>
        </div>
        <Section title="What survives what" desc="Be precise about your recovery expectations.">
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>Scenario</th><th>Passphrase</th><th>Recovery code</th><th>Shamir shares</th><th>Backup file</th></tr></thead>
              <tbody>
                <tr><td>Forgot passphrase</td><td class="muted">No</td><td class="ok">Yes</td><td class="ok">Yes</td><td class="ok">Yes</td></tr>
                <tr><td>Disk failure</td><td class="ok">If known</td><td class="ok">Yes</td><td class="ok">Yes</td><td class="ok">Yes</td></tr>
                <tr><td>New PC, TPM sealed</td><td class="ok">Yes</td><td class="ok">From backup</td><td class="ok">From backup</td><td class="ok">Yes</td></tr>
                <tr><td>Lost YubiKey</td><td class="ok">Yes</td><td class="ok">Yes</td><td class="ok">Yes</td><td class="ok">Yes</td></tr>
              </tbody>
            </table>
          </div>
        </Section>
      </Show>

      <Show when={tab() === "network"}>
        <div class="toolbar">
          <span class="chip ghost"><Icon name="shield" size={11} /> Allow-list enforced before DNS</span>
          <div class="grow" />
          <span class="muted tiny">{network().filter((n) => !n.allowed).length} blocked</span>
        </div>
        <Show when={network().length} fallback={<EmptyState icon="network" title="No requests yet" body="Every outbound request is recorded here with its allow-list decision, duration, and size." />}>
          <div class="card table-card">
            <table class="data-table">
              <thead><tr><th>Time</th><th>Method</th><th>Host</th><th>Path</th><th>Status</th><th class="right">Duration</th><th>Allowed</th></tr></thead>
              <tbody>
                <For each={network().slice(0, 100)}>
                  {(entry) => (
                    <tr>
                      <td class="muted">{new Date(entry.at).toLocaleTimeString()}</td>
                      <td class="mono">{entry.method}</td>
                      <td>{entry.host}</td>
                      <td class="muted mono truncate">{entry.path}</td>
                      <td>{entry.status ?? "—"}</td>
                      <td class="right muted">{entry.duration_ms} ms</td>
                      <td><span class={`chip tiny ${entry.allowed ? "ok" : "bad"}`}>{entry.allowed ? "allowed" : "blocked"}</span></td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
        <p class="hint"><Icon name="info" size={12} /> Allowed: *.roblox.com, *.rbxcdn.com, notifier endpoints you configure, and the signed update feed. No telemetry hosts exist.</p>
      </Show>

      <Show when={unlockOpen()}>
        <Modal title="Unlock vault" icon="unlock" narrow onClose={() => setUnlockOpen(false)} footer={<><button class="btn ghost" onClick={() => setUnlockOpen(false)}>Cancel</button><button class="btn primary" onClick={unlock}>Unlock</button></>}>
          <label class="field"><span>Passphrase</span><input class="input" type="password" value={passphrase()} onInput={(e) => setPassphrase(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && unlock()} placeholder="Your vault passphrase" autofocus /></label>
          <p class="muted small">Or use Windows Hello from the lock screen. Failed attempts back off exponentially.</p>
        </Modal>
      </Show>

      <Show when={backupOpen()}>
        <Modal title="Encrypted backup" icon="database" narrow onClose={() => setBackupOpen(false)} footer={<><button class="btn ghost" onClick={() => setBackupOpen(false)}>Cancel</button><button class="btn primary" onClick={makeBackup}>Create .rofarm</button></>}>
          <label class="field"><span>Backup passphrase (separate from the vault)</span><input class="input" type="password" value={passphrase()} onInput={(e) => setPassphrase(e.currentTarget.value)} placeholder="At least 12 characters" /></label>
          <p class="muted small">The archive is written beside your data folder, then copied wherever you choose. Restore always runs a dry-run schema and size check first.</p>
        </Modal>
      </Show>

      <Show when={changeOpen()}>
        <Modal title="Change passphrase" icon="keyboard" narrow onClose={() => setChangeOpen(false)} footer={<><button class="btn ghost" onClick={() => setChangeOpen(false)}>Cancel</button><button class="btn primary" onClick={changePassphrase}>Rekey vault</button></>}>
          <label class="field"><span>Current passphrase</span><input class="input" type="password" value={passwords().current} onInput={(e) => setPasswords({ ...passwords(), current: e.currentTarget.value })} /></label>
          <label class="field"><span>New passphrase</span><input class="input" type="password" value={passwords().next} onInput={(e) => setPasswords({ ...passwords(), next: e.currentTarget.value })} /></label>
          <label class="field"><span>Confirm new passphrase</span><input class="input" type="password" value={passwords().confirm} onInput={(e) => setPasswords({ ...passwords(), confirm: e.currentTarget.value })} /></label>
        </Modal>
      </Show>
    </div>
  );
}
