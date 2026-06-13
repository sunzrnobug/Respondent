import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const setSizeMock = vi.hoisted(() => vi.fn(async () => undefined));
const innerSizeMock = vi.hoisted(() =>
  vi.fn(async () => ({ width: 420, height: 388 })),
);
const scaleFactorMock = vi.hoisted(() => vi.fn(async () => 1));
const getCurrentWindowMock = vi.hoisted(() =>
  vi.fn(() => ({
    innerSize: innerSizeMock,
    scaleFactor: scaleFactorMock,
    setSize: setSizeMock,
  })),
);

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: getCurrentWindowMock,
}));

vi.mock("@tauri-apps/api/dpi", () => ({
  LogicalSize: class LogicalSize {
    width: number;
    height: number;

    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  },
}));

vi.mock("./realtimeBridge", () => ({
  isTauriRuntime: () => true,
}));

import { setupMainWindowFit } from "./windowFit";

describe("setupMainWindowFit", () => {
  let mutationCallback: MutationCallback | null = null;

  beforeEach(() => {
    setSizeMock.mockClear();
    innerSizeMock.mockClear();
    scaleFactorMock.mockClear();
    getCurrentWindowMock.mockClear();
    innerSizeMock.mockResolvedValue({ width: 420, height: 388 });
    scaleFactorMock.mockResolvedValue(1);
    mutationCallback = null;
    vi.stubGlobal(
      "ResizeObserver",
      class {
        private callback: ResizeObserverCallback;

        constructor(callback: ResizeObserverCallback) {
          this.callback = callback;
        }

        observe() {
          this.callback([], this as unknown as ResizeObserver);
        }

        disconnect() {}
      },
    );
    vi.stubGlobal(
      "MutationObserver",
      class {
        constructor(callback: MutationCallback) {
          mutationCallback = callback;
        }

        observe() {}

        disconnect() {}
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resizes the main window height while preserving the native width", async () => {
    const element = document.createElement("main");
    element.getBoundingClientRect = () =>
      ({
        width: 560,
        height: 388,
        top: 0,
        left: 0,
        right: 420,
        bottom: 388,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect;

    const cleanup = setupMainWindowFit(element);
    await Promise.resolve();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 388 }),
    );

    cleanup();
  });

  it("does not include hidden scrollable content in the native window size", async () => {
    const element = document.createElement("main");
    element.getBoundingClientRect = () =>
      ({
        width: 420,
        height: 300,
        top: 0,
        left: 0,
        right: 420,
        bottom: 300,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect;
    Object.defineProperty(element, "scrollHeight", {
      configurable: true,
      value: 520,
    });

    const cleanup = setupMainWindowFit(element);
    await Promise.resolve();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 300 }),
    );

    mutationCallback?.([], {} as MutationObserver);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();

    expect(setSizeMock).toHaveBeenCalledTimes(1);

    cleanup();
  });

  it("resizes again when expanded content changes the visible shell box", async () => {
    const element = document.createElement("main");
    let height = 300;
    element.getBoundingClientRect = () =>
      ({
        width: 420,
        height,
        top: 0,
        left: 0,
        right: 420,
        bottom: height,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect;

    const cleanup = setupMainWindowFit(element);
    await Promise.resolve();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 300 }),
    );

    height = 520;
    mutationCallback?.([], {} as MutationObserver);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();

    expect(setSizeMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ width: 420, height: 520 }),
    );

    cleanup();
  });
});
