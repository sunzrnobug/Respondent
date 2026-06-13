import { StrictMode } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invokeMock = vi.hoisted(() => vi.fn());
const revealItemInDirMock = vi.hoisted(() => vi.fn(async () => undefined));
const openDialogWindowMock = vi.hoisted(() => vi.fn());
const emitMock = vi.hoisted(() => vi.fn(async () => undefined));
const listenMock = vi.hoisted(() =>
  vi.fn(async () => () => undefined),
);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: revealItemInDirMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: emitMock,
  listen: listenMock,
}));

vi.mock("./services/dialogWindows", () => ({
  closeCurrentDialogWindow: vi.fn(async () => false),
  openDialogWindow: openDialogWindowMock,
}));

vi.mock("./services/windowFit", () => ({
  setupMainWindowFit: vi.fn(() => () => undefined),
}));

function mockTauriRuntime() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {
      invoke: invokeMock,
      transformCallback: (callback: () => void) => callback,
    },
    configurable: true,
  });
}

describe("App", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    revealItemInDirMock.mockReset();
    revealItemInDirMock.mockResolvedValue(undefined);
    openDialogWindowMock.mockReset();
    openDialogWindowMock.mockResolvedValue(false);
    localStorage.clear();
    window.history.replaceState(null, "", "/");
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  afterEach(() => {
    vi.useRealTimers();
    localStorage.clear();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("starts in a ready state before the user begins listening", () => {
    render(<App />);

    expect(screen.getByText("就绪")).toBeInTheDocument();
    expect(screen.queryByText("聆听中")).toBeNull();
    expect(screen.getByTitle("开始")).toBeInTheDocument();
    expect(screen.queryByTitle("暂停")).toBeNull();
  });

  it("starts native sessions with the low-latency default endpoint silence", async () => {
    mockTauriRuntime();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "list_audio_output_devices") {
        return [{ id: "default-output", name: "Default", is_default: true }];
      }
      if (command === "start_session") {
        return "native-session-1";
      }
      return [];
    });

    render(<App />);
    fireEvent.click(screen.getByTitle("开始"));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("start_session", {
        title: "会议",
        outputDeviceId: "default-output",
        endpointerSilenceMs: 300,
      });
    });
  });

  it("keeps native start mounted checks valid under React StrictMode", async () => {
    mockTauriRuntime();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "list_audio_output_devices") {
        return [{ id: "default-output", name: "Default", is_default: true }];
      }
      if (command === "start_session") {
        return "native-session-1";
      }
      return [];
    });

    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    fireEvent.click(screen.getByTitle("开始"));

    await vi.waitFor(() => {
      expect(screen.getByText("聆听中")).toBeInTheDocument();
    });
    expect(screen.getByTitle("暂停")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("end_session", {
      sessionId: "native-session-1",
    });
  });

  it("shows immediate feedback while a native session is starting", async () => {
    mockTauriRuntime();
    let resolveDevices: (devices: unknown) => void = () => {};
    const devicesPromise = new Promise((resolve) => {
      resolveDevices = resolve;
    });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "list_audio_output_devices") {
        return devicesPromise;
      }
      if (command === "start_session") {
        return "native-session-1";
      }
      return [];
    });

    render(<App />);
    fireEvent.click(screen.getByTitle("开始"));

    expect(screen.getByText("正在启动原生会话…")).toBeInTheDocument();
    expect(screen.getByTitle("处理中…")).toBeDisabled();

    resolveDevices([{ id: "default-output", name: "Default", is_default: true }]);
  });

  it("shows the native startup failure instead of staying silent", async () => {
    mockTauriRuntime();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "list_audio_output_devices") {
        throw new Error("无法读取音频输出设备");
      }
      return [];
    });

    render(<App />);
    fireEvent.click(screen.getByTitle("开始"));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(
      screen.getByText("启动会话失败：无法读取音频输出设备"),
    ).toBeInTheDocument();
    expect(screen.getByTitle("开始")).not.toBeDisabled();
  });

  it("streams a suggested reply after starting a mock session", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(700);
    });
    expect(screen.getByText("能否概括一下时间线")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(
      screen.getAllByText("先列出关键日期，再说明负责人和风险。").length,
    ).toBeGreaterThan(0);
  });

  it("asks whether to save the session after End", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    fireEvent.click(screen.getByTitle("结束"));

    expect(
      screen.getByRole("dialog", { name: "保存会话" }),
    ).toBeInTheDocument();
    expect(screen.getByText("本次会话记录是否保存")).toBeInTheDocument();
    expect(screen.getByText("已结束")).toBeInTheDocument();
  });

  it("opens only one detached save-session window after End under React StrictMode", async () => {
    mockTauriRuntime();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      return [];
    });
    openDialogWindowMock.mockResolvedValue(true);

    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    fireEvent.click(screen.getByTitle("结束"));

    expect(openDialogWindowMock).toHaveBeenCalledTimes(1);
    expect(openDialogWindowMock).toHaveBeenCalledWith("save-session");
  });

  it("saves an ended long conversation and opens it from the top-right history modal", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2200);
    });
    fireEvent.click(screen.getByTitle("结束"));
    fireEvent.click(screen.getByRole("button", { name: "保存会话" }));

    fireEvent.click(screen.getByTitle("会话历史"));
    const historyDialog = screen.getByRole("dialog", {
      name: "会话历史",
    });
    expect(
      within(historyDialog).getByRole("button", {
        name: /能否概括一下时间线先列出/,
      }),
    ).toBeInTheDocument();
    expect(
      within(historyDialog).getAllByText(/\d{4}-\d{2}-\d{2}/).length,
    ).toBeGreaterThan(0);

    fireEvent.click(
      within(historyDialog).getByRole("button", {
        name: /能否概括一下时间线先列出/,
      }),
    );
    expect(
      within(historyDialog).getByText("能否概括一下时间线？"),
    ).toBeInTheDocument();
    expect(
      within(historyDialog).getByText("先列出关键日期，再说明负责人和风险。"),
    ).toBeInTheDocument();
    expect(
      within(historyDialog).getByRole("button", { name: "导出 Markdown" }),
    ).toBeInTheDocument();
  });

  it("downloads markdown when exporting a saved conversation", async () => {
    const createObjectURL = vi
      .spyOn(URL, "createObjectURL")
      .mockReturnValue("blob:session-markdown");
    const revokeObjectURL = vi
      .spyOn(URL, "revokeObjectURL")
      .mockImplementation(() => undefined);
    const clickMock = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    const appendChild = vi.spyOn(document.body, "appendChild");
    const removeChild = vi.spyOn(document.body, "removeChild");

    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2200);
    });
    fireEvent.click(screen.getByTitle("结束"));
    fireEvent.click(screen.getByRole("button", { name: "保存会话" }));
    fireEvent.click(screen.getByTitle("会话历史"));

    const historyDialog = screen.getByRole("dialog", {
      name: "会话历史",
    });
    fireEvent.click(
      within(historyDialog).getByRole("button", {
        name: /能否概括一下时间线先列出/,
      }),
    );
    fireEvent.click(
      within(historyDialog).getByRole("button", { name: "导出 Markdown" }),
    );

    expect(createObjectURL).toHaveBeenCalledWith(expect.any(Blob));
    expect(appendChild).toHaveBeenCalledWith(expect.any(HTMLAnchorElement));
    expect(clickMock).toHaveBeenCalledTimes(1);
    expect(removeChild).toHaveBeenCalledWith(expect.any(HTMLAnchorElement));
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:session-markdown");
    expect(
      document.querySelector('a[download$=".md"]'),
    ).not.toBeInTheDocument();

    createObjectURL.mockRestore();
    revokeObjectURL.mockRestore();
    clickMock.mockRestore();
    appendChild.mockRestore();
    removeChild.mockRestore();
  });

  it("writes markdown through Tauri when exporting from a detached history window", async () => {
    mockTauriRuntime();
    window.history.replaceState(null, "", "/?dialog=conversation-history");
    localStorage.setItem(
      "respondent.savedSessions",
      JSON.stringify([
        {
          id: "saved-1",
          title: "测试导出",
          date: "2026-06-14",
          startedAt: "2026-06-14T10:00:00.000Z",
          endedAt: "2026-06-14T10:01:00.000Z",
          turns: [{ transcript: "测试转写", suggestion: "测试建议" }],
          systemMessages: [],
        },
      ]),
    );
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "save_markdown_file") {
        return "C:\\Users\\JackieLoveUnique\\Downloads\\测试导出.md";
      }
      return [];
    });

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "导出 Markdown" }));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(invokeMock).toHaveBeenCalledWith("save_markdown_file", {
      filename: "测试导出.md",
      content: expect.stringContaining("测试转写"),
    });
    expect(screen.getByText("已导出：")).toBeInTheDocument();
    const exportedFileLink = screen.getByRole("button", {
      name: "测试导出.md",
    });
    expect(exportedFileLink).toBeInTheDocument();

    fireEvent.click(exportedFileLink);

    await act(async () => {
      await Promise.resolve();
    });
    expect(revealItemInDirMock).toHaveBeenCalledWith(
      "C:\\Users\\JackieLoveUnique\\Downloads\\测试导出.md",
    );
  });

  it("clears export feedback when switching to another saved session", async () => {
    mockTauriRuntime();
    window.history.replaceState(null, "", "/?dialog=conversation-history");
    localStorage.setItem(
      "respondent.savedSessions",
      JSON.stringify([
        {
          id: "saved-1",
          title: "第一个会话",
          date: "2026-06-14",
          startedAt: "2026-06-14T10:00:00.000Z",
          endedAt: "2026-06-14T10:01:00.000Z",
          turns: [{ transcript: "第一个转写", suggestion: "第一个建议" }],
          systemMessages: [],
        },
        {
          id: "saved-2",
          title: "第二个会话",
          date: "2026-06-13",
          startedAt: "2026-06-13T10:00:00.000Z",
          endedAt: "2026-06-13T10:01:00.000Z",
          turns: [{ transcript: "第二个转写", suggestion: "第二个建议" }],
          systemMessages: [],
        },
      ]),
    );
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "save_markdown_file") {
        return "C:\\Users\\JackieLoveUnique\\Downloads\\第一个会话.md";
      }
      return [];
    });

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "导出 Markdown" }));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      screen.getByRole("button", { name: "第一个会话.md" }),
    ).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "第二个会话2026-06-13" }),
    );

    expect(
      screen.queryByRole("button", { name: "第一个会话.md" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("已导出：")).not.toBeInTheDocument();
  });

  it("deletes a saved long conversation from the history list", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2200);
    });
    fireEvent.click(screen.getByTitle("结束"));
    fireEvent.click(screen.getByRole("button", { name: "保存会话" }));

    fireEvent.click(screen.getByTitle("会话历史"));
    const historyDialog = screen.getByRole("dialog", {
      name: "会话历史",
    });
    const deleteButton = within(historyDialog).getByRole("button", {
      name: "删除",
    });

    fireEvent.click(deleteButton);

    expect(
      within(historyDialog).getByText("暂无已保存的会话。"),
    ).toBeInTheDocument();
    expect(
      within(historyDialog).getByText("保存会话后，可在这里查看完整长会话内容。"),
    ).toBeInTheDocument();
    expect(localStorage.getItem("respondent.savedSessions")).toBe("[]");
  });

  it("keeps bottom Session history scoped to current turn history, not saved long conversations", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3600);
    });
    fireEvent.click(screen.getByTitle("结束"));
    fireEvent.click(screen.getByRole("button", { name: "保存会话" }));

    fireEvent.click(screen.getByTitle("会话历史"));
    fireEvent.click(screen.getByTitle("关闭会话历史"));
    fireEvent.click(screen.getByRole("button", { name: "展开或收起轮次记录" }));
    const currentHistory = screen.getByLabelText("当前轮次记录");

    expect(
      within(currentHistory).getByText(/能否概括一下时间线/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: /能否概括一下时间线先列出/,
      }),
    ).toBeNull();
    expect(screen.queryByText(/\d{4}-\d{2}-\d{2}/)).toBeNull();
  });

  it("shows the latest current turn first and reveals all current turns from More history", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4800);
    });

    fireEvent.click(screen.getByRole("button", { name: "展开或收起轮次记录" }));
    const currentHistory = screen.getByLabelText("当前轮次记录");
    expect(
      within(currentHistory).getByText(/负责人那边有什么风险/),
    ).toBeInTheDocument();
    expect(
      within(currentHistory).queryByText(/能否概括一下时间线/),
    ).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "更多历史" }));

    expect(currentHistory).toHaveClass("expanded");
    expect(
      within(currentHistory).getByText("先列出关键日期，再说明负责人和风险。"),
    ).toBeInTheDocument();
    expect(
      within(currentHistory).getByText(/能否概括一下时间线/),
    ).toBeInTheDocument();
  });

  it("toggles bottom session history visibility from the dropdown", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("开始"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3200);
    });

    expect(screen.queryByLabelText("当前轮次记录")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "展开或收起轮次记录" }));
    expect(screen.getByLabelText("当前轮次记录")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "展开或收起轮次记录" }));
    expect(screen.queryByLabelText("当前轮次记录")).toBeNull();
  });

  it("saves LLM and ASR provider settings from the configuration panel", async () => {
    window.history.replaceState(null, "", "/?dialog=providers");
    mockTauriRuntime();
    invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "list_provider_profiles") {
        return { profiles: [], active: { llm: null, asr: null } };
      }
      if (command === "save_provider_profile") {
        return {
          profiles: [
            {
              id: "profile-1",
              name: args?.name,
              isActive: true,
              summary: {
                llm: { provider: "siliconflow", hasApiKey: true },
                asr: { provider: "bailian_realtime", hasApiKey: true },
              },
            },
          ],
          active: {
            llm: { provider: "siliconflow", hasApiKey: true },
            asr: { provider: "bailian_realtime", hasApiKey: true },
          },
        };
      }
      return [];
    });

    render(<App />);

    await vi.waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "服务商配置" }),
      ).toBeInTheDocument();
    });
    fireEvent.change(screen.getByLabelText("配置名称"), {
      target: { value: "工作配置" },
    });
    fireEvent.click(screen.getByLabelText("LLM 服务商"));
    fireEvent.click(screen.getByRole("option", { name: "SiliconFlow" }));
    fireEvent.change(screen.getByLabelText("LLM API 密钥"), {
      target: { value: "llm-key" },
    });
    fireEvent.click(screen.getByLabelText("ASR 服务商"));
    fireEvent.click(screen.getByRole("option", { name: "DashScope 实时" }));
    fireEvent.change(screen.getByLabelText("ASR API 密钥"), {
      target: { value: "asr-key" },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
    });

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_provider_profile", {
        name: "工作配置",
        profileId: null,
        payload: expect.objectContaining({
          llm: expect.objectContaining({
            provider: "siliconflow",
            apiKey: "llm-key",
          }),
          asr: expect.objectContaining({
            provider: "bailian_realtime",
            apiKey: "asr-key",
          }),
        }),
      });
    });
    await vi.waitFor(() => {
      expect(screen.getByText("已保存")).toBeInTheDocument();
      expect(screen.getByText("工作配置")).toBeInTheDocument();
    });
  });

  it("opens provider settings in an independent Tauri window when available", async () => {
    mockTauriRuntime();
    invokeMock.mockResolvedValue({ llm: null, asr: null });
    openDialogWindowMock.mockResolvedValueOnce(true);

    render(<App />);

    fireEvent.click(screen.getByTitle("服务商配置"));

    await vi.waitFor(() => {
      expect(openDialogWindowMock).toHaveBeenCalledWith("providers");
    });
    expect(screen.queryByRole("dialog", { name: "服务商配置" })).toBeNull();
  });

  it("opens appearance controls and adjusts window opacity and blur", () => {
    render(<App />);

    const shell = screen.getByRole("main");
    expect(shell).toHaveStyle({ "--window-opacity": "0.72" });
    expect(shell).toHaveStyle({ "--window-blur": "24px" });

    fireEvent.click(screen.getByTitle("外观设置"));
    expect(
      screen.getByRole("dialog", { name: "外观" }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("窗口透明度"), {
      target: { value: "86" },
    });
    fireEvent.change(screen.getByLabelText("背景模糊"), {
      target: { value: "16" },
    });

    expect(screen.getByText("86%")).toBeInTheDocument();
    expect(screen.getByText("16px")).toBeInTheDocument();
    expect(shell).toHaveStyle({ "--window-opacity": "0.86" });
    expect(shell).toHaveStyle({ "--window-blur": "16px" });
  });
});
