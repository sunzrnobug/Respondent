import {
  exportMarkdown,
  type SessionExport,
  type SessionExportEvent,
} from "../domain/exportTranscript";
import { isTauriRuntime } from "../services/realtimeBridge";
import {
  deleteSavedSession as deleteSavedSessionCommand,
  importLegacySavedSessions,
  listSavedSessions as listSavedSessionsCommand,
  upsertSavedSession as upsertSavedSessionCommand,
} from "../services/tauriApi";
import type { SessionState } from "./sessionStore";
import { buildSessionTurns } from "./sessionStore";

export type SessionTurn = {
  transcript: string;
  suggestion?: string;
};

export type SavedSession = {
  id: string;
  title: string;
  date: string;
  startedAt: string;
  endedAt: string;
  turns: SessionTurn[];
  systemMessages: string[];
};

export const SESSION_HISTORY_STORAGE_KEY = "respondent.savedSessions";
export const SESSION_HISTORY_RETENTION_DAYS = 90;
const EXPORT_SENSITIVITY_NOTICE =
  "> **注意**：导出内容包含会议敏感文本，请妥善保管。\n\n";

const TITLE_STOP_WORDS = new Set([
  "a",
  "an",
  "and",
  "before",
  "call",
  "could",
  "out",
  "start",
  "summarize",
  "the",
  "then",
  "what",
  "with",
  "you",
]);

export { buildSessionTurns };

export function summarizeSessionTitle(session: SessionState): string {
  const source = [
    ...session.transcript,
    ...session.suggestions,
    session.currentSuggestion,
    ...session.systemMessages,
  ].join(" ");

  const cjk = source.replace(/[^\u4e00-\u9fff]/g, "");
  if (cjk.length > 0) {
    const title = cjk.slice(0, 12);
    return title || "未命名会话";
  }

  const words = source
    .replace(/[“”"'.?!,;:()[\]{}]/g, "")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => word.toLowerCase())
    .filter((word) => !TITLE_STOP_WORDS.has(word));
  const titleWords: string[] = [];
  for (const word of words) {
    if (titleWords.includes(word)) continue;
    titleWords.push(word);
    if (titleWords.length === 5) break;
  }

  if (titleWords.length === 0) return "未命名会话";
  const title = titleWords.join(" ");
  return `${title.charAt(0).toUpperCase()}${title.slice(1)}`;
}

export function createSavedSession(
  session: SessionState,
  endedAt: Date = new Date(),
): SavedSession {
  const endedIso = endedAt.toISOString();
  return {
    id: session.sessionId,
    title: summarizeSessionTitle(session),
    date: endedIso.slice(0, 10),
    startedAt: endedIso,
    endedAt: endedIso,
    turns: buildSessionTurns(session),
    systemMessages: [...session.systemMessages],
  };
}

function savedSessionEvents(session: SavedSession): SessionExportEvent[] {
  const events: SessionExportEvent[] = [];
  session.turns.forEach((turn, index) => {
    const atMs = index * 2_000;
    events.push({
      type: "transcript",
      text: turn.transcript,
      atMs,
    });
    if (turn.suggestion) {
      events.push({
        type: "suggestion",
        text: turn.suggestion,
        atMs: atMs + 1_000,
      });
    }
  });
  session.systemMessages.forEach((message, index) => {
    events.push({
      type: "system",
      text: message,
      atMs: session.turns.length * 2_000 + index * 1_000,
    });
  });
  return events;
}

export function exportSavedSessionMarkdown(session: SavedSession): string {
  const exportSession: SessionExport = {
    title: session.title,
    startedAt: session.startedAt,
    endedAt: session.endedAt,
    events: savedSessionEvents(session),
  };
  return `${EXPORT_SENSITIVITY_NOTICE}${exportMarkdown(exportSession)}`;
}

function loadSavedSessionsFromStorage(storage: Storage): SavedSession[] {
  const raw = storage.getItem(SESSION_HISTORY_STORAGE_KEY);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const sessions = parsed.filter(isSavedSession);
    const retained = purgeExpiredSessions(sessions);
    if (retained.length !== sessions.length) {
      persistSavedSessionsToStorage(storage, retained);
    }
    return retained;
  } catch {
    return [];
  }
}

export async function loadSavedSessions(): Promise<SavedSession[]> {
  if (isTauriRuntime()) {
    return listSavedSessionsCommand();
  }
  if (typeof window === "undefined") return [];
  return loadSavedSessionsFromStorage(window.localStorage);
}

export function purgeExpiredSessions(
  sessions: SavedSession[],
  retentionDays: number = SESSION_HISTORY_RETENTION_DAYS,
): SavedSession[] {
  const cutoffMs =
    Date.now() - Math.max(1, retentionDays) * 24 * 60 * 60 * 1000;
  return sessions.filter((session) => {
    const endedAt = Date.parse(session.endedAt);
    return Number.isFinite(endedAt) && endedAt >= cutoffMs;
  });
}

function persistSavedSessionsToStorage(
  storage: Storage,
  sessions: SavedSession[],
): void {
  storage.setItem(SESSION_HISTORY_STORAGE_KEY, JSON.stringify(sessions));
}

export async function persistSavedSession(session: SavedSession): Promise<void> {
  if (isTauriRuntime()) {
    await upsertSavedSessionCommand(session);
    return;
  }
  if (typeof window === "undefined") return;
  const current = loadSavedSessionsFromStorage(window.localStorage);
  const next = [session, ...current.filter((item) => item.id !== session.id)];
  persistSavedSessionsToStorage(window.localStorage, next);
}

export async function removeSavedSessionById(
  sessions: SavedSession[],
  sessionId: string,
): Promise<SavedSession[]> {
  if (isTauriRuntime()) {
    await deleteSavedSessionCommand(sessionId);
    return sessions.filter((session) => session.id !== sessionId);
  }
  if (typeof window === "undefined") {
    return sessions.filter((session) => session.id !== sessionId);
  }
  const next = sessions.filter((session) => session.id !== sessionId);
  persistSavedSessionsToStorage(window.localStorage, next);
  return next;
}

export async function migrateLegacySavedSessionsFromLocalStorage(): Promise<void> {
  if (!isTauriRuntime() || typeof window === "undefined") return;
  const legacy = loadSavedSessionsFromStorage(window.localStorage);
  if (legacy.length === 0) return;
  await importLegacySavedSessions(legacy);
  window.localStorage.removeItem(SESSION_HISTORY_STORAGE_KEY);
}

function isSavedSession(value: unknown): value is SavedSession {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<SavedSession>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.title === "string" &&
    typeof candidate.date === "string" &&
    typeof candidate.startedAt === "string" &&
    typeof candidate.endedAt === "string" &&
    Array.isArray(candidate.turns) &&
    Array.isArray(candidate.systemMessages)
  );
}
