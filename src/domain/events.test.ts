import { describe, expect, it } from "vitest";
import { isRealtimeEvent } from "./events";

describe("isRealtimeEvent", () => {
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
});
