import { describe, expect, it } from "vitest";
import { createTranscriptEngine } from "./transcriptEngine";

describe("transcript engine", () => {
  it("shows partial text but only stores final text in history", () => {
    const engine = createTranscriptEngine("s1");

    engine.apply({
      type: "transcript.partial",
      sessionId: "s1",
      text: "I can",
      startedAtMs: 0,
      endedAtMs: 300,
      receivedAtMs: 350,
    });
    expect(engine.snapshot().livePartial).toBe("I can");
    expect(engine.snapshot().finalTurns).toEqual([]);

    engine.apply({
      type: "transcript.partial",
      sessionId: "s1",
      text: "I can help",
      startedAtMs: 0,
      endedAtMs: 700,
      receivedAtMs: 760,
    });
    expect(engine.snapshot().livePartial).toBe("I can help");
    expect(engine.snapshot().finalTurns).toEqual([]);

    engine.apply({
      type: "transcript.final",
      sessionId: "s1",
      text: "I can help.",
      startedAtMs: 0,
      endedAtMs: 900,
      receivedAtMs: 1100,
    });

    expect(engine.snapshot().livePartial).toBe("");
    expect(engine.snapshot().finalTurns).toEqual([
      { text: "I can help.", startedAtMs: 0, endedAtMs: 900 },
    ]);
  });

  it("ignores events from another session", () => {
    const engine = createTranscriptEngine("s1");

    engine.apply({
      type: "transcript.partial",
      sessionId: "s2",
      text: "wrong partial",
      startedAtMs: 0,
      endedAtMs: 50,
      receivedAtMs: 80,
    });
    engine.apply({
      type: "transcript.final",
      sessionId: "s2",
      text: "wrong session",
      startedAtMs: 0,
      endedAtMs: 100,
      receivedAtMs: 150,
    });

    expect(engine.snapshot().livePartial).toBe("");
    expect(engine.snapshot().finalTurns).toEqual([]);
  });
});
