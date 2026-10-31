import type { Settings, ThemeDefinition, ThemePalette } from "./types";

export const palettes: Record<string, ThemePalette> = {
  obsidian: {
    id: "obsidian",
    name: "Obsidian",
    mode: "dark",
    accent: "#6c8cff",
    second: "#a78bfa",
    bg: "#0b0e14",
    bg2: "#10141d",
    panel: "#141a25",
    panel2: "#1a2130",
    line: "#252e3f",
    text: "#e9edf7",
    muted: "#93a0b7",
    ok: "#4fd18b",
    warn: "#f0b55a",
    danger: "#ff6574",
    info: "#5cc8ff",
    radius: 12,
  },
  paper: {
    id: "paper",
    name: "Paper",
    mode: "light",
    accent: "#3157d5",
    second: "#8b5cf6",
    bg: "#f4f1ea",
    bg2: "#ece8de",
    panel: "#fbf9f4",
    panel2: "#f1ede4",
    line: "#ddd6c8",
    text: "#23252b",
    muted: "#6c6f79",
    ok: "#1f8a5b",
    warn: "#a86416",
    danger: "#c22f3f",
    info: "#176f9c",
    radius: 10,
  },
  aurora: {
    id: "aurora",
    name: "Aurora",
    mode: "dark",
    accent: "#5eead4",
    second: "#818cf8",
    bg: "#071316",
    bg2: "#0b1b20",
    panel: "#0f242a",
    panel2: "#143039",
    line: "#1e4049",
    text: "#e4f5f6",
    muted: "#8fb0b7",
    ok: "#70e0a1",
    warn: "#f5c76b",
    danger: "#ff7b8b",
    info: "#67d5ff",
    radius: 14,
  },
  terminal: {
    id: "terminal",
    name: "Terminal",
    mode: "dark",
    accent: "#5ef08a",
    second: "#b6f36a",
    bg: "#050806",
    bg2: "#081009",
    panel: "#0b140d",
    panel2: "#101c12",
    line: "#1d3221",
    text: "#d9f7df",
    muted: "#7da889",
    ok: "#5ef08a",
    warn: "#e8d25a",
    danger: "#ff6b6b",
    info: "#6bd5e8",
    radius: 4,
  },
  high_contrast: {
    id: "high_contrast",
    name: "High Contrast",
    mode: "dark",
    accent: "#ffde59",
    second: "#7cd4ff",
    bg: "#000000",
    bg2: "#050505",
    panel: "#0a0a0a",
    panel2: "#121212",
    line: "#7a7a7a",
    text: "#ffffff",
    muted: "#d0d0d0",
    ok: "#5dff9c",
    warn: "#ffd15d",
    danger: "#ff5d6c",
    info: "#6fdcff",
    radius: 2,
  },
  neon: {
    id: "neon",
    name: "Neon",
    mode: "dark",
    accent: "#ff4ecd",
    second: "#4ee9ff",
    bg: "#0a0410",
    bg2: "#12071c",
    panel: "#180a25",
    panel2: "#21102f",
    line: "#3a1a52",
    text: "#f4e9ff",
    muted: "#a98bc4",
    ok: "#4dffb8",
    warn: "#ffd166",
    danger: "#ff5470",
    info: "#4ee9ff",
    radius: 16,
  },
  sakura: {
    id: "sakura",
    name: "Sakura",
    mode: "light",
    accent: "#d84f86",
    second: "#8f6ed5",
    bg: "#fbf1f4",
    bg2: "#f6e7ec",
    panel: "#fff9fb",
    panel2: "#f9eef2",
    line: "#ead2da",
    text: "#35242b",
    muted: "#8b6c78",
    ok: "#2f8d65",
    warn: "#a76a1f",
    danger: "#c4304f",
    info: "#36729a",
    radius: 16,
  },
  bricks: {
    id: "bricks",
    name: "Bricks",
    mode: "light",
    accent: "#d1492e",
    second: "#e3a21a",
    bg: "#efe7da",
    bg2: "#e7ddcd",
    panel: "#f8f2e7",
    panel2: "#efe6d7",
    line: "#d3c4ad",
    text: "#3a2f24",
    muted: "#7c6a55",
    ok: "#3f7d43",
    warn: "#a96a12",
    danger: "#b33427",
    info: "#2f6b85",
    radius: 6,
  },
};

const colorAliases: Record<string, keyof ThemePalette> = {
  accent: "accent",
  secondary: "second",
  background: "bg",
  bg: "bg",
  surface: "panel",
  panel: "panel",
  panel_alt: "panel2",
  border: "line",
  text: "text",
  muted: "muted",
  success: "ok",
  warning: "warn",
  danger: "danger",
  info: "info",
};

