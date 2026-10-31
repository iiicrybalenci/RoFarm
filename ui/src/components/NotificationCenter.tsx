import { For, Show } from "solid-js";
import { Drawer } from "./Primitives";
import { Icon } from "./Icon";
import { activeNotices, closeNotifications, notificationsOpen, refresh, toast } from "../lib/store";
import { invoke } from "../lib/api";
import { timeAgo } from "../lib/format";
import type { Notice } from "../lib/types";

function iconFor(severity: Notice["severity"]) {
  return severity === "critical" ? "error" : severity === "warning" ? "warning" : severity === "success" ? "check" : "info";
}

export function NotificationCenter() {
  if (!notificationsOpen()) return null;
  const markAll = async () => {
    try {
      await invoke("mark_notices_read");
      await refresh();
    } catch (error: any) {
      toast("Could not mark notices read", error.message, "error");
    }
  };
  const dismiss = async (id: string) => {
    try {
      await invoke("remove_notice", { id });
      await refresh();
    } catch (error: any) {
      toast("Could not dismiss notice", error.message, "error");
    }
  };
  return (
    <Drawer onClose={closeNotifications} label="Notification center">
      <div class="drawer-head">
        <div class="grow">
          <h2>Notifications</h2>
          <div class="muted tiny">{activeNotices().filter((n) => !n.read).length} unread · local history only</div>
        </div>
        <button class="btn sm" onClick={markAll}>Mark all read</button>
        <button class="icon-btn" aria-label="Close" onClick={closeNotifications}><Icon name="close" size={16} /></button>
      </div>
      <div class="drawer-body" style={{ padding: "0" }}>
        <Show when={activeNotices().length} fallback={<div style={{ padding: "40px 20px", "text-align": "center" }} class="muted">You're all caught up. Alerts from the supervisor, automations and updates appear here.</div>}>
          <div class="notice-list">
            <For each={activeNotices()}>
              {(notice) => (
                <div class={`notice ${notice.severity}`}>
                  <div class="notice-icon"><Icon name={iconFor(notice.severity)} size={15} /></div>
                  <div class="grow">
                    <div class="notice-title">{notice.title} {!notice.read && <span class="dot in_game" style={{ display: "inline-block", "margin-left": "4px" }} />}</div>
                    <div class="notice-body">{notice.body}</div>
                    <div class="row" style={{ "margin-top": "5px", gap: "8px" }}>
                      <span class="tiny muted">{timeAgo(notice.at)}</span>
                      <Show when={notice.action}>
                        {(action) => <button class="btn ghost sm" onClick={() => { closeNotifications(); }}>{action().label}</button>}
                      </Show>
                    </div>
                  </div>
                  <button class="icon-btn" aria-label="Dismiss notification" onClick={() => dismiss(notice.id)}><Icon name="close" size={13} /></button>
                </div>
              )}
            </For>
          </div>
        </Show>
        <div style={{ padding: "12px 16px 22px" }}>
          <div class="banner info">
            <Icon name="shield" size={15} />
            <span>Native toasts respect Focus Assist and quiet hours. Nothing leaves your PC.</span>
          </div>
        </div>
      </div>
    </Drawer>
  );
}
