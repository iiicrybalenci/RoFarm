import { Show, createSignal } from "solid-js";
import { Modal } from "./Primitives";
import { api, invoke } from "../lib/api";
import { closeReauth, refresh, toast } from "../lib/store";

export function ReauthModal(props: { action: "reveal" | "copy" | "backup" | "change"; id?: string }) {
  const [passphrase, setPassphrase] = createSignal("");
  const [newPassphrase, setNewPassphrase] = createSignal("");
  const [revealed, setRevealed] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const isChange = () => props.action === "change";
  const title = () =>
    props.action === "reveal" ? "Reveal cookie" : props.action === "copy" ? "Copy cookie" : props.action === "change" ? "Change vault passphrase" : "Confirm backup";

  const submit = async () => {
    if (!passphrase()) return;
    setBusy(true);
    try {
      const ok = await api.reauthenticate(passphrase());
      if (!ok) throw new Error("Incorrect passphrase.");
      if (props.action === "reveal" && props.id) {
        const cookie = await api.revealCookie(props.id);
        setRevealed(cookie);
        toast("Cookie revealed", "It is excluded from screenshots. Close this dialog when done.", "warning");
      } else if (props.action === "copy" && props.id) {
        await api.copyCookie(props.id);
        toast("Cookie copied", "The clipboard clears automatically.", "success");
        closeReauth();
      } else if (props.action === "change") {
        if (newPassphrase().length < 10) throw new Error("The new passphrase must be at least 10 characters.");
        await invoke("change_passphrase", { current: passphrase(), next: newPassphrase() });
        toast("Passphrase changed", "Encrypted backups from before this change still use the old passphrase.", "success");
        closeReauth();
      } else {
        closeReauth();
      }
      await refresh();
    } catch (error: any) {
      toast("Re-authentication failed", error.message, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={title()}
      icon={props.action === "change" ? "shield" : "lock"}
      narrow
      onClose={closeReauth}
      footer={
        <Show when={!revealed()} fallback={<button class="btn primary" onClick={closeReauth}>Done</button>}>
          <button class="btn" onClick={closeReauth}>Cancel</button>
          <button class="btn primary" disabled={busy() || !passphrase()} onClick={submit}>{busy() ? "Checking…" : isChange() ? "Change passphrase" : "Confirm"}</button>
        </Show>
      }
    >
      <Show when={!revealed()} fallback={
        <div class="stack">
          <div class="banner danger"><span>This is a live session secret. It is never logged or included in diagnostics bundles.</span></div>
          <textarea class="textarea" style={{ "min-height": "94px" }} readonly value={revealed()!} aria-label="Revealed cookie" />
          <button class="btn sm" onClick={() => navigator.clipboard.writeText(revealed()!)}>Copy to clipboard</button>
        </div>
      }>
        <p class="muted" style={{ margin: 0, "font-size": "0.77rem" }}>
          {props.action === "change"
            ? "Confirm your current passphrase, then choose a new one. This re-wraps every account key."
            : "Re-authentication is required for sensitive actions. Enter your vault passphrase to continue."}
        </p>
        <div class="field">
          <label class="field-label" for="reauth-pass">Vault passphrase</label>
          <input
            id="reauth-pass"
            class="input"
            type="password"
            autocomplete="current-password"
            value={passphrase()}
            onInput={(event) => setPassphrase(event.currentTarget.value)}
            onKeyDown={(event) => { if (event.key === "Enter") void submit(); }}
          />
        </div>
        <Show when={isChange()}>
          <div class="field">
            <label class="field-label" for="reauth-new">New passphrase</label>
            <input id="reauth-new" class="input" type="password" autocomplete="new-password" value={newPassphrase()} onInput={(event) => setNewPassphrase(event.currentTarget.value)} placeholder="At least 10 characters" />
          </div>
        </Show>
        <div class="banner info">Locked or failed attempts back off automatically. Your passphrase never leaves this process.</div>
      </Show>
    </Modal>
  );
}