export function paletteFromSettings(settings: Settings): ThemePalette {
  const base = palettes[settings.theme?.builtin] ?? palettes.obsidian;
  const custom = settings.theme?.custom;
  if (!custom) return base;
  return paletteFromDefinition(custom, base);
}

export function paletteFromDefinition(definition: ThemeDefinition, fallback = palettes.obsidian): ThemePalette {
  const result: ThemePalette = {
    ...fallback,
    id: definition.id || "custom",
    name: definition.name || "Custom",
    mode: definition.mode === "light" ? "light" : "dark",
    radius: definition.radius_px ?? fallback.radius,
  };
  for (const [key, value] of Object.entries(definition.colors ?? {})) {
    const mapped = colorAliases[key];
    if (mapped && typeof value === "string") {
      (result as any)[mapped] = value;
    }
  }
  return result;
}

let mediaQuery: MediaQueryList | null = null;
let mediaListener: ((event: MediaQueryListEvent) => void) | null = null;

export function applyTheme(settings: Settings, systemAccent?: string) {
  const root = document.documentElement;
  const palette = paletteFromSettings(settings);
  const custom = settings.theme?.custom;
  const accent = settings.theme?.accent_override || (settings.theme?.accent_sync && systemAccent) || palette.accent;
  root.dataset.theme = palette.id;
  root.dataset.mode = palette.mode;
  root.dataset.density = settings.density ?? "comfortable";
  root.dataset.material = settings.theme?.reduce_transparency ? "solid" : (settings.theme?.material ?? "mica");
  root.style.setProperty("--accent", accent);
  root.style.setProperty("--accent-2", palette.second);
  root.style.setProperty("--bg", palette.bg);
  root.style.setProperty("--bg-2", palette.bg2);
  root.style.setProperty("--panel", palette.panel);
  root.style.setProperty("--panel-2", palette.panel2);
  root.style.setProperty("--line", palette.line);
  root.style.setProperty("--text", palette.text);
  root.style.setProperty("--muted", palette.muted);
  root.style.setProperty("--ok", palette.ok);
  root.style.setProperty("--warn", palette.warn);
  root.style.setProperty("--danger", palette.danger);
  root.style.setProperty("--info", palette.info);
  root.style.setProperty("--radius", `${palette.radius}px`);
  root.style.setProperty("--radius-sm", `${Math.max(4, palette.radius - 4)}px`);
  root.style.setProperty("--ui-scale", String((settings.ui_scale ?? 100) / 100));
  const dim = custom?.background_dim ?? settings.theme?.background_dim ?? 40;
  root.style.setProperty("--bg-dim", String(dim / 100));
  root.style.setProperty("--bg-blur", `${custom?.background_blur ?? 0}px`);
  if (custom?.background_image) {
    const safe = /^https:\/\//.test(custom.background_image) || custom.background_image.startsWith("data:image/");
    root.style.setProperty("--bg-image", safe ? `url("${custom.background_image.replace(/"/g, "%22")}")` : "none");
  } else {
    root.style.setProperty("--bg-image", "none");
  }
  if (custom?.font_family) {
    root.style.setProperty("--font", `"${custom.font_family.replace(/"/g, "")}", "Segoe UI Variable", system-ui, sans-serif`);
  } else {
    root.style.removeProperty("--font");
  }
  const motion = settings.performance?.reduce_motion ?? "system";
  if (motion === "reduce") {
    root.dataset.motion = "reduce";
  } else if (motion === "full") {
    root.dataset.motion = "full";
  } else {
    delete root.dataset.motion;
  }
  applyAccentListener(settings, systemAccent);
}

function applyAccentListener(settings: Settings, systemAccent?: string) {
  if (mediaListener && mediaQuery) {
    mediaQuery.removeEventListener("change", mediaListener);
    mediaListener = null;
    mediaQuery = null;
  }
  if (!settings.theme?.accent_sync || settings.theme?.accent_override) return;
  try {
    mediaQuery = window.matchMedia("(prefers-color-scheme: light)");
    mediaListener = () => {
      if (systemAccent) document.documentElement.style.setProperty("--accent", systemAccent);
    };
    mediaQuery.addEventListener("change", mediaListener);
  } catch {
  }
}

export function themeDefinitionFrom(palette: ThemePalette): ThemeDefinition {
  return {
    schema_version: 1,
    id: palette.id,
    name: palette.name,
    author: "You",
    mode: palette.mode,
    colors: {
      accent: palette.accent,
      secondary: palette.second,
      background: palette.bg,
      panel: palette.panel,
      panel_alt: palette.panel2,
      border: palette.line,
      text: palette.text,
      muted: palette.muted,
      success: palette.ok,
      warning: palette.warn,
      danger: palette.danger,
      info: palette.info,
    },
    radius_px: palette.radius,
    background_image: null,
    background_dim: 40,
    background_blur: 0,
  };
}
