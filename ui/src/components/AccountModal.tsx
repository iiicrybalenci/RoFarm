import { For, Show, createSignal } from "solid-js";
import { Modal, Segmented } from "./Primitives";
import { Icon } from "./Icon";
import { api, invoke } from "../lib/api";
import { accounts, closeAccountDialog, lanes, refresh, toast, openAccount, snapshot } from "../lib/store";

const noteTemplates = [
  { label: "Farming", text: "Goal:\nSchedule:\nNotes:\n" },
  { label: "Trading", text: "Inventory value:\nWanted:\nTrades:\n" },
  { label: "Bank", text: "Never trade.\nNever join unknown games.\nVerified:\n" },
];

export function AccountModal(props: { mode: "add" | "edit"; id?: string }) {
  const existing = () => (props.id ? accounts().find((a) => a.id === props.id) : undefined);
  const [tab, setTab] = createSignal<"single" | "bulk">("single");
  const [username, setUsername] = createSignal(existing()?.username ?? "");
  const [displayName, setDisplayName] = createSignal(existing()?.display_name ?? "");
  const [alias, setAlias] = createSignal(existing()?.alias ?? "");
  const [laneId, setLaneId] = createSignal(existing()?.lane_id ?? lanes()[0]?.id ?? "");
  const [tags, setTags] = createSignal((existing()?.tags ?? []).join(", "));
  const [notes, setNotes] = createSignal(existing()?.notes ?? "");
  const [cookie, setCookie] = createSignal("");
  const [favourite, setFavourite] = createSignal(existing()?.favourite ?? false);
  const [bulk, setBulk] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const isEdit = () => props.mode === "edit";

  const save = async () => {
    setError(null);
    if (tab() === "bulk") {
      if (!bulk().trim()) return setError("Paste at least one credential.");
      setBusy(true);
      try {
        const result: any = await invoke("import_accounts", { text: bulk() });
        toast("Import finished", `${result.imported ?? 0} imported · ${result.duplicates ?? 0} duplicates · ${result.issues ?? 0} issues`, "success");
        closeAccountDialog();
        await refresh();
      } catch (err: any) {
        setError(err.message);
      } finally {
        setBusy(false);
      }
      return;
    }
    if (!isEdit() && !/^[A-Za-z0-9_]{3,32}$/.test(username().trim())) {
      setError("Username must be 3–32 letters, numbers or underscores.");
      return;
    }
    setBusy(true);
    try {
      const tagList = tags().split(",").map((tag) => tag.trim()).filter(Boolean);
      if (isEdit() && props.id) {
        const account = existing()!;
        await api.updateAccount(props.id, {
          username: username().trim(),
          display_name: displayName() || username().trim(),
          alias: alias() || null,
          notes: notes(),
          tags: tagList,
          favourite: favourite(),
        });
        if (laneId() !== account.lane_id) await api.moveAccount(account.id, laneId());
        if (cookie().trim()) await invoke("store_cookie", { id: account.id, cookie: cookie().trim() });
        toast("Account updated", `@${username()}`, "success");
      } else {
        const account: any = await api.addAccount(
          {
            username: username().trim(),
            display_name: displayName() || username().trim(),
            alias: alias() || null,
            lane_id: laneId(),
            notes: notes(),
            tags: tagList,
            favourite: favourite(),
          },
          cookie().trim() || undefined,
        );
        toast("Account added", cookie().trim() ? "@" + username() + " is encrypted and ready to validate." : "@" + username() + " added without a cookie.", "success");
        closeAccountDialog();
        await refresh();
        openAccount(account.id ?? account?.id);
        return;
      }
      closeAccountDialog();
      await refresh();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={isEdit() ? `Edit @${existing()?.username ?? ""}` : "Add account"}
      icon={isEdit() ? "edit" : "plus"}
      onClose={closeAccountDialog}
      footer={
        <>
          <button class="btn" onClick={closeAccountDialog}>Cancel</button>
          <button class="btn primary" disabled={busy()} onClick={save}>{busy() ? "Saving…" : isEdit() ? "Save changes" : "Add account"}</button>
        </>
      }
    >
      <Show when={!isEdit()}>
        <Segmented
          value={tab()}
          onChange={(next) => setTab(next as "single" | "bulk")}
          options={[
            { value: "single", label: "One account" },
            { value: "bulk", label: "Bulk paste" },
          ]}
        />
      </Show>
      <Show when={error()}>
        <div class="banner danger"><Icon name="warning" size={14} /><span>{error()}</span></div>
      </Show>
      <Show when={tab() === "bulk"}>
        <div class="field">
          <label class="field-label" for="bulk-cookies">username, cookie — one per line</label>
          <textarea
            id="bulk-cookies"
            class="textarea"
            style={{ "min-height": "180px" }}
            value={bulk()}
            onInput={(event) => setBulk(event.currentTarget.value)}
            placeholder={"main_account\t_|WARNING:-DO-NOT-SHARE|_|…\nalt_account,_|WARNING:-DO-NOT-SHARE|_|…"}
            spellcheck={false}
          />
          <div class="tiny muted">Supports tab, colon, comma or bare cookies. Duplicates and invalid lines are skipped safely.</div>
        </div>
        <div class="banner info"><Icon name="shield" size={14} /><span>Cookies are encrypted with your vault key before the account record is written. They are never sent to a log.</span></div>
      </Show>
      <Show when={tab() === "single"}>
        <div class="row" style={{ "align-items": "flex-start" }}>
          <div class="field grow">
            <label class="field-label" for="account-username">Roblox username</label>
            <input id="account-username" class="input" value={username()} onInput={(event) => setUsername(event.currentTarget.value)} placeholder="moonlit_farmer" autocomplete="off" spellcheck={false} disabled={isEdit()} />
          </div>
          <div class="field grow">
            <label class="field-label" for="account-display">Display name</label>
            <input id="account-display" class="input" value={displayName()} onInput={(event) => setDisplayName(event.currentTarget.value)} placeholder="Optional" />
          </div>
        </div>
        <div class="row" style={{ "align-items": "flex-start" }}>
          <div class="field grow">
            <label class="field-label" for="account-alias">Alias</label>
            <input id="account-alias" class="input" value={alias()} onInput={(event) => setAlias(event.currentTarget.value)} placeholder="e.g. Main bank" />
          </div>
          <div class="field grow">
            <label class="field-label" for="account-lane">Lane</label>
            <select id="account-lane" class="select" value={laneId()} onChange={(event) => setLaneId(event.currentTarget.value)}>
              <For each={lanes()}>{(lane) => <option value={lane.id}>{lane.name}</option>}</For>
            </select>
          </div>
        </div>
        <div class="field">
          <label class="field-label" for="account-cookie">{isEdit() ? "Replace session cookie (optional)" : "Session cookie"}</label>
          <textarea
            id="account-cookie"
            class="textarea"
            style={{ "min-height": "58px" }}
            value={cookie()}
            onInput={(event) => setCookie(event.currentTarget.value)}
            placeholder=".ROBLOSECURITY cookie or a full Cookie: header"
            spellcheck={false}
            autocomplete="off"
          />
          <div class="tiny muted">Leave blank to add metadata now and paste the cookie later. Validation runs automatically in the background.</div>
        </div>
        <div class="field">
          <label class="field-label" for="account-tags">Tags</label>
          <input id="account-tags" class="input" value={tags()} onInput={(event) => setTags(event.currentTarget.value)} placeholder="main, premium, bank" />
          <div class="row" style={{ gap: "4px" }}>
            <For each={snapshot().fleet.settings.custom_tags}>{ (tag) => <button class="tag neutral" onClick={() => setTags([...tags().split(",").map(t=>t.trim()).filter(Boolean), tag].filter((v,i,a)=>a.indexOf(v)===i).join(", "))}>{tag}</button>}</For>
          </div>
        </div>
        <div class="field">
          <label class="field-label" for="account-notes">Notes</label>
          <textarea id="account-notes" class="textarea" value={notes()} onInput={(event) => setNotes(event.currentTarget.value)} placeholder="Private notes, stay local to this vault" />
          <div class="row" style={{ gap: "4px" }}>
            <For each={noteTemplates}>{(template) => <button class="btn sm ghost" onClick={() => setNotes((current) => current ? current + "\n" + template.text : template.text)}>{template.label}</button>}</For>
          </div>
        </div>
        <label class="check"><input type="checkbox" checked={favourite()} onChange={(event) => setFavourite(event.currentTarget.checked)} /> Favourite (pins to the top of the board)</label>
        <div class="banner info"><Icon name="info" size={14} /><span>You can also sign in through Roblox's browser flow. RoFarm never sees the password, only the resulting session cookie you choose to store.</span></div>
      </Show>
    </Modal>
  );
}
