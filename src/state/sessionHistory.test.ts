import { describe, expect, it } from "vitest";
import { createInitialSessionState } from "./sessionStore";
import {
  createSavedSession,
  exportSavedSessionMarkdown,
  loadSavedSessions,
  persistSavedSessions,
  removeSavedSession,
  SESSION_HISTORY_STORAGE_KEY,
  summarizeSessionTitle,
} from "./sessionHistory";

describe("session history", () => {
  it("summarizes a Chinese session title from transcript content", () => {
    const session = createInitialSessionState("s1");
    session.transcript.push("能否概括一下时间线？");
    session.suggestions.push("先列出关键日期，再说明负责人和风险。");

    expect(summarizeSessionTitle(session)).toBe("能否概括一下时间线先列出");
  });

  it("creates, persists, loads, and removes saved sessions", () => {
    const storage = new Map<string, string>();
    const api = {
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
    } as Storage;

    const session = createInitialSessionState("session-1");
    session.transcript.push("你好");
    session.suggestions.push("你好，有什么可以帮你？");

    const saved = createSavedSession(session, new Date("2026-06-14T10:00:00.000Z"));
    persistSavedSessions(api, [saved]);

    expect(loadSavedSessions(api)).toHaveLength(1);
    expect(storage.get(SESSION_HISTORY_STORAGE_KEY)).toContain("session-1");

    const next = removeSavedSession(api, [saved], saved.id);
    expect(next).toHaveLength(0);
    expect(loadSavedSessions(api)).toHaveLength(0);
  });

  it("exports saved sessions as markdown", () => {
    const session = createInitialSessionState("session-2");
    session.transcript.push("测试转写");
    session.suggestions.push("测试建议");
    const saved = createSavedSession(session, new Date("2026-06-14T12:00:00.000Z"));

    const markdown = exportSavedSessionMarkdown(saved);

    expect(markdown).toContain("测试转写");
    expect(markdown).toContain("测试建议");
    expect(markdown).toContain("时间线");
  });
});
