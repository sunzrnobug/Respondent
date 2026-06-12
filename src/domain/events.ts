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

export function isRealtimeEvent(value: unknown): value is RealtimeEvent {
  if (!value || typeof value !== "object") return false;
  const candidate = value as { type?: unknown };
  return (
    typeof candidate.type === "string" &&
    allowedTypes.has(candidate.type as RealtimeEvent["type"])
  );
}
