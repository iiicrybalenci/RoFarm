import { For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { Icon, type IconName } from "./Icon";
import { colorPair, initials as initialsOf } from "../lib/format";
import type { Account } from "../lib/types";

export function Avatar(props: { account?: Account; name?: string; size?: "sm" | "md" | "lg" | number; presence?: boolean }) {
  const name = () => props.account?.display_name || props.account?.username || props.name || "?";
  const [a, b] = colorPair(props.account?.id ?? name());
  const size = () => (typeof props.size === "number" ? props.size : props.size === "lg" ? 46 : props.size === "sm" ? 22 : 30);
  return (
    <div
      class={`avatar ${props.size === "lg" ? "lg" : ""}`}
      style={{ width: `${size()}px`, height: `${size()}px`, "--avatar-a": a, "--avatar-b": b, "border-radius": size() <= 22 ? "6px" : size() >= 46 ? "12px" : "8px" }}
      aria-hidden="true"
    >
      <Show when={props.account?.avatar_url} fallback={<span style={{ "font-size": `${Math.max(8, size() * 0.34)}px` }}>{initialsOf(name())}</span>}>
        <img src={props.account!.avatar_url!} alt="" loading="lazy" />
      </Show>
      <Show when={props.presence !== false}>
        <span class={`presence ${props.account ? (props.account.presence === "Online" ? "online" : "idle") : ""}`} />
      </Show>
    </div>
  );
}

export function Sparkline(props: { values: number[]; width?: number; height?: number; color?: string; fill?: boolean }) {
  const values = () => (props.values.length > 1 ? props.values : [0, ...(props.values.length ? props.values : [0, 0])]);
  const width = () => props.width ?? 120;
  const height = () => props.height ?? 24;
  const points = () => {
    const max = Math.max(...values(), 1);
    const min = Math.min(...values(), 0);
    const span = max - min || 1;
    return values().map((v, i) => {
      const x = (i / Math.max(1, values().length - 1)) * width();
      const y = height() - ((v - min) / span) * (height() - 3) - 1.5;
      return [x, y] as const;
    });
  };
  const path = () => {
    const pts = points();
    if (!pts.length) return "";
    return pts.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  };
  const area = () => `${path()} L${width()},${height()} L0,${height()} Z`;
  return (
    <svg class="sparkline" width={width()} height={height()} viewBox={`0 0 ${width()} ${height()}`} preserveAspectRatio="none" aria-hidden="true">
      <Show when={props.fill}>
        <path d={area()} fill={props.color ?? "var(--accent)"} opacity="0.12" stroke="none" />
      </Show>
      <path d={path()} style={props.color ? { stroke: props.color } : undefined} />
    </svg>
  );
}

export function Switch(props: { checked: boolean; onChange: (next: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      class={`switch ${props.checked ? "on" : ""}`}
      role="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      disabled={props.disabled}
      onClick={() => props.onChange(!props.checked)}
    />
  );
}

export function ToggleRow(props: { title: string; desc?: string; checked: boolean; onChange: (next: boolean) => void; disabled?: boolean }) {
  return (
    <div class="setting-row">
      <div class="setting-copy">
        <div class="setting-title">{props.title}</div>
        <Show when={props.desc}><div class="setting-desc">{props.desc}</div></Show>
      </div>
      <div class="setting-control">
        <Switch checked={props.checked} onChange={props.onChange} label={props.title} disabled={props.disabled} />
      </div>
    </div>
  );
}

export function Segmented<T extends string>(props: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void; label?: string }) {
  return (
    <div class="segmented" role="group" aria-label={props.label}>
      <For each={props.options}>
        {(option) => (
          <button class={props.value === option.value ? "active" : ""} aria-pressed={props.value === option.value} onClick={() => props.onChange(option.value)}>
            {option.label}
          </button>
        )}
      </For>
    </div>
  );
}

export function Modal(props: {
  title: string;
  children: JSX.Element;
  footer?: JSX.Element;
  onClose: () => void;
  wide?: boolean;
  narrow?: boolean;
  icon?: IconName;
}) {
  let panel: HTMLDivElement | undefined;
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      props.onClose();
    }
    if (event.key === "Tab" && panel) {
      const focusable = panel.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  };
  onMount(() => {
    document.addEventListener("keydown", onKey, true);
    requestAnimationFrame(() => panel?.querySelector<HTMLElement>("input, textarea, button")?.focus());
  });
  onCleanup(() => document.removeEventListener("keydown", onKey, true));
  return (
    <Portal>
      <div class="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) props.onClose(); }}>
        <div class={`modal ${props.wide ? "wide" : ""} ${props.narrow ? "narrow" : ""}`} role="dialog" aria-modal="true" aria-label={props.title} ref={panel}>
          <div class="modal-head">
            <Show when={props.icon}>{(icon) => <Icon name={icon()} size={18} />}</Show>
            <h2>{props.title}</h2>
            <div class="grow" />
            <button class="icon-btn" onClick={props.onClose} aria-label="Close"><Icon name="close" size={16} /></button>
          </div>
          <div class="modal-body">{props.children}</div>
          <Show when={props.footer}><div class="modal-foot">{props.footer}</div></Show>
        </div>
      </div>
    </Portal>
  );
}

