export type TranscriptPartialEvent = {
  type: "transcript.partial";
  sessionId: string;
  text: string;
  startedAtMs: number;
  endedAtMs: number;
  receivedAtMs: number;
};

export type TranscriptFinalEvent = {
  type: "transcript.final";
  sessionId: string;
  text: string;
  startedAtMs: number;
  endedAtMs: number;
  receivedAtMs: number;
};

export type EndpointEvent = {
  type: "endpoint.detected";
  sessionId: string;
  silenceMs: number;
  detectedAtMs: number;
};

export type ReplyStartedEvent = {
  type: "reply.started";
  sessionId: string;
  generationId: string;
  basedOnTranscriptEventId: string;
  receivedAtMs: number;
};

export type ReplyTokenEvent = {
  type: "reply.token";
  sessionId: string;
  generationId: string;
  token: string;
  receivedAtMs: number;
};

export type ReplyFinalEvent = {
  type: "reply.final";
  sessionId: string;
  generationId: string;
  text: string;
  receivedAtMs: number;
};

export type SystemEvent = {
  type: "system.status";
  sessionId?: string;
  level: "info" | "warning" | "error";
  message: string;
  receivedAtMs: number;
};

export type RealtimeEvent =
  | TranscriptPartialEvent
  | TranscriptFinalEvent
  | EndpointEvent
  | ReplyStartedEvent
  | ReplyTokenEvent
  | ReplyFinalEvent
  | SystemEvent;

const allowedTypes = new Set<RealtimeEvent["type"]>([
  "transcript.partial",
  "transcript.final",
  "endpoint.detected",
  "reply.started",
  "reply.token",
  "reply.final",
  "system.status",
]);

function hasStringFields(
  value: Record<string, unknown>,
  fields: readonly string[],
): boolean {
  return fields.every((field) => typeof value[field] === "string");
}

function hasNumberFields(
  value: Record<string, unknown>,
  fields: readonly string[],
): boolean {
  return fields.every(
    (field) => typeof value[field] === "number" && Number.isFinite(value[field]),
  );
}

function isTranscriptEvent(value: Record<string, unknown>): boolean {
  return (
    hasStringFields(value, ["sessionId", "text"]) &&
    hasNumberFields(value, ["startedAtMs", "endedAtMs", "receivedAtMs"])
  );
}

function isSystemLevel(value: unknown): value is SystemEvent["level"] {
  return value === "info" || value === "warning" || value === "error";
}

export function isRealtimeEvent(value: unknown): value is RealtimeEvent {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.type !== "string" ||
    !allowedTypes.has(candidate.type as RealtimeEvent["type"])
  ) {
    return false;
  }

  switch (candidate.type) {
    case "transcript.partial":
    case "transcript.final":
      return isTranscriptEvent(candidate);
    case "endpoint.detected":
      return (
        hasStringFields(candidate, ["sessionId"]) &&
        hasNumberFields(candidate, ["silenceMs", "detectedAtMs"])
      );
    case "reply.started":
      return (
        hasStringFields(candidate, [
          "sessionId",
          "generationId",
          "basedOnTranscriptEventId",
        ]) && hasNumberFields(candidate, ["receivedAtMs"])
      );
    case "reply.token":
      return (
        hasStringFields(candidate, ["sessionId", "generationId", "token"]) &&
        hasNumberFields(candidate, ["receivedAtMs"])
      );
    case "reply.final":
      return (
        hasStringFields(candidate, ["sessionId", "generationId", "text"]) &&
        hasNumberFields(candidate, ["receivedAtMs"])
      );
    case "system.status":
      return (
        (candidate.sessionId === undefined ||
          typeof candidate.sessionId === "string") &&
        isSystemLevel(candidate.level) &&
        hasStringFields(candidate, ["message"]) &&
        hasNumberFields(candidate, ["receivedAtMs"])
      );
    default:
      return false;
  }
}
