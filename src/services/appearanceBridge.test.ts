import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  listenAppearanceSettings,
  publishAppearanceSettings,
} from "./appearanceBridge";

const emitMock = vi.hoisted(() => vi.fn(async () => undefined));
const listenMock = vi.hoisted(() =>
  vi.fn(async (_event, handler: (payload: { payload: unknown }) => void) => {
    tauriHandler = handler;
    return () => undefined;
  }),
);

let tauriHandler: ((payload: { payload: unknown }) => void) | null = null;

vi.mock("@tauri-apps/api/event", () => ({
  emit: emitMock,
  listen: listenMock,
}));

class MockBroadcastChannel {
  static channels = new Map<string, Set<MockBroadcastChannel>>();

  private listeners = new Set<(event: MessageEvent<unknown>) => void>();

  constructor(public name: string) {
    const channels =
      MockBroadcastChannel.channels.get(name) ?? new Set<MockBroadcastChannel>();
    channels.add(this);
    MockBroadcastChannel.channels.set(name, channels);
  }

  postMessage(data: unknown) {
    for (const channel of MockBroadcastChannel.channels.get(this.name) ?? []) {
      for (const listener of channel.listeners) {
        listener({ data } as MessageEvent<unknown>);
      }
    }
  }

  addEventListener(_type: "message", listener: (event: MessageEvent) => void) {
    this.listeners.add(listener);
  }

  removeEventListener(
    _type: "message",
    listener: (event: MessageEvent) => void,
  ) {
    this.listeners.delete(listener);
  }
}

describe("appearanceBridge", () => {
  beforeEach(() => {
    vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: { invoke: vi.fn(), transformCallback: (cb: () => void) => cb },
      configurable: true,
    });
  });

  afterEach(() => {
    localStorage.clear();
    emitMock.mockReset();
    listenMock.mockReset();
    tauriHandler = null;
    MockBroadcastChannel.channels.clear();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.unstubAllGlobals();
  });

  it("broadcasts theme changes to other listeners immediately", async () => {
    const onChange = vi.fn();
    const unlisten = await listenAppearanceSettings(onChange);

    await publishAppearanceSettings({
      windowOpacity: 72,
      windowBlur: 24,
      appearanceTheme: "light",
    });

    expect(onChange).toHaveBeenCalledWith({
      windowOpacity: 72,
      windowBlur: 24,
      appearanceTheme: "light",
    });
    expect(emitMock).toHaveBeenCalled();

    unlisten();
  });

  it("forwards tauri appearance events to listeners", async () => {
    const onChange = vi.fn();
    const unlisten = await listenAppearanceSettings(onChange);

    tauriHandler?.({
      payload: {
        windowOpacity: 80,
        windowBlur: 20,
        appearanceTheme: "dark",
      },
    });

    expect(onChange).toHaveBeenCalledWith({
      windowOpacity: 80,
      windowBlur: 20,
      appearanceTheme: "dark",
    });

    unlisten();
  });
});
