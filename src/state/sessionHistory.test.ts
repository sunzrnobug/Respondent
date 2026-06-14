import { describe, expect, it, vi } from "vitest";
import { createInitialSessionState } from "./sessionStore";
import {
  createSavedSession,
  exportSavedSessionMarkdown,
  loadSavedSessions,
  persistSavedSession,
  purgeExpiredSessions,
  removeSavedSessionById,
  SESSION_HISTORY_STORAGE_KEY,
  summarizeSessionTitle,
} from "./sessionHistory";

vi.mock("../services/realtimeBridge", () => ({
  isTauriRuntime: () => false,
}));

function createMemoryStorage() {
  const storage = new Map<string, string>();
  return {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => {
      storage.set(key, value);
    },
    removeItem: (key: string) => {
      storage.delete(key);
    },
    clear: () => storage.clear(),
    key: () => null,
    length: 0,
    raw: storage,
  } as Storage & { raw: Map<string, string> };
}

describe("session history", () => {
  it("summarizes a Chinese session title from transcript content", () => {
    const session = createInitialSessionState("s1");
    session.transcript.push("能否概括一下时间线？");
    session.suggestions.push("先列出关键日期，再说明负责人和风险。");

    expect(summarizeSessionTitle(session)).toBe("能否概括一下时间线先列出");
  });

  it("purges expired saved sessions on load", async () => {
    const api = createMemoryStorage();
    Object.defineProperty(globalThis, "window", {
      value: { localStorage: api },
      configurable: true,
    });

    const expired = createSavedSession(
      createInitialSessionState("expired"),
      new Date("2020-01-01T00:00:00.000Z"),
    );
    const recent = createSavedSession(
      createInitialSessionState("recent"),
      new Date("2099-01-01T00:00:00.000Z"),
    );
    api.setItem(
      SESSION_HISTORY_STORAGE_KEY,
      JSON.stringify([expired, recent]),
    );

    const loaded = await loadSavedSessions();
    expect(loaded).toHaveLength(1);
    expect(loaded[0]?.id).toBe("recent");
  });

  it("creates, persists, loads, and removes saved sessions", async () => {
    const api = createMemoryStorage();
    Object.defineProperty(globalThis, "window", {
      value: { localStorage: api },
      configurable: true,
    });

    const session = createInitialSessionState("session-1");
    session.transcript.push("你好");
    session.suggestions.push("你好，有什么可以帮你？");

    const saved = createSavedSession(session, new Date("2026-06-14T10:00:00.000Z"));
    await persistSavedSession(saved);

    expect(await loadSavedSessions()).toHaveLength(1);
    expect(api.getItem(SESSION_HISTORY_STORAGE_KEY)).toContain("session-1");

    const next = await removeSavedSessionById([saved], saved.id);
    expect(next).toHaveLength(0);
    expect(await loadSavedSessions()).toHaveLength(0);
  });

  it("exports saved sessions as markdown with a sensitivity notice", () => {
    const session = createInitialSessionState("session-2");
    session.transcript.push("测试转写");
    session.suggestions.push("测试建议");
    const saved = createSavedSession(session, new Date("2026-06-14T12:00:00.000Z"));

    const markdown = exportSavedSessionMarkdown(saved);

    expect(markdown).toContain("会议敏感文本");
    expect(markdown).toContain("测试转写");
    expect(markdown).toContain("测试建议");
    expect(markdown).toContain("时间线");
  });

  it("purges expired sessions by endedAt", () => {
    const expired = createSavedSession(
      createInitialSessionState("expired"),
      new Date("2020-01-01T00:00:00.000Z"),
    );
    const recent = createSavedSession(
      createInitialSessionState("recent"),
      new Date("2099-01-01T00:00:00.000Z"),
    );
    const retained = purgeExpiredSessions([expired, recent]);
    expect(retained).toHaveLength(1);
    expect(retained[0]?.id).toBe("recent");
  });
});
