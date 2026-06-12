import { describe, expect, it } from "vitest";
import { createReplyEngine } from "./replyEngine";

describe("reply engine", () => {
  it("does not trigger from partial transcript alone", () => {
    const engine = createReplyEngine("s1");
    const action = engine.apply({
      type: "transcript.partial",
      sessionId: "s1",
      text: "Can you explain",
      startedAtMs: 0,
      endedAtMs: 400,
      receivedAtMs: 450,
    });
    expect(action).toEqual({ type: "none" });
  });

  it("triggers after endpoint and final transcript", () => {
    const engine = createReplyEngine("s1");
    engine.apply({
      type: "endpoint.detected",
      sessionId: "s1",
      silenceMs: 300,
      detectedAtMs: 1200,
    });
    const action = engine.apply({
      type: "transcript.final",
      sessionId: "s1",
      text: "Can you explain the timeline?",
      startedAtMs: 0,
      endedAtMs: 1100,
      receivedAtMs: 1300,
    });
    expect(action).toEqual({
      type: "start-reply",
      transcript: "Can you explain the timeline?",
      context: ["Can you explain the timeline?"],
    });
  });

  it("does not arm a local reply from a foreign endpoint", () => {
    const engine = createReplyEngine("s1");
    engine.apply({
      type: "endpoint.detected",
      sessionId: "s2",
      silenceMs: 300,
      detectedAtMs: 1200,
    });

    const action = engine.apply({
      type: "transcript.final",
      sessionId: "s1",
      text: "Can you explain the timeline?",
      startedAtMs: 0,
      endedAtMs: 1100,
      receivedAtMs: 1300,
    });

    expect(action).toEqual({ type: "none" });
  });

  it("does not add foreign final transcripts to local context", () => {
    const engine = createReplyEngine("s1");
    engine.apply({
      type: "transcript.final",
      sessionId: "s2",
      text: "Use the other session",
      startedAtMs: 0,
      endedAtMs: 1100,
      receivedAtMs: 1300,
    });

    expect(engine.context()).toEqual([]);
  });

  it("returns a start-reply context copy that cannot mutate internal context", () => {
    const engine = createReplyEngine("s1");
    engine.apply({
      type: "endpoint.detected",
      sessionId: "s1",
      silenceMs: 300,
      detectedAtMs: 1200,
    });
    const action = engine.apply({
      type: "transcript.final",
      sessionId: "s1",
      text: "Can you explain the timeline?",
      startedAtMs: 0,
      endedAtMs: 1100,
      receivedAtMs: 1300,
    });

    expect(action.type).toBe("start-reply");
    if (action.type !== "start-reply") return;
    action.context.push("mutated outside");

    expect(engine.context()).toEqual(["Can you explain the timeline?"]);
  });

  it("triggers at most one reply for a single endpoint", () => {
    const engine = createReplyEngine("s1");
    engine.apply({
      type: "endpoint.detected",
      sessionId: "s1",
      silenceMs: 300,
      detectedAtMs: 1200,
    });

    expect(
      engine.apply({
        type: "transcript.final",
        sessionId: "s1",
        text: "First final",
        startedAtMs: 0,
        endedAtMs: 1100,
        receivedAtMs: 1300,
      }),
    ).toEqual({
      type: "start-reply",
      transcript: "First final",
      context: ["First final"],
    });
    expect(
      engine.apply({
        type: "transcript.final",
        sessionId: "s1",
        text: "Second final",
        startedAtMs: 1400,
        endedAtMs: 1800,
        receivedAtMs: 1900,
      }),
    ).toEqual({ type: "none" });
  });

  it("keeps only the latest six final turns in context", () => {
    const engine = createReplyEngine("s1");
    for (let index = 0; index < 7; index += 1) {
      engine.apply({
        type: "endpoint.detected",
        sessionId: "s1",
        silenceMs: 300,
        detectedAtMs: index * 1000 + 500,
      });
      engine.apply({
        type: "transcript.final",
        sessionId: "s1",
        text: `turn ${index}`,
        startedAtMs: index * 1000,
        endedAtMs: index * 1000 + 400,
        receivedAtMs: index * 1000 + 600,
      });
    }
    expect(engine.context()).toEqual([
      "turn 1",
      "turn 2",
      "turn 3",
      "turn 4",
      "turn 5",
      "turn 6",
    ]);
  });
});
