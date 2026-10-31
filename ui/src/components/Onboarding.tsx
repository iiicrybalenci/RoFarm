import { For, Show, createSignal } from "solid-js";
import { Icon } from "./Icon";
import {
  closeOnboarding,
  settings,
  updateSettings,
  saveSettingsNow,
  snapshot,
  refresh,
  toast,
  openAddAccount,
} from "../lib/store";
import { api, invoke } from "../lib/api";
import { t } from "../lib/i18n";
import { Segmented } from "./Primitives";

const steps = ["Welcome", "Vault", "Accounts", "Performance", "Ready"];

export function Onboarding() {
  const [step, setStep] = createSignal(0);
  const [method, setMethod] = createSignal<"passphrase" | "tpm" | "hello">("passphrase");
  const [passphrase, setPassphrase] = createSignal("");
  const [confirm, setConfirm] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [importText, setImportText] = createSignal("");
  const [imported, setImported] = createSignal<number | null>(null);
  const [profile, setProfile] = createSignal<"eco" | "balanced" | "performance">(settings().resource_profile === "custom" ? "balanced" : settings().resource_profile as any);
  const vault = () => snapshot().vault;

  const finish = async () => {
    updateSettings((draft) => {
      draft.onboarding = { ...(draft.onboarding ?? {}), completed: true, step: 4, tour_seen: true };
      draft.resource_profile = profile();
      draft.start_with_windows = draft.start_with_windows ?? false;
    }, false);
    await saveSettingsNow();
    closeOnboarding();
    toast("Welcome to RoFarm", "Your Command Deck is ready. Press Ctrl+K to do anything.", "success");
  };

  const unlock = async () => {
    if (passphrase().length < 10) {
      toast("Use at least 10 characters", "A longer passphrase is the best protection for your cookies.", "warning");
      return;
    }
    if (passphrase() !== confirm()) {
      toast("Passphrases do not match", "", "error");
      return;
    }
    setBusy(true);
    try {
      if (vault().exists) {
        try {
          await api.unlockVault(passphrase());
        } catch {
          await api.createVault(passphrase());
        }
      } else {
        await api.createVault(passphrase());
      }
      await refresh();
      setStep(2);
    } catch (error: any) {
      toast("Could not open the vault", error.message, "error");
    } finally {
      setBusy(false);
    }
  };

  const importAccounts = async () => {
    if (!importText().trim()) {
      setStep(3);
      return;
    }
    setBusy(true);
    try {
      const result: any = await invoke("import_accounts", { text: importText() });
      const count = result?.imported ?? result?.credentials ?? 0;
      setImported(count);
      await refresh();
      toast("Accounts imported", `${count} credential(s) encrypted in your vault.`, "success");
      setStep(3);
    } catch (error: any) {
      toast("Import failed", error.message, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="onboarding" role="dialog" aria-modal="true" aria-label="Welcome to RoFarm">
      <div class="onboarding-card">
        <div class="onboarding-visual">
          <div class="ghost-card" />
          <div class="ghost-card" />
          <div class="ghost-card" />
          <div style={{ position: "absolute", "z-index": 2, bottom: "12px", left: "16px", display: "flex", gap: "6px", "align-items": "center", "font-size": "0.7rem", color: "rgba(255,255,255,0.72)" }}>
            <Icon name="shield" size={13} /> Local-first · no telemetry
          </div>
        </div>
        <div class="onboarding-body">
          <div class="onboarding-steps">
            <For each={steps}>{(_, index) => <span classList={{ done: index() <= step() }} />}</For>
          </div>
          <Show when={step() === 0}>
            <h1>{t("onboarding.welcome")}</h1>
            <p>
              A fast, local-first command deck for the Roblox accounts you own. Drag accounts between lanes, launch many clients at once,
              cap their resources and let the supervisor restart the ones that matter. No cloud, no account, no telemetry.
            </p>
            <div class="choice-grid">
              <div class="choice active"><strong>You stay in control</strong><span>Every secret is encrypted locally. Nothing leaves this PC unless you configure a notifier.</span></div>
              <div class="choice"><strong>Windows-native</strong><span>Job objects, DWM thumbnails, ETW events, EcoQoS and Task Scheduler integration.</span></div>
              <div class="choice"><strong>Respectful by design</strong><span>No injection, no memory reads, no in-game automation. Only documented APIs and normal input.</span></div>
            </div>
          </Show>
          <Show when={step() === 1}>
            <h1>{t("onboarding.vault")}</h1>
            <p>RoFarm encrypts cookies with XChaCha20-Poly1305 and an Argon2id key calibrated on this machine. Choose how the vault unlocks.</p>
            <Show when={vault().exists && vault().unlocked} fallback={
              <>
                <div class="choice-grid" style={{ "margin-bottom": "14px" }}>
                  <button class="choice" classList={{ active: method() === "passphrase" }} onClick={() => setMethod("passphrase")}>
                    <strong>Passphrase</strong><span>Works everywhere, survives hardware changes, can be backed up.</span>
                  </button>
                  <button class="choice" classList={{ active: method() === "tpm" }} onClick={() => setMethod("tpm")}>
                    <strong>TPM + passphrase</strong><span>Sealed to this PC's TPM. Does not survive a motherboard change.</span>
                  </button>
                  <button class="choice" classList={{ active: method() === "hello" }} onClick={() => setMethod("hello")}>
                    <strong>Windows Hello</strong><span>Unlock with face, fingerprint or PIN. Falls back to the passphrase.</span>
                  </button>
                </div>
                <div class="stack">
                  <div class="field">
                    <label class="field-label" for="onboard-pass">Vault passphrase</label>
                    <input id="onboard-pass" class="input" type="password" autocomplete="new-password" value={passphrase()} onInput={(e) => setPassphrase(e.currentTarget.value)} placeholder="At least 10 characters" />
                  </div>
                  <div class="field">
                    <label class="field-label" for="onboard-confirm">Confirm passphrase</label>
                    <input id="onboard-confirm" class="input" type="password" autocomplete="new-password" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} placeholder="Type it again" />
                  </div>
                  <div class="banner info"><Icon name="info" size={14} /><span>Binding to {method() === "tpm" ? "the TPM" : method() === "hello" ? "Windows Hello" : "your passphrase"} can be added later in Vault settings. A recovery code is offered after setup.</span></div>
                </div>
              </>
            }>
              <div class="banner"><Icon name="check" size={15} /><span>Your vault is open. You can continue; backup and recovery options live in the Vault view.</span></div>
            </Show>
          </Show>
          <Show when={step() === 2}>
            <h1>{t("onboarding.import")}</h1>
            <p>Paste cookies from Roblox Account Manager or RoFarm backups, one per line. You can also add accounts one at a time later. Secrets are encrypted immediately and never logged.</p>
            <textarea
              class="textarea"
              style={{ "min-height": "130px" }}
              placeholder={"username<TAB>_|WARNING:-DO-NOT-SHARE|_|…\nusername:_|WARNING:-DO-NOT-SHARE|_|…"}
              value={importText()}
              onInput={(event) => setImportText(event.currentTarget.value)}
              spellcheck={false}
            />
            <div class="row" style={{ "margin-top": "10px" }}>
              <button class="btn" onClick={() => { closeOnboarding(); openAddAccount(); }}><Icon name="plus" size={13} />Add one account</button>
              <Show when={imported() !== null}><span class="pill ok">{imported()} imported</span></Show>
              <span class="grow" />
              <span class="muted tiny">Skips blank lines and duplicates. Invalid lines are reported without exposing cookies.</span>
            </div>
          </Show>
          <Show when={step() === 3}>
            <h1>{t("onboarding.profile")}</h1>
            <p>Resource profiles set CPU caps, memory limits, FPS, graphics and EcoQoS for every client. You can change this per lane later.</p>
            <div class="stack">
              <Segmented
                value={profile()}
                onChange={(next) => setProfile(next)}
                options={[
                  { value: "eco", label: "Eco" },
                  { value: "balanced", label: "Balanced" },
                  { value: "performance", label: "Performance" },
                ]}
              />
              <div class="choice-grid">
                <button class="choice" classList={{ active: profile() === "eco" }} onClick={() => setProfile("eco")}><strong>Eco</strong><span>15 FPS, 20% CPU cap, memory trim, Maximum efficiency. Best for many alts on a laptop.</span></button>
                <button class="choice" classList={{ active: profile() === "balanced" }} onClick={() => setProfile("balanced")}><strong>Balanced</strong><span>30 FPS, sensible caps, EcoQoS. The default for most fleets.</span></button>
                <button class="choice" classList={{ active: profile() === "performance" }} onClick={() => setProfile("performance")}><strong>Performance</strong><span>60 FPS and above-normal priority for testing or a powerful desktop.</span></button>
              </div>
              <div class="banner"><Icon name="cpu" size={14} /><span>RoFarm never raises client priority above normal for farming lanes. You can override this per lane.</span></div>
            </div>
          </Show>
          <Show when={step() === 4}>
            <h1>{t("onboarding.done")}</h1>
            <p>Your Command Deck is ready. Press <span class="kbd">Ctrl K</span> for anything, drag cards between lanes, and hold the red button to stop every client.</p>
            <div class="choice-grid">
              <div class="choice"><strong>Board</strong><span>Drag accounts between Farming, Idle, Trading and Testing. Lane rules apply instantly.</span></div>
              <div class="choice"><strong>Fleet</strong><span>Live thumbnails, per-client metrics and controls. Pop any view into its own window.</span></div>
              <div class="choice"><strong>Vault</strong><span>Reveal or copy cookies only after re-authentication. Encrypted backups are one click away.</span></div>
            </div>
            <div class="banner info" style={{ "margin-top": "12px" }}><Icon name="info" size={14} /><span>Independent project, not affiliated with Roblox Corporation. Use only accounts you own and follow Roblox's Terms of Use.</span></div>
          </Show>
        </div>
        <div class="onboarding-foot">
          <button class="btn ghost" onClick={() => { void finish(); }}>{t("onboarding.skip")}</button>
          <span class="spacer" />
          <Show when={step() > 0}><button class="btn" onClick={() => setStep(step() - 1)}>{t("onboarding.back")}</button></Show>
          <Show when={step() === 1}>
            <button class="btn primary" disabled={busy()} onClick={() => (vault().exists && vault().unlocked ? setStep(2) : unlock())}>
              {busy() ? "Working…" : vault().exists && vault().unlocked ? "Continue" : "Create vault"}
            </button>
          </Show>
          <Show when={step() === 2}>
            <button class="btn primary" disabled={busy()} onClick={importAccounts}>{busy() ? "Encrypting…" : importText().trim() ? "Import & continue" : t("onboarding.next")}</button>
          </Show>
          <Show when={step() === 3}>
            <button class="btn primary" onClick={() => setStep(4)}>{t("onboarding.next")}</button>
          </Show>
          <Show when={step() === 0}>
            <button class="btn primary" onClick={() => setStep(1)}>{t("onboarding.next")}</button>
          </Show>
          <Show when={step() === 4}>
            <button class="btn primary" onClick={finish}>Open Command Deck</button>
          </Show>
        </div>
      </div>
    </div>
  );
}
