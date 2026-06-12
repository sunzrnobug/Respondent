import type { RealtimeEvent } from "./events";

export type TranscriptTurn = {
  text: string;
  startedAtMs: number;
  endedAtMs: number;
};

export type TranscriptSnapshot = {
  sessionId: string;
  livePartial: string;
  finalTurns: TranscriptTurn[];
};

export function createTranscriptEngine(sessionId: string) {
  let livePartial = "";
  const finalTurns: TranscriptTurn[] = [];

  return {
    apply(event: RealtimeEvent): void {
      if ("sessionId" in event && event.sessionId !== sessionId) return;

      if (event.type === "transcript.partial") {
        livePartial = event.text;
      }

      if (event.type === "transcript.final") {
        livePartial = "";
        finalTurns.push({
          text: event.text,
          startedAtMs: event.startedAtMs,
          endedAtMs: event.endedAtMs,
        });
      }
    },
    snapshot(): TranscriptSnapshot {
      return {
        sessionId,
        livePartial,
        finalTurns: [...finalTurns],
      };
    },
  };
}
