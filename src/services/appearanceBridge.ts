import { emit, listen } from "@tauri-apps/api/event";
import {
  APPEARANCE_SETTINGS_BROADCAST_CHANNEL,
  APPEARANCE_SETTINGS_EVENT,
  type AppearanceSettings,
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

export async function publishAppearanceSettings(
  settings: AppearanceSettings,
): Promise<AppearanceSettings> {
  const normalized = persistAppearanceSettings(settings);
  getAppearanceBroadcastChannel()?.postMessage(normalized);

  if (isTauriRuntime()) {
    await emit(APPEARANCE_SETTINGS_EVENT, normalized);
  }

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

function normalizeIncoming(
  value: AppearanceSettings | null | undefined,
): AppearanceSettings {
  const current = readAppearanceSettings();
  if (!value) return current;
  const next = {
    windowOpacity: value.windowOpacity ?? current.windowOpacity,
    windowBlur: value.windowBlur ?? current.windowBlur,
    appearanceTheme: value.appearanceTheme ?? current.appearanceTheme,
  };
  return persistAppearanceSettings(next);
}
