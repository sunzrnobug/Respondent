import { describe, expect, it } from "vitest";
import {
  DEFAULT_APPEARANCE_SETTINGS,
  buildAppearanceShellStyle,
  loadAppearanceSettings,
  normalizeAppearanceSettings,
  persistAppearanceSettings,
} from "./appearanceSettings";

describe("appearance settings", () => {
  it("returns defaults when storage is empty", () => {
    const storage = new Map<string, string>();

    expect(
      loadAppearanceSettings({
        getItem: (key) => storage.get(key) ?? null,
      }),
    ).toEqual(DEFAULT_APPEARANCE_SETTINGS);
  });

  it("persists and reloads appearance settings", () => {
    const storage = new Map<string, string>();
    const api = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
    };

    persistAppearanceSettings(
      {
        windowOpacity: 86,
        windowBlur: 18,
        appearanceTheme: "light",
      },
      api,
    );

    expect(loadAppearanceSettings(api)).toEqual({
      windowOpacity: 86,
      windowBlur: 18,
      appearanceTheme: "light",
    });
  });

  it("clamps invalid values", () => {
    expect(
      normalizeAppearanceSettings({
        windowOpacity: 10,
        windowBlur: 99,
        appearanceTheme: "neon",
      } as unknown as Parameters<typeof normalizeAppearanceSettings>[0]),
    ).toEqual({
      windowOpacity: 55,
      windowBlur: 32,
      appearanceTheme: "dark",
    });
  });

  it("builds shell styles with theme tokens inline", () => {
    expect(
      buildAppearanceShellStyle({
        windowOpacity: 80,
        windowBlur: 16,
        appearanceTheme: "light",
      }),
    ).toMatchObject({
      "--window-opacity": "0.80",
      "--window-blur": "16px",
      "--text": "#171a21",
      "--shell-bg": "rgba(247, 249, 252, var(--window-opacity))",
    });
  });
});
