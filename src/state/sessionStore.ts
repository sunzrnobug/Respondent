import type { RealtimeEvent } from "../domain/events";

export type SessionState = {
  sessionId: string;
  nativeSessionId: string | null;
  status: "idle" | "listening" | "paused" | "ended";
  liveSubtitle: string;
  transcript: string[];
  currentGenerationId: string | null;
  currentSuggestion: string;
  suggestions: string[];
  generationToTurnIndex: Record<string, number>;
  systemMessages: string[];
};

export type SessionTurn = {
  transcript: string;
  suggestion?: string;
};

export function buildSessionTurns(session: SessionState): SessionTurn[] {
  return session.transcript.map((transcript, index) => ({
    transcript,
    suggestion: session.suggestions[index],
  }));
}

export function createInitialSessionState(sessionId: string): SessionState {
  return {
    sessionId,
    nativeSessionId: null,
    status: "idle",
    liveSubtitle: "",
    transcript: [],
    currentGenerationId: null,
    currentSuggestion: "",
    suggestions: [],
    generationToTurnIndex: {},
    systemMessages: [],
  };
}

function writeSuggestionAt(
  suggestions: string[],
  index: number,
  text: string,
): string[] {
  const next = [...suggestions];
  while (next.length <= index) next.push("");
  next[index] = text;
  return next;
}

export function reduceSessionEvent(
  state: SessionState,
  event: RealtimeEvent,
): SessionState {
  const routingId = state.nativeSessionId ?? state.sessionId;
  if ("sessionId" in event && event.sessionId && event.sessionId !== routingId) {
    return state;
  }

  if (event.type === "transcript.partial") {
    return { ...state, liveSubtitle: event.text };
  }

  if (event.type === "transcript.final") {
    return {
      ...state,
      liveSubtitle: "",
      transcript: [...state.transcript, event.text],
    };
  }

  if (event.type === "reply.started") {
    return {
      ...state,
      currentGenerationId: event.generationId,
      currentSuggestion: "",
      generationToTurnIndex: {
        ...state.generationToTurnIndex,
        [event.generationId]: state.transcript.length - 1,
      },
    };
  }

  if (
    event.type === "reply.token" &&
    event.generationId === state.currentGenerationId
  ) {
    return {
      ...state,
      currentSuggestion: `${state.currentSuggestion}${event.token}`,
    };
  }

  if (
    event.type === "reply.cancelled" &&
    event.generationId === state.currentGenerationId
  ) {
    const index = state.generationToTurnIndex[event.generationId] ?? -1;
    return {
      ...state,
      currentGenerationId: null,
      currentSuggestion: "",
      suggestions:
        index >= 0
          ? writeSuggestionAt(state.suggestions, index, "")
          : state.suggestions,
    };
  }

  if (
    event.type === "reply.final" &&
    event.generationId === state.currentGenerationId
  ) {
    const index = state.generationToTurnIndex[event.generationId] ?? -1;
    return {
      ...state,
      currentGenerationId: null,
      currentSuggestion: event.text,
      suggestions:
        index >= 0
          ? writeSuggestionAt(state.suggestions, index, event.text)
          : [...state.suggestions, event.text],
    };
  }

  if (event.type === "system.status") {
    return {
      ...state,
      systemMessages: [...state.systemMessages, event.message],
    };
  }

  return state;
}
