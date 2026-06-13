import { beforeEach, describe, expect, it, vi } from "vitest";
import { dialogWindowOptions, openDialogWindow } from "./dialogWindows";

const getByLabelMock = vi.hoisted(() => vi.fn());
const showMock = vi.hoisted(() => vi.fn());
const setFocusMock = vi.hoisted(() => vi.fn());
const webviewWindowConstructorMock = vi.hoisted(() => vi.fn());
let tauriRuntime = false;

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: class {
    static getByLabel = getByLabelMock;

    once = vi.fn((event: string, handler: () => void) => {
      if (event === "tauri://created") {
        handler();
      }
      return Promise.resolve(() => undefined);
    });

    constructor(label: string, options: unknown) {
      webviewWindowConstructorMock(label, options);
    }
  },
}));

vi.mock("./realtimeBridge", () => ({
  isTauriRuntime: () => tauriRuntime,
}));

describe("dialog window service", () => {
  beforeEach(() => {
    tauriRuntime = true;
    getByLabelMock.mockReset();
    showMock.mockReset();
    setFocusMock.mockReset();
    webviewWindowConstructorMock.mockReset();
    window.history.replaceState(null, "", "/");
  });

  it("builds independent draggable native windows without a main-window parent", () => {
    const providers = dialogWindowOptions("providers");
    const history = dialogWindowOptions("conversation-history");
    const saveSession = dialogWindowOptions("save-session");

    expect(providers.label).toBe("dialog-providers");
    expect(history.label).toBe("dialog-conversation-history");
    expect(saveSession.label).toBe("dialog-save-session");
    expect(providers.window).not.toHaveProperty("parent");
    expect(providers.window.resizable).toBe(true);
    expect(providers.window.decorations).toBe(false);
    expect(providers.window.transparent).toBe(true);
    expect(providers.window.shadow).toBe(false);
    expect(providers.window.center).toBe(true);
    expect(providers.window.width).toBeGreaterThan(500);
    expect(providers.window.height).toBeGreaterThan(500);
    expect(history.window.url).toContain("dialog=conversation-history");
    expect(saveSession.window.url).toContain("dialog=save-session");
  });

  it("opens a new Tauri dialog window for the requested route", async () => {
    getByLabelMock.mockResolvedValue(null);

    await expect(openDialogWindow("providers")).resolves.toBe(true);

    expect(webviewWindowConstructorMock).toHaveBeenCalledWith(
      "dialog-providers",
      expect.objectContaining({
        url: expect.stringContaining("dialog=providers"),
        resizable: true,
        decorations: false,
        transparent: true,
        shadow: false,
      }),
    );
  });

  it("focuses an existing dialog instead of creating a duplicate", async () => {
    getByLabelMock.mockResolvedValue({
      show: showMock,
      setFocus: setFocusMock,
    });

    await expect(openDialogWindow("appearance")).resolves.toBe(true);

    expect(showMock).toHaveBeenCalled();
    expect(setFocusMock).toHaveBeenCalled();
    expect(webviewWindowConstructorMock).not.toHaveBeenCalled();
  });

  it("falls back to in-app dialogs outside Tauri", async () => {
    tauriRuntime = false;

    await expect(openDialogWindow("appearance")).resolves.toBe(false);

    expect(webviewWindowConstructorMock).not.toHaveBeenCalled();
  });
});
