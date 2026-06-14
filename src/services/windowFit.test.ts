import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const setSizeMock = vi.hoisted(() =>
  vi.fn(async (_size: { width: number; height: number }) => undefined),
);
const innerSizeMock = vi.hoisted(() =>
  vi.fn(async () => ({ width: 420, height: 520 })),
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

import { remeasureMainWindowFit, setupDialogWindowFit, setupMainWindowFit } from "./windowFit";

async function flushWindowFit() {
  for (let index = 0; index < 3; index += 1) {
    await Promise.resolve();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await Promise.resolve();
  }
}

describe("setupMainWindowFit", () => {
  let mutationCallback: MutationCallback | null = null;

  function mockShellHeight(element: HTMLElement, height: number) {
    Object.defineProperty(element, "scrollHeight", {
      configurable: true,
      value: height,
    });
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
  }

  beforeEach(() => {
    setSizeMock.mockClear();
    innerSizeMock.mockClear();
    scaleFactorMock.mockClear();
    getCurrentWindowMock.mockClear();
    innerSizeMock.mockResolvedValue({ width: 420, height: 520 });
    scaleFactorMock.mockResolvedValue(1);
    setSizeMock.mockImplementation(async (size: { width: number; height: number }) => {
      innerSizeMock.mockResolvedValue({
        width: size.width,
        height: size.height,
      });
    });
    mutationCallback = null;
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
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
        constructor(_callback: ResizeObserverCallback) {}
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fits the main window height to content on startup", async () => {
    const element = document.createElement("main");
    mockShellHeight(element, 388);

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 388 }),
    );
    expect(element.style.minHeight).toBe("388px");

    cleanup();
  });

  it("stretches the shell when the native window is manually resized taller", async () => {
    const element = document.createElement("main");
    mockShellHeight(element, 388);

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(element.style.minHeight).toBe("388px");

    innerSizeMock.mockResolvedValue({ width: 420, height: 520 });
    globalThis.dispatchEvent(new Event("resize"));
    await flushWindowFit();

    expect(element.style.minHeight).toBe("520px");

    cleanup();
  });

  it("does not resize on startup when content already matches the window height", async () => {
    const element = document.createElement("main");
    mockShellHeight(element, 520);

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(setSizeMock).not.toHaveBeenCalled();
    expect(element.style.minHeight).toBe("520px");

    cleanup();
  });

  it("does not include hidden scrollable content in the native window size", async () => {
    const element = document.createElement("main");
    mockShellHeight(element, 300);
    Object.defineProperty(element, "scrollHeight", {
      configurable: true,
      value: 520,
    });

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 300 }),
    );

    mutationCallback?.([], {} as MutationObserver);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledTimes(1);

    cleanup();
  });

  it("grows the window when expanded content exceeds the current height", async () => {
    const element = document.createElement("main");
    let height = 388;
    const applyHeight = () => mockShellHeight(element, height);
    applyHeight();

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 388 }),
    );

    height = 560;
    applyHeight();
    mutationCallback?.([], {} as MutationObserver);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ width: 420, height: 560 }),
    );

    cleanup();
  });

  it("shrinks the window when expanded content collapses below the current height", async () => {
    const element = document.createElement("main");
    let height = 388;
    const applyHeight = () => mockShellHeight(element, height);
    applyHeight();

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    height = 560;
    applyHeight();
    mutationCallback?.([], {} as MutationObserver);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ width: 420, height: 560 }),
    );

    height = 388;
    applyHeight();
    remeasureMainWindowFit(element, { forceContentShrink: true });
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ width: 420, height: 388 }),
    );

    cleanup();
  });

  it("does not shrink the window after the user manually resizes taller", async () => {
    const element = document.createElement("main");
    let height = 388;
    const applyHeight = () => mockShellHeight(element, height);
    applyHeight();

    const cleanup = setupMainWindowFit(element);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 420, height: 388 }),
    );

    innerSizeMock.mockResolvedValue({ width: 420, height: 520 });
    globalThis.dispatchEvent(new Event("resize"));
    await flushWindowFit();

    expect(element.style.minHeight).toBe("520px");

    height = 300;
    applyHeight();
    mutationCallback?.([], {} as MutationObserver);
    await flushWindowFit();

    expect(setSizeMock).not.toHaveBeenCalledWith(
      expect.objectContaining({ height: 300 }),
    );

    cleanup();
  });
});

describe("setupDialogWindowFit", () => {
  beforeEach(() => {
    setSizeMock.mockClear();
    innerSizeMock.mockClear();
    scaleFactorMock.mockClear();
    innerSizeMock.mockResolvedValue({ width: 480, height: 640 });
    scaleFactorMock.mockResolvedValue(1);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
        constructor(_callback: ResizeObserverCallback) {}
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resizes the dialog window to the measured root height", async () => {
    const root = document.createElement("main");
    root.className = "dialogWindowRoot dialogWindowRootFitContent";
    root.getBoundingClientRect = () =>
      ({
        height: 418,
      }) as DOMRect;
    const panel = document.createElement("section");
    panel.className = "replyStylePanel detachedPanel";
    root.append(panel);
    document.body.append(root);

    const cleanup = setupDialogWindowFit(panel);
    await flushWindowFit();

    expect(setSizeMock).toHaveBeenCalledWith(
      expect.objectContaining({ width: 480, height: 418 }),
    );

    cleanup();
    root.remove();
  });
});
