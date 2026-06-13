import type { RealtimeEvent } from "../domain/events";

export type RealtimeEmit = (event: RealtimeEvent) => void;
export type StopRealtimeSession = () => void;

type ScheduledEvent = {
  delayMs: number;
  event: RealtimeEvent;
};

export function runMockRealtimeSession(
  sessionId: string,
  emit: RealtimeEmit,
): StopRealtimeSession {
  const generationId = "mock-generation-1";
  const events: ScheduledEvent[] = [
    {
      delayMs: 0,
      event: {
        type: "system.status",
        sessionId,
        level: "info",
        message: "演示实时会话已启动",
        receivedAtMs: 0,
      },
    },
    {
      delayMs: 250,
      event: {
        type: "transcript.partial",
        sessionId,
        text: "能否",
        startedAtMs: 0,
        endedAtMs: 240,
        receivedAtMs: 250,
      },
    },
    {
      delayMs: 650,
      event: {
        type: "transcript.partial",
        sessionId,
        text: "能否概括一下时间线",
        startedAtMs: 0,
        endedAtMs: 620,
        receivedAtMs: 650,
      },
    },
    {
      delayMs: 950,
      event: {
        type: "endpoint.detected",
        sessionId,
        silenceMs: 300,
        detectedAtMs: 950,
      },
    },
    {
      delayMs: 1050,
      event: {
        type: "transcript.final",
        sessionId,
        text: "能否概括一下时间线？",
        startedAtMs: 0,
        endedAtMs: 900,
        receivedAtMs: 1050,
      },
    },
    {
      delayMs: 1300,
      event: {
        type: "reply.started",
        sessionId,
        generationId,
        basedOnTranscriptEventId: "mock-transcript-1",
        receivedAtMs: 1300,
      },
    },
    {
      delayMs: 1550,
      event: {
        type: "reply.token",
        sessionId,
        generationId,
        token: "先列出关键日期，",
        receivedAtMs: 1550,
      },
    },
    {
      delayMs: 1800,
      event: {
        type: "reply.token",
        sessionId,
        generationId,
        token: "再说明负责人和风险。",
        receivedAtMs: 1800,
      },
    },
    {
      delayMs: 2100,
      event: {
        type: "reply.final",
        sessionId,
        generationId,
        text: "先列出关键日期，再说明负责人和风险。",
        receivedAtMs: 2100,
      },
    },
    {
      delayMs: 2600,
      event: {
        type: "transcript.partial",
        sessionId,
        text: "负责人那边",
        startedAtMs: 2200,
        endedAtMs: 2550,
        receivedAtMs: 2600,
      },
    },
    {
      delayMs: 2900,
      event: {
        type: "endpoint.detected",
        sessionId,
        silenceMs: 300,
        detectedAtMs: 2900,
      },
    },
    {
      delayMs: 3000,
      event: {
        type: "transcript.final",
        sessionId,
        text: "负责人那边有什么风险？",
        startedAtMs: 2200,
        endedAtMs: 2850,
        receivedAtMs: 3000,
      },
    },
    {
      delayMs: 3250,
      event: {
        type: "reply.started",
        sessionId,
        generationId: "mock-generation-2",
        basedOnTranscriptEventId: "mock-transcript-2",
        receivedAtMs: 3250,
      },
    },
    {
      delayMs: 3500,
      event: {
        type: "reply.final",
        sessionId,
        generationId: "mock-generation-2",
        text: "先确认外部依赖和交付节点，再补充缓解方案。",
        receivedAtMs: 3500,
      },
    },
    {
      delayMs: 3700,
      event: {
        type: "transcript.partial",
        sessionId,
        text: "还有其他问题",
        startedAtMs: 3600,
        endedAtMs: 3650,
        receivedAtMs: 3700,
      },
    },
    {
      delayMs: 4000,
      event: {
        type: "endpoint.detected",
        sessionId,
        silenceMs: 300,
        detectedAtMs: 4000,
      },
    },
    {
      delayMs: 4100,
      event: {
        type: "transcript.final",
        sessionId,
        text: "还有其他问题吗？",
        startedAtMs: 3600,
        endedAtMs: 3950,
        receivedAtMs: 4100,
      },
    },
    {
      delayMs: 4300,
      event: {
        type: "reply.started",
        sessionId,
        generationId: "mock-generation-3",
        basedOnTranscriptEventId: "mock-transcript-3",
        receivedAtMs: 4300,
      },
    },
    {
      delayMs: 4600,
      event: {
        type: "reply.final",
        sessionId,
        generationId: "mock-generation-3",
        text: "请参考会议纪要。",
        receivedAtMs: 4600,
      },
    },
  ];

  const timers = events.map(({ delayMs, event }) =>
    window.setTimeout(() => emit(event), delayMs),
  );

  return () => {
    timers.forEach((timer) => window.clearTimeout(timer));
  };
}

let mockRetryCounter = 0;

export function scheduleMockReplyRetry(
  sessionId: string,
  emit: RealtimeEmit,
): void {
  mockRetryCounter += 1;
  const generationId = `mock-retry-${mockRetryCounter}`;
  const events: ScheduledEvent[] = [
    {
      delayMs: 0,
      event: {
        type: "reply.started",
        sessionId,
        generationId,
        basedOnTranscriptEventId: `mock-retry-${mockRetryCounter}`,
        receivedAtMs: Date.now(),
      },
    },
    {
      delayMs: 180,
      event: {
        type: "reply.token",
        sessionId,
        generationId,
        token: "换个角度说，",
        receivedAtMs: Date.now() + 180,
      },
    },
    {
      delayMs: 360,
      event: {
        type: "reply.token",
        sessionId,
        generationId,
        token: "可以先给结论，再补充关键理由。",
        receivedAtMs: Date.now() + 360,
      },
    },
    {
      delayMs: 540,
      event: {
        type: "reply.final",
        sessionId,
        generationId,
        text: "换个角度说，可以先给结论，再补充关键理由。",
        receivedAtMs: Date.now() + 540,
      },
    },
  ];

  events.forEach(({ delayMs, event }) => {
    window.setTimeout(() => emit(event), delayMs);
  });
}
