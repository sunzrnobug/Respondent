import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("App", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  afterEach(() => {
    vi.useRealTimers();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("streams a suggested reply after starting a mock session", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("Start"));

    // Partial subtitle shows while the speaker is mid-sentence.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(700);
    });
    expect(
      screen.getByText("Could you summarize the timeline"),
    ).toBeInTheDocument();

    // After endpoint + final + reply tokens, the streamed suggestion lands.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(
      screen.getByText(
        "Start with the key dates, then call out owners and risks.",
      ),
    ).toBeInTheDocument();
  });

  it("marks the session saved after End", async () => {
    render(<App />);

    fireEvent.click(screen.getByTitle("Start"));
    fireEvent.click(screen.getByTitle("End"));

    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  it("saves LLM and ASR provider settings from the configuration panel", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_provider_config") {
        return { llm: null, asr: null };
      }
      if (command === "save_provider_config") {
        return {
          llm: { provider: "siliconflow", hasApiKey: true },
          asr: { provider: "bailian_realtime", hasApiKey: true },
        };
      }
      return [];
    });

    render(<App />);

    fireEvent.click(screen.getByTitle("Configure providers"));
    fireEvent.change(screen.getByLabelText("LLM provider"), {
      target: { value: "siliconflow" },
    });
    fireEvent.change(screen.getByLabelText("LLM API key"), {
      target: { value: "llm-key" },
    });
    fireEvent.change(screen.getByLabelText("ASR provider"), {
      target: { value: "bailian_realtime" },
    });
    fireEvent.change(screen.getByLabelText("ASR API key"), {
      target: { value: "asr-key" },
    });
    fireEvent.click(screen.getByText("Save"));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_provider_config", {
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
  });
});