export function Drawer(props: { children: JSX.Element; onClose: () => void; label: string }) {
  onMount(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
    };
    document.addEventListener("keydown", handler);
    onCleanup(() => document.removeEventListener("keydown", handler));
  });
  return (
    <Portal>
      <div class="drawer-backdrop" onMouseDown={props.onClose} />
      <aside class="drawer" role="dialog" aria-modal="true" aria-label={props.label}>
        {props.children}
      </aside>
    </Portal>
  );
}

export function HoldButton(props: { onConfirm: () => void; label: string; class?: string; icon?: IconName; disabled?: boolean; duration?: number }) {
  let timer: number | undefined;
  const [holding, setHolding] = createSignal(false);
  const [progress, setProgress] = createSignal(0);
  const start = () => {
    if (props.disabled) return;
    setHolding(true);
    const started = performance.now();
    const duration = props.duration ?? 1100;
    const step = () => {
      const pct = Math.min(1, (performance.now() - started) / duration);
      setProgress(pct);
      if (pct >= 1) {
        stop();
        props.onConfirm();
      } else {
        timer = window.setTimeout(step, 24);
      }
    };
    step();
  };
  const stop = () => {
    window.clearTimeout(timer);
    setHolding(false);
    setProgress(0);
  };
  onCleanup(stop);
  return (
    <button
      class={`btn kill-btn ${props.class ?? ""} ${holding() ? "holding" : ""}`}
      onPointerDown={start}
      onPointerUp={stop}
      onPointerLeave={stop}
      onPointerCancel={stop}
      disabled={props.disabled}
      aria-label={props.label}
      title={props.label}
    >
      <span class="hold-fill" style={{ transform: `scaleX(${progress()})`, transition: holding() ? "transform 1.1s linear" : "none" }} />
      <Show when={props.icon}>{(icon) => <Icon name={icon()} size={14} />}</Show>
      <span style={{ position: "relative" }}>{props.label}</span>
    </button>
  );
}

export function EmptyState(props: { icon: IconName; title: string; body: string; action?: JSX.Element }) {
  return (
    <div class="empty">
      <div class="empty-card">
        <div class="empty-icon"><Icon name={props.icon} size={24} /></div>
        <h2>{props.title}</h2>
        <p>{props.body}</p>
        <Show when={props.action}><div style={{ "margin-top": "6px" }}>{props.action}</div></Show>
      </div>
    </div>
  );
}

export function KeyCap(props: { keys: string }) {
  return <span class="kbd">{props.keys}</span>;
}

export function Meter(props: { value: number; max: number; warnAt?: number; dangerAt?: number }) {
  const pct = () => Math.min(100, Math.max(0, (props.value / Math.max(1, props.max)) * 100));
  const tone = () => (props.dangerAt !== undefined && props.value >= props.dangerAt ? "danger" : props.warnAt !== undefined && props.value >= props.warnAt ? "warn" : "");
  return (
    <div class={`meter ${tone()}`} aria-hidden="true">
      <span style={{ width: `${pct()}%` }} />
    </div>
  );
}

export function Section(props: { title: string; desc?: string; children: JSX.Element; action?: JSX.Element }) {
  return (
    <section class="section">
      <div class="row between">
        <div>
          <h3>{props.title}</h3>
          <Show when={props.desc}><p class="section-desc">{props.desc}</p></Show>
        </div>
        <Show when={props.action}>{props.action}</Show>
      </div>
      {props.children}
    </section>
  );
}

export function CopyField(props: { value: string; label?: string }) {
  const [copied, setCopied] = createSignal(false);
  const copy = async () => {
    await navigator.clipboard.writeText(props.value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };
  return (
    <div class="row" style={{ gap: "6px", "flex-wrap": "nowrap" }}>
      <div class="copy grow" aria-label={props.label}>{props.value}</div>
      <button class="btn sm" onClick={copy}><Icon name={copied() ? "check" : "copy"} size={13} />{copied() ? "Copied" : "Copy"}</button>
    </div>
  );
}

export function Spinner(props: { size?: number }) {
  const size = props.size ?? 16;
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-label="Loading" role="status" style={{ animation: "spin 900ms linear infinite" }}>
      <style>{`@keyframes spin { to { transform: rotate(360deg); transform-origin: 12px 12px; } }`}</style>
      <circle cx="12" cy="12" r="9" stroke="var(--line)" stroke-width="2.5" />
      <path d="M21 12a9 9 0 00-9-9" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" />
    </svg>
  );
}
