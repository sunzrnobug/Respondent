import {
  exportMarkdown,
  type SessionExport,
  type SessionExportEvent,
} from "../domain/exportTranscript";
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
  return exportMarkdown(exportSession);
}

export function loadSavedSessions(storage: Storage): SavedSession[] {
  const raw = storage.getItem(SESSION_HISTORY_STORAGE_KEY);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isSavedSession);
  } catch {
    return [];
  }
}

export function persistSavedSessions(
  storage: Storage,
  sessions: SavedSession[],
): void {
  storage.setItem(SESSION_HISTORY_STORAGE_KEY, JSON.stringify(sessions));
}

export function removeSavedSession(
  storage: Storage,
  sessions: SavedSession[],
  sessionId: string,
): SavedSession[] {
  const next = sessions.filter((session) => session.id !== sessionId);
  persistSavedSessions(storage, next);
  return next;
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
