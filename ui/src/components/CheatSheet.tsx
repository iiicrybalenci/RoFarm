import { For } from "solid-js";
import { Modal } from "./Primitives";
import { closeOverlays, settings } from "../lib/store";

const labels: Record<string, string> = {
  command_palette: "Command palette",
  global_search: "Global search",
  kill_all: "Hold to kill all",
  panic: "Panic hotkey (off by default)",
  mini_mode: "Mini mode",
  toggle_view_board: "Board",
  toggle_view_fleet: "Fleet",
  toggle_view_timeline: "Timeline",
  toggle_view_automations: "Automations",
  toggle_view_vault: "Vault",
  cheat_sheet: "This cheat sheet",
  save_scene: "Save scene",
  screenshot_client: "Screenshot client",
  lock_vault: "Lock vault",
  focus_next_client: "Focus next client",
  undo: "Undo",
  redo: "Redo",
};

export function CheatSheet() {
  return (
    <Modal title="Keyboard shortcuts" icon="keyboard" wide onClose={closeOverlays}>
      <p class="muted" style={{ margin: "0 0 6px", "font-size": "0.76rem" }}>
        Every shortcut is rebindable in Settings → Shortcuts. Shortcuts never fire while you are typing in a field.
      </p>
      <div class="cheat-grid">
        <For each={Object.entries(settings().shortcuts)}>
          {([action, keys]) => (
            <div class="cheat-row">
              <span>{labels[action] ?? action.replaceAll("_", " ")}</span>
              <span class="keys">{keys}</span>
            </div>
          )}
        </For>
        <div class="cheat-row"><span>Focus search</span><span class="keys">Ctrl K</span></div>
        <div class="cheat-row"><span>Close overlays</span><span class="keys">Esc</span></div>
        <div class="cheat-row"><span>Board pan</span><span class="keys">Space + drag</span></div>
        <div class="cheat-row"><span>Board zoom</span><span class="keys">Ctrl + wheel</span></div>
      </div>
    </Modal>
  );
}
