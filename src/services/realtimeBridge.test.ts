import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RealtimeEvent } from "../domain/events";

const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

import {
  listenNativeRealtimeEvents,
  REALTIME_EVENT_NAME,
} from "./realtimeBridge";

describe("realtimeBridge", () => {
  beforeEach(() => {
    listenMock.mockReset();
  });

  it("listens on the realtime event name and forwards valid payloads", async () => {
    const unlisten = vi.fn();
    listenMock.mockImplementation(async (_name, handler) => {
      handler({
        payload: {
          type: "transcript.partial",
          sessionId: "s1",
          text: "hello",
          startedAtMs: 0,
          endedAtMs: 20,
          receivedAtMs: 25,
        } satisfies RealtimeEvent,
      });
      return unlisten;
    });
    const emit = vi.fn();

    const stop = await listenNativeRealtimeEvents(emit);

    expect(listenMock).toHaveBeenCalledWith(
      REALTIME_EVENT_NAME,
      expect.any(Function),
    );
    expect(emit).toHaveBeenCalledWith(
      expect.objectContaining({ type: "transcript.partial", text: "hello" }),
    );
    stop();
    expect(unlisten).toHaveBeenCalled();
  });

  it("ignores invalid payloads", async () => {
    listenMock.mockImplementation(async (_name, handler) => {
      handler({ payload: { type: "nope" } });
      return vi.fn();
    });
    const emit = vi.fn();

    await listenNativeRealtimeEvents(emit);

    expect(emit).not.toHaveBeenCalled();
  });
});
