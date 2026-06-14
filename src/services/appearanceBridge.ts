import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  APPEARANCE_SETTINGS_BROADCAST_CHANNEL,
  APPEARANCE_SETTINGS_EVENT,
  DEFAULT_APPEARANCE_SETTINGS,
  type AppearanceSettings,
  appearanceSettingsEqual,
  normalizeAppearanceSettings,
  persistAppearanceSettings,
  loadAppearanceSettings,
} from "../state/appearanceSettings";
import { isTauriRuntime } from "./realtimeBridge";

export function readAppearanceSettings(): AppearanceSettings {
  return loadAppearanceSettings();
}

let appearanceBroadcastChannel: BroadcastChannel | null = null;

function getAppearanceBroadcastChannel(): BroadcastChannel | null {
  if (typeof BroadcastChannel === "undefined") {
    return null;
  }

  appearanceBroadcastChannel ??= new BroadcastChannel(
    APPEARANCE_SETTINGS_BROADCAST_CHANNEL,
  );
  return appearanceBroadcastChannel;
}

export async function fetchAppearanceSettings(): Promise<AppearanceSettings> {
  if (isTauriRuntime()) {
    return invoke<AppearanceSettings>("get_appearance_settings");
  }
  return readAppearanceSettings();
}

export async function publishAppearanceSettings(
  settings: AppearanceSettings,
): Promise<AppearanceSettings> {
  const normalized = persistAppearanceSettings(settings);

  if (isTauriRuntime()) {
    return invoke<AppearanceSettings>("publish_appearance_settings", {
      payload: normalized,
    });
  }

  getAppearanceBroadcastChannel()?.postMessage(normalized);
  return normalized;
}

export async function listenAppearanceSettings(
  onChange: (settings: AppearanceSettings) => void,
): Promise<() => void> {
  const unlisteners: Array<() => void> = [];

  const applyIncoming = (value: AppearanceSettings | null | undefined) => {
    onChange(normalizeIncoming(value));
  };

  const onStorage = (event: StorageEvent) => {
    if (event.key !== "respondent.appearance") return;
    applyIncoming(readAppearanceSettings());
  };
  window.addEventListener("storage", onStorage);
  unlisteners.push(() => window.removeEventListener("storage", onStorage));

  const channel = getAppearanceBroadcastChannel();
  if (channel) {
    const onBroadcast = (event: MessageEvent<AppearanceSettings>) => {
      applyIncoming(event.data);
    };
    channel.addEventListener("message", onBroadcast);
    unlisteners.push(() => {
      channel.removeEventListener("message", onBroadcast);
    });
  }

  if (isTauriRuntime()) {
    const unlisten = await listen<AppearanceSettings>(
      APPEARANCE_SETTINGS_EVENT,
      (event) => {
        applyIncoming(event.payload);
      },
    );
    unlisteners.push(unlisten);
  }

  return () => {
    for (const unlisten of unlisteners) {
      unlisten();
    }
  };
}

export async function hydrateAppearanceSettings(
  localSettings: AppearanceSettings,
  dialogWindow: boolean,
): Promise<AppearanceSettings> {
  if (!isTauriRuntime()) {
    return localSettings;
  }

  const remote = await fetchAppearanceSettings();
  const shouldSeedRemote =
    !dialogWindow &&
    appearanceSettingsEqual(remote, DEFAULT_APPEARANCE_SETTINGS) &&
    !appearanceSettingsEqual(localSettings, DEFAULT_APPEARANCE_SETTINGS);

  if (shouldSeedRemote) {
    return publishAppearanceSettings(localSettings);
  }

  persistAppearanceSettings(remote);
  return remote;
}

function normalizeIncoming(
  value: AppearanceSettings | null | undefined,
): AppearanceSettings {
  if (!value) {
    return readAppearanceSettings();
  }

  const raw = value as AppearanceSettings & {
    appearance_theme?: unknown;
    window_opacity?: unknown;
    window_blur?: unknown;
  };

  return persistAppearanceSettings(
    normalizeAppearanceSettings({
      windowOpacity: raw.windowOpacity ?? raw.window_opacity,
      windowBlur: raw.windowBlur ?? raw.window_blur,
      appearanceTheme: raw.appearanceTheme ?? raw.appearance_theme,
    }),
  );
}
