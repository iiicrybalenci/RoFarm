import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { Modal } from "./Primitives";
import { Avatar } from "./Primitives";
import { Icon } from "./Icon";
import { api } from "../lib/api";
import { accounts, closeLaunch, filteredAccounts, refresh, settings, snapshot, toast, lanes } from "../lib/store";
import type { Account } from "../lib/types";

type Preview = {
  kind: string;
  place_id?: number | null;
  game_instance_id?: string | null;
  warnings: string[];
  launchable: boolean;
  summary?: string;
  needs_resolution?: boolean;
  confidence?: number;
};

export function LaunchModal(props: { accounts: string[] }) {
  const selected = createMemo<Account[]>(() => props.accounts.map((id) => accounts().find((a) => a.id === id)).filter(Boolean) as Account[]);
  const firstLane = () => lanes().find((lane) => lane.id === selected()[0]?.lane_id);
  const [target, setTarget] = createSignal(firstLane()?.rules?.default_target ?? "");
  const [profileId, setProfileId] = createSignal(Object.keys(snapshot().fleet.profiles)[0] ?? "");
  const [preview, setPreview] = createSignal<Preview | null>(null);
  const [parsing, setParsing] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  let debounce: number | undefined;

  const parse = async (value: string) => {
    if (!value.trim()) {
      setPreview(null);
      return;
    }
    setParsing(true);
    try {
      const result = await api.parseTarget(value);
      setPreview(result);
    } catch (error: any) {
      setPreview({ kind: "invalid", warnings: [error.message], launchable: false });
    } finally {
      setParsing(false);
    }
  };

  onMount(() => {
    if (target()) void parse(target());
  });

  const onTarget = (value: string) => {
    setTarget(value);
    window.clearTimeout(debounce);
    debounce = window.setTimeout(() => void parse(value), 280);
  };

  const launch = async () => {
    if (!target().trim()) {
      toast("Pick a target first", "Paste a game link, place ID, private server link or @username.", "warning");
      return;
    }
    if (!snapshot().vault.unlocked) {
      toast("Vault is locked", "Unlock the vault before launching clients.", "warning");
      return;
    }
    setBusy(true);
    try {
      const result: any = await api.launch(props.accounts, target(), profileId() || undefined);
      const failures = (result?.outcomes ?? []).filter((outcome: any) => outcome.error);
      if (failures.length) {
        toast("Some accounts failed to launch", failures.map((f: any) => f.error).join(" · "), "error");
      } else {
        toast("Launch queued", `${props.accounts.length} client${props.accounts.length === 1 ? "" : "s"} starting with stagger.`, "success");
      }
      closeLaunch();
      await refresh();
    } catch (error: any) {
      toast("Launch failed", error.message, "error");
    } finally {
      setBusy(false);
    }
  };

  const profiles = () => Object.values(snapshot().fleet.profiles);

  return (
    <Modal
      title={`Launch ${props.accounts.length} account${props.accounts.length === 1 ? "" : "s"}`}
      icon="rocket"
      onClose={closeLaunch}
      wide
      footer={
        <>
          <button class="btn" onClick={closeLaunch}>Cancel</button>
          <button class="btn primary" disabled={busy() || !target().trim()} onClick={launch}>
            <Icon name={busy() ? "refresh" : "play"} size={13} />
            {busy() ? "Launching…" : "Launch"}
          </button>
        </>
      }
    >
      <div class="row" style={{ gap: "5px" }}>
        <For each={selected()}>
          {(account) => (
            <span class="pill" title={account.username} style={{ gap: "5px" }}>
              <Avatar account={account} size={18} presence={false} />
              {account.username}
            </span>
          )}
        </For>
        <span class="muted tiny" style={{ "margin-left": "auto" }}>{selected()[0]?.lane_id ? lanes().find((l) => l.id === selected()[0].lane_id)?.name : ""}</span>
      </div>
      <div class="field">
        <label class="field-label" htmlFor="launch-target">Game link, place ID, private server or @friend</label>
        <input
          id="launch-target"
          class="input"
          value={target()}
          onInput={(event) => onTarget(event.currentTarget.value)}
          placeholder="https://www.roblox.com/games/920587237/Adopt-Me"
          spellcheck={false}
        />
      </div>
      <Show when={preview()}>
        {(value) => (
          <div class="stack">
            <div class={`banner ${value().launchable ? "info" : "danger"}`}>
              <Icon name={value().launchable ? "check" : "warning"} size={14} />
              <div>
                <strong>{value().summary ?? (value().place_id ? `Place ${value().place_id}` : "Unrecognized target")}</strong>
                <div class="tiny" style={{ "margin-top": "2px" }}>
                  {value().place_id ? `place ${value().place_id}` : "No place ID"}
                  {value().game_instance_id ? ` · server ${value().game_instance_id!.slice(0, 12)}…` : ""}
                  {value().confidence !== undefined ? ` · confidence ${Math.round((value().confidence ?? 0) * 100)}%` : ""}
                </div>
              </div>
              <span class="grow" />
              <Show when={parsing()}><span class="tiny muted">parsing…</span></Show>
            </div>
            <Show when={value().warnings?.length}>
              <div class="tiny muted" style={{ "font-size": "0.7rem" }}>{value().warnings.join(" · ")}</div>
            </Show>
          </div>
        )}
      </Show>
      <div class="row" style={{ "align-items": "flex-start" }}>
        <div class="field grow">
          <label class="field-label" htmlFor="launch-profile">Launch profile</label>
          <select id="launch-profile" class="select" value={profileId()} onChange={(event) => setProfileId(event.currentTarget.value)}>
            <option value="">Default (lane rules)</option>
            <For each={profiles()}>{(profile) => <option value={profile.id}>{profile.name}</option>}</For>
          </select>
          <div class="tiny muted" style={{ "margin-top": "4px" }}>
            Profile controls FPS cap, graphics, limits, window placement, stagger and retry rules.
          </div>
        </div>
        <div class="field" style={{ "min-width": "160px" }}>
          <span class="field-label">Resource profile</span>
          <span class="pill info" style={{ "align-self": "flex-start" }}>{settings().resource_profile}</span>
        </div>
      </div>
      <Show when={!snapshot().vault.unlocked}>
        <div class="banner danger"><Icon name="lock" size={14} /><span>The vault is locked. Unlock it in the Vault view before launching.</span></div>
      </Show>
      <div class="banner info"><Icon name="shield" size={14} /><span>Roblox's own signed launcher handles sign-in. RoFarm attaches limits once the process appears, and never injects code.</span></div>
    </Modal>
  );
}
