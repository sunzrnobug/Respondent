import { describe, expect, it } from "vitest";
import { isRealtimeEvent } from "./events";

describe("isRealtimeEvent", () => {
  it.each([
    [
      "final transcript",
      {
        type: "transcript.final",
        sessionId: "s1",
        text: "hello",
        startedAtMs: 10,
        endedAtMs: 320,
        receivedAtMs: 350,
      },
    ],
    [
      "endpoint detection",
      {
        type: "endpoint.detected",
        sessionId: "s1",
        silenceMs: 900,
        detectedAtMs: 1200,
      },
    ],
    [
      "reply started",
      {
        type: "reply.started",
        sessionId: "s1",
        generationId: "g1",
        basedOnTranscriptEventId: "t1",
        receivedAtMs: 400,
      },
    ],
    [
      "reply final",
      {
        type: "reply.final",
        sessionId: "s1",
        generationId: "g1",
        text: "Done",
        receivedAtMs: 800,
      },
    ],
    [
      "system status",
      {
        type: "system.status",
        sessionId: "s1",
        level: "info",
        message: "Connected",
        receivedAtMs: 900,
      },
    ],
    [
      "system status without a session",
      {
        type: "system.status",
        level: "warning",
        message: "Reconnecting",
        receivedAtMs: 950,
      },
    ],
  ])("accepts %s events", (_name, event) => {
    expect(isRealtimeEvent(event)).toBe(true);
  });

  it("accepts partial transcript events", () => {
    expect(
      isRealtimeEvent({
        type: "transcript.partial",
        sessionId: "s1",
        text: "hello",
        startedAtMs: 10,
        endedAtMs: 320,
        receivedAtMs: 350,
      }),
    ).toBe(true);
  });

  it("accepts reply token events", () => {
    expect(
      isRealtimeEvent({
        type: "reply.token",
        sessionId: "s1",
        generationId: "g1",
        token: "Yes",
        receivedAtMs: 500,
      }),
    ).toBe(true);
  });

  it("rejects microphone events because the MVP has no microphone path", () => {
    expect(
      isRealtimeEvent({
        type: "microphone.frame",
        sessionId: "s1",
      }),
    ).toBe(false);
  });

  it("rejects reply token events with missing required fields", () => {
    expect(isRealtimeEvent({ type: "reply.token" })).toBe(false);
  });

  it("rejects final transcript events with malformed text or timing", () => {
    expect(
      isRealtimeEvent({
        type: "transcript.final",
        sessionId: "s1",
        text: 42,
        startedAtMs: 10,
        endedAtMs: 320,
        receivedAtMs: 350,
      }),
    ).toBe(false);

    expect(
      isRealtimeEvent({
        type: "transcript.final",
        sessionId: "s1",
        text: "hello",
        startedAtMs: "10",
        endedAtMs: 320,
        receivedAtMs: 350,
      }),
    ).toBe(false);
  });

  it("rejects system status events with invalid levels", () => {
    expect(
      isRealtimeEvent({
        type: "system.status",
        level: "debug",
        message: "Connected",
        receivedAtMs: 400,
      }),
    ).toBe(false);
  });

  it.each([
    [
      "partial transcript",
      {
        type: "transcript.partial",
        sessionId: "s1",
        text: "hello",
        startedAtMs: 10,
        endedAtMs: 320,
      },
    ],
    [
      "endpoint detection",
      {
        type: "endpoint.detected",
        sessionId: "s1",
        silenceMs: "900",
        detectedAtMs: 1200,
      },
    ],
    [
      "reply started",
      {
        type: "reply.started",
        sessionId: "s1",
        generationId: "g1",
        basedOnTranscriptEventId: 42,
        receivedAtMs: 400,
      },
    ],
    [
      "reply final",
      {
        type: "reply.final",
        sessionId: "s1",
        generationId: "g1",
        text: "Done",
      },
    ],
    [
      "system status",
      {
        type: "system.status",
        sessionId: 123,
        level: "error",
        message: "Disconnected",
        receivedAtMs: 900,
      },
    ],
  ])("rejects malformed %s events", (_name, event) => {
    expect(isRealtimeEvent(event)).toBe(false);
  });
});
