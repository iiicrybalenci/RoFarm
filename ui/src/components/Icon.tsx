import type { JSX } from "solid-js";

type IconName =
  | "board"
  | "fleet"
  | "timeline"
  | "servers"
  | "automation"
  | "inventory"
  | "friends"
  | "vault"
  | "metrics"
  | "logs"
  | "settings"
  | "search"
  | "command"
  | "kill"
  | "play"
  | "plus"
  | "chevron"
  | "close"
  | "minimize"
  | "maximize"
  | "restore"
  | "more"
  | "refresh"
  | "shield"
  | "bell"
  | "lock"
  | "unlock"
  | "copy"
  | "eye"
  | "download"
  | "upload"
  | "trash"
  | "edit"
  | "external"
  | "grid"
  | "layout"
  | "camera"
  | "volume"
  | "cpu"
  | "memory"
  | "network"
  | "battery"
  | "sun"
  | "moon"
  | "check"
  | "warning"
  | "error"
  | "info"
  | "filter"
  | "drag"
  | "dots"
  | "spark"
  | "clock"
  | "user"
  | "globe"
  | "database"
  | "plugin"
  | "keyboard"
  | "palette"
  | "rocket"
  | "pause"
  | "restart"
  | "monitor"
  | "pin"
  | "folder"
  | "file"
  | "code"
  | "terminal"
  | "save"
  | "arrow-left"
  | "arrow-right"
  | "rotate"
  | "link"
  | "zap"
  | "heart"
  | "shuffle"
  | "wifi"
  | "temp"
  | "help"
  | "cloud"
  | "discord"
  | "telegram"
  | "slack"
  | "mail"
  | "volume-off"
  | "subtract"
  | "sidebar"
  | "table"
  | "card";

