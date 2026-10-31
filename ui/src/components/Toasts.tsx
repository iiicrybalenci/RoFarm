import { For } from "solid-js";
import { Icon } from "./Icon";
import { dismissToast, toasts } from "../lib/store";

export function Toasts() {
  return (
    <div class="toast-stack" aria-live="polite" aria-atomic="false">
      <For each={toasts()}>
        {(toast) => (
          <div class={`toast ${toast.kind}`} role="status">
            <span class="toast-accent" />
            <div class="grow">
              <div class="toast-title">{toast.title}</div>
              {toast.message && <div class="toast-msg">{toast.message}</div>}
            </div>
            <button class="toast-close" onClick={() => dismissToast(toast.id)} aria-label="Dismiss">
              <Icon name="close" size={13} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
