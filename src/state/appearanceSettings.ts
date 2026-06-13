import type { CSSProperties } from "react";

export const APPEARANCE_SETTINGS_STORAGE_KEY = "respondent.appearance";
export const APPEARANCE_SETTINGS_EVENT = "appearance-settings-changed";
export const APPEARANCE_SETTINGS_BROADCAST_CHANNEL =
  "respondent-appearance-settings";

const DARK_APPEARANCE_TOKENS = {
  "--shell-bg": "rgba(18, 20, 25, var(--window-opacity))",
  "--panel-bg": "rgba(255, 255, 255, 0.07)",
  "--panel-bg-strong": "rgba(255, 255, 255, 0.1)",
  "--panel-border": "rgba(255, 255, 255, 0.12)",
  "--hairline": "rgba(255, 255, 255, 0.1)",
  "--control-bg": "rgba(255, 255, 255, 0.08)",
  "--control-bg-hover": "rgba(255, 255, 255, 0.13)",
  "--control-border": "rgba(255, 255, 255, 0.14)",
  "--control-border-hover": "rgba(255, 255, 255, 0.26)",
  "--text": "#f7f8fb",
  "--muted": "#aeb7c5",
  "--soft": "#d4dbe6",
  "--reply": "#ffedb8",
  "--reply-muted": "#ffd86d",
  "--scrollbar-track": "rgba(255, 255, 255, 0.05)",
  "--scrollbar-thumb": "rgba(255, 255, 255, 0.22)",
  "--scrollbar-thumb-hover": "rgba(255, 255, 255, 0.34)",
} as const;

const LIGHT_APPEARANCE_TOKENS = {
  "--shell-bg": "rgba(247, 249, 252, var(--window-opacity))",
  "--panel-bg": "rgba(255, 255, 255, 0.62)",
  "--panel-bg-strong": "rgba(255, 255, 255, 0.78)",
  "--panel-border": "rgba(20, 24, 32, 0.08)",
  "--hairline": "rgba(20, 24, 32, 0.1)",
  "--control-bg": "rgba(255, 255, 255, 0.72)",
  "--control-bg-hover": "rgba(255, 255, 255, 0.9)",
  "--control-border": "rgba(20, 24, 32, 0.12)",
  "--control-border-hover": "rgba(20, 24, 32, 0.22)",
  "--text": "#171a21",
  "--muted": "#69717d",
  "--soft": "#343a44",
  "--reply": "#5f4300",
  "--reply-muted": "#8a6b16",
  "--scrollbar-track": "rgba(20, 24, 32, 0.06)",
  "--scrollbar-thumb": "rgba(20, 24, 32, 0.22)",
  "--scrollbar-thumb-hover": "rgba(20, 24, 32, 0.34)",
} as const;

export type AppearanceTheme = "dark" | "light";

export type AppearanceSettings = {
  windowOpacity: number;
  windowBlur: number;
  appearanceTheme: AppearanceTheme;
};

export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {
  windowOpacity: 72,
  windowBlur: 24,
  appearanceTheme: "dark",
};

function clampOpacity(value: number): number {
  return Math.min(92, Math.max(55, Math.round(value)));
}

function clampBlur(value: number): number {
  return Math.min(32, Math.max(8, Math.round(value)));
}

function normalizeTheme(value: unknown): AppearanceTheme {
  return value === "light" ? "light" : "dark";
}

export function normalizeAppearanceSettings(
  value: Partial<AppearanceSettings> | null | undefined,
): AppearanceSettings {
  return {
    windowOpacity: clampOpacity(
      value?.windowOpacity ?? DEFAULT_APPEARANCE_SETTINGS.windowOpacity,
    ),
    windowBlur: clampBlur(
      value?.windowBlur ?? DEFAULT_APPEARANCE_SETTINGS.windowBlur,
    ),
    appearanceTheme: normalizeTheme(value?.appearanceTheme),
  };
}

export function loadAppearanceSettings(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): AppearanceSettings {
  const raw = storage.getItem(APPEARANCE_SETTINGS_STORAGE_KEY);
  if (!raw) return DEFAULT_APPEARANCE_SETTINGS;

  try {
    return normalizeAppearanceSettings(JSON.parse(raw) as AppearanceSettings);
  } catch {
    return DEFAULT_APPEARANCE_SETTINGS;
  }
}

export function persistAppearanceSettings(
  settings: AppearanceSettings,
  storage: Pick<Storage, "setItem"> = window.localStorage,
): AppearanceSettings {
  const normalized = normalizeAppearanceSettings(settings);
  storage.setItem(APPEARANCE_SETTINGS_STORAGE_KEY, JSON.stringify(normalized));
  return normalized;
}

export function appearanceSettingsEqual(
  left: AppearanceSettings,
  right: AppearanceSettings,
): boolean {
  return (
    left.windowOpacity === right.windowOpacity &&
    left.windowBlur === right.windowBlur &&
    left.appearanceTheme === right.appearanceTheme
  );
}

export function buildAppearanceShellStyle(
  settings: Pick<
    AppearanceSettings,
    "windowOpacity" | "windowBlur" | "appearanceTheme"
  >,
): CSSProperties {
  const themeTokens =
    settings.appearanceTheme === "light"
      ? LIGHT_APPEARANCE_TOKENS
      : DARK_APPEARANCE_TOKENS;

  return {
    "--window-opacity": (settings.windowOpacity / 100).toFixed(2),
    "--window-blur": `${settings.windowBlur}px`,
    ...themeTokens,
  } as CSSProperties;
}