const paths: Record<IconName, JSX.Element> = {
  board: <><rect x="3" y="4" width="7" height="16" rx="1.5" /><rect x="14" y="4" width="7" height="9" rx="1.5" /><rect x="14" y="17" width="7" height="3" rx="1.2" /></>,
  fleet: <><rect x="3" y="4" width="8" height="7" rx="1.5" /><rect x="13" y="4" width="8" height="7" rx="1.5" /><rect x="3" y="13" width="8" height="7" rx="1.5" /><rect x="13" y="13" width="8" height="7" rx="1.5" /></>,
  timeline: <><path d="M3 12h4l2-7 4 14 2-7h6" /></>,
  servers: <><rect x="3" y="4" width="18" height="6" rx="2" /><rect x="3" y="14" width="18" height="6" rx="2" /><path d="M7 7h.01M7 17h.01" /></>,
  automation: <><path d="M12 3v4M12 17v4M3 12h4M17 12h4" /><circle cx="12" cy="12" r="4" /><path d="M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M18.4 5.6l-2.8 2.8M8.4 15.6l-2.8 2.8" /></>,
  inventory: <><path d="M21 8l-9-5-9 5v8l9 5 9-5V8z" /><path d="M3 8l9 5 9-5M12 13v8" /></>,
  friends: <><circle cx="9" cy="8" r="3.2" /><path d="M2.8 20c.4-3.4 2.9-5.4 6.2-5.4s5.8 2 6.2 5.4" /><circle cx="17.5" cy="9" r="2.4" /><path d="M16 14.7c2.7.2 4.6 1.9 5 4.5" /></>,
  vault: <><rect x="3" y="4" width="18" height="16" rx="3" /><circle cx="12" cy="12" r="4" /><path d="M12 8v2M12 14v2M8 12h2M14 12h2" /></>,
  metrics: <><path d="M3 20V4M3 20h18" /><path d="M6 16l4-5 3 3 5-7" /></>,
  logs: <><path d="M5 3h10l4 4v14H5z" /><path d="M9 8h6M9 12h6M9 16h4" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 00.3 1.9l.1.1-2.1 2.1-.1-.1a1.7 1.7 0 00-1.9-.3 1.7 1.7 0 00-1 1.5V20h-3v-.2a1.7 1.7 0 00-1-1.5 1.7 1.7 0 00-1.9.3l-.1.1-2.1-2.1.1-.1a1.7 1.7 0 00.3-1.9 1.7 1.7 0 00-1.5-1H4v-3h.2a1.7 1.7 0 001.5-1 1.7 1.7 0 00-.3-1.9l-.1-.1 2.1-2.1.1.1a1.7 1.7 0 001.9.3 1.7 1.7 0 001-1.5V4h3v.2a1.7 1.7 0 001 1.5 1.7 1.7 0 001.9-.3l.1-.1 2.1 2.1-.1.1a1.7 1.7 0 00-.3 1.9 1.7 1.7 0 001.5 1h.2v3h-.2a1.7 1.7 0 00-1.5 1z" /></>,
  search: <><circle cx="11" cy="11" r="7" /><path d="M20 20l-4-4" /></>,
  command: <><path d="M9 6a3 3 0 10-3 3h12a3 3 0 10-3-3v12a3 3 0 103-3H6a3 3 0 103 3V6z" /></>,
  kill: <><path d="M6 6l12 12M18 6L6 18" /></>,
  play: <path d="M7 4l13 8-13 8z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  chevron: <path d="M9 6l6 6-6 6" />,
  close: <path d="M6 6l12 12M18 6L6 18" />,
  minimize: <path d="M5 12h14" />,
  maximize: <rect x="5" y="5" width="14" height="14" rx="1" />,
  restore: <><rect x="8" y="4" width="12" height="12" rx="1" /><path d="M16 20H4V8" /></>,
  more: <><circle cx="5" cy="12" r="1" /><circle cx="12" cy="12" r="1" /><circle cx="19" cy="12" r="1" /></>,
  refresh: <><path d="M21 12a9 9 0 11-3-6.7" /><path d="M21 4v5h-5" /></>,
  shield: <><path d="M12 3l8 3v6c0 5-3.4 8.4-8 9-4.6-.6-8-4-8-9V6z" /><path d="M9 12l2 2 4-4" /></>,
  bell: <><path d="M18 8a6 6 0 10-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9" /><path d="M10 21h4" /></>,
  lock: <><rect x="4" y="10" width="16" height="11" rx="2" /><path d="M8 10V7a4 4 0 018 0v3" /></>,
  unlock: <><rect x="4" y="10" width="16" height="11" rx="2" /><path d="M8 10V7a4 4 0 017.7-1.4" /></>,
  copy: <><rect x="9" y="9" width="12" height="12" rx="2" /><path d="M5 15H4a1 1 0 01-1-1V4a1 1 0 011-1h10a1 1 0 011 1v1" /></>,
  eye: <><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z" /><circle cx="12" cy="12" r="3" /></>,
  download: <><path d="M12 3v12M7 10l5 5 5-5" /><path d="M4 21h16" /></>,
  upload: <><path d="M12 21V9M7 14l5-5 5 5" /><path d="M4 3h16" /></>,
  trash: <><path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14" /><path d="M10 11v7M14 11v7" /></>,
  edit: <><path d="M4 20h4L20 8l-4-4L4 16z" /><path d="M14 6l4 4" /></>,
  external: <><path d="M14 4h6v6M20 4l-9 9" /><path d="M20 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V5a1 1 0 011-1h5" /></>,
  grid: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
  layout: <><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M9 3v18M3 9h18" /></>,
  camera: <><path d="M4 7h3l2-3h6l2 3h3a1 1 0 011 1v11a1 1 0 01-1 1H4a1 1 0 01-1-1V8a1 1 0 011-1z" /><circle cx="12" cy="13" r="4" /></>,
  volume: <><path d="M4 9v6h4l5 4V5L8 9z" /><path d="M16 8a5 5 0 010 8M19 5a9 9 0 010 14" /></>,
  "volume-off": <><path d="M4 9v6h4l5 4V5L8 9z" /><path d="M16 9l5 6M21 9l-5 6" /></>,
  cpu: <><rect x="5" y="5" width="14" height="14" rx="2" /><rect x="9" y="9" width="6" height="6" rx="1" /><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" /></>,
  memory: <><rect x="3" y="6" width="18" height="12" rx="2" /><path d="M7 18v3M12 18v3M17 18v3M7 10h10v4H7z" /></>,
  network: <><circle cx="12" cy="5" r="2" /><circle cx="5" cy="19" r="2" /><circle cx="19" cy="19" r="2" /><path d="M12 7v5M12 12L6 17M12 12l6 5" /></>,
  wifi: <><path d="M2 8.8a15 15 0 0120 0M5 12.5a10 10 0 0114 0M8.5 16a5 5 0 017 0" /><circle cx="12" cy="20" r="1" /></>,
  battery: <><rect x="2" y="7" width="18" height="10" rx="2" /><path d="M22 10v4M6 10v4M10 10v4" /></>,
  sun: <><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M19 5l-1.5 1.5M6.5 17.5L5 19" /></>,
  moon: <path d="M20 15a8.5 8.5 0 11-11-11 7 7 0 0011 11z" />,
  check: <path d="M4 12l5 5L20 6" />,
  warning: <><path d="M12 3l10 18H2z" /><path d="M12 9v5M12 18v.01" /></>,
  error: <><circle cx="12" cy="12" r="9" /><path d="M12 7v6M12 17v.01" /></>,
  info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7v.01" /></>,
  filter: <path d="M3 5h18l-7 8v6l-4 2v-8z" />,
  drag: <><circle cx="9" cy="6" r="1" /><circle cx="15" cy="6" r="1" /><circle cx="9" cy="12" r="1" /><circle cx="15" cy="12" r="1" /><circle cx="9" cy="18" r="1" /><circle cx="15" cy="18" r="1" /></>,
  dots: <><circle cx="5" cy="12" r="1.4" /><circle cx="12" cy="12" r="1.4" /><circle cx="19" cy="12" r="1.4" /></>,
  spark: <path d="M12 2l2.2 6.8L21 11l-6.8 2.2L12 20l-2.2-6.8L3 11l6.8-2.2z" />,
  clock: <><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>,
  user: <><circle cx="12" cy="8" r="4" /><path d="M4 21c.6-4.2 3.7-6.5 8-6.5s7.4 2.3 8 6.5" /></>,
  globe: <><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3c2.5 2.4 4 5.6 4 9s-1.5 6.6-4 9c-2.5-2.4-4-5.6-4-9s1.5-6.6 4-9z" /></>,
  database: <><ellipse cx="12" cy="5" rx="8" ry="3" /><path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" /></>,
  plugin: <><path d="M9 3v4M15 3v4M5 7h14v5a7 7 0 01-14 0z" /><path d="M12 17v4" /></>,
  keyboard: <><rect x="2" y="6" width="20" height="12" rx="2" /><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M6 14h.01M18 14h.01M9 14h6" /></>,
  palette: <><path d="M12 3a9 9 0 000 18c1.5 0 2-1 1.5-2-.6-1.2.2-2.5 1.6-2.5H17a4 4 0 004-4c0-5-4-9.5-9-9.5z" /><circle cx="7.5" cy="10" r="1" /><circle cx="10" cy="6.5" r="1" /><circle cx="15" cy="7" r="1" /></>,
  rocket: <><path d="M5 15l4 4M14 4c4 1 6 3 6 6 0 4-4 8-8 10l-2-2C12 14 16 10 20 10" /><path d="M9 15l-4 4M14 10a1.5 1.5 0 100-3 1.5 1.5 0 000 3z" /></>,
  pause: <><rect x="6" y="4" width="4" height="16" rx="1" /><rect x="14" y="4" width="4" height="16" rx="1" /></>,
  restart: <><path d="M3 12a9 9 0 109-9" /><path d="M3 3v6h6" /></>,
  monitor: <><rect x="2" y="3" width="20" height="14" rx="2" /><path d="M8 21h8M12 17v4" /></>,
  pin: <><path d="M12 21v-7M8 3h8l-1 7 3 3H6l3-3z" /></>,
  folder: <path d="M3 6a1 1 0 011-1h5l2 3h9a1 1 0 011 1v10a1 1 0 01-1 1H4a1 1 0 01-1-1z" />,
  file: <><path d="M6 2h8l4 4v16H6z" /><path d="M14 2v5h5" /></>,
  code: <><path d="M8 6l-5 6 5 6M16 6l5 6-5 6" /></>,
  terminal: <><rect x="2" y="4" width="20" height="16" rx="2" /><path d="M6 9l3 3-3 3M12 15h5" /></>,
  save: <><path d="M4 4h13l3 3v13H4z" /><path d="M8 4v5h8V4M8 20v-6h8v6" /></>,
  "arrow-left": <path d="M20 12H4M10 6l-6 6 6 6" />,
  "arrow-right": <path d="M4 12h16M14 6l6 6-6 6" />,
  rotate: <><path d="M20 11a8 8 0 10-2.3 6.3" /><path d="M20 5v6h-6" /></>,
  link: <><path d="M10 13a5 5 0 007 0l3-3a5 5 0 00-7-7l-1 1" /><path d="M14 11a5 5 0 00-7 0l-3 3a5 5 0 007 7l1-1" /></>,
  zap: <path d="M13 2L4 14h6l-1 8 9-12h-6z" />,
  heart: <path d="M12 21s-8-5-8-11a4.5 4.5 0 018-3 4.5 4.5 0 018 3c0 6-8 11-8 11z" />,
  shuffle: <><path d="M3 7h3c5 0 7 10 12 10h3M3 17h3c2 0 3.5-1.5 5-3.5M14 9.5c1-1.5 2-2.5 4-2.5h3" /><path d="M18 4l3 3-3 3M18 14l3 3-3 3" /></>,
  temp: <><path d="M10 13V5a2 2 0 014 0v8a4 4 0 11-4 0z" /><path d="M12 9v6" /></>,
  help: <><circle cx="12" cy="12" r="9" /><path d="M9.5 9a2.5 2.5 0 015 0c0 1.5-2.5 2-2.5 4M12 17v.01" /></>,
  cloud: <path d="M7 18a4 4 0 01-.5-8 6 6 0 0111.7-1.2A4.5 4.5 0 0117 18z" />,
  discord: <><path d="M8 6.5A14 14 0 0112 6a14 14 0 014 .5c2 3 3 6 3 10a12 12 0 01-4 2l-1-2a9 9 0 01-4 0l-1 2a12 12 0 01-4-2c0-4 1-7 3-10z" /><circle cx="9.5" cy="12.5" r="1" /><circle cx="14.5" cy="12.5" r="1" /></>,
  telegram: <path d="M21 4L3 11l5 2 2 6 3-4 5 3z" />,
  slack: <><path d="M9 3a2 2 0 000 4h2V5a2 2 0 00-2-2zM3 9a2 2 0 004 0V7H5a2 2 0 00-2 2zM15 21a2 2 0 000-4h-2v2a2 2 0 002 2zM21 15a2 2 0 00-4 0v2h2a2 2 0 002-2z" /><path d="M9 7h6v10H9z" /></>,
  mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M3 7l9 6 9-6" /></>,
  subtract: <path d="M5 12h14" />,
  sidebar: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></>,
  table: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 10h18M9 10v10" /></>,
  card: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 9h4M7 13h7" /></>,
};

export function Icon(props: { name: IconName; size?: number; class?: string; strokeWidth?: number }) {
  return (
    <svg
      class={props.class}
      width={props.size ?? 18}
      height={props.size ?? 18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width={props.strokeWidth ?? 1.7}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {paths[props.name] ?? paths.info}
    </svg>
  );
}

export type { IconName };
