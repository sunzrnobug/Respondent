import { listen } from "@tauri-apps/api/event";
import { isRealtimeEvent, type RealtimeEvent } from "../domain/events";
import type { StopRealtimeSession } from "./mockRealtime";

export const REALTIME_EVENT_NAME = "realtime-event";

export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;

  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__?: { invoke?: unknown };
    }
  ).__TAURI_INTERNALS__;

  return typeof internals?.invoke === "function";
}

export async function listenNativeRealtimeEvents(
  emit: (event: RealtimeEvent) => void,
): Promise<StopRealtimeSession> {
  const unlisten = await listen<unknown>(REALTIME_EVENT_NAME, (event) => {
    if (isRealtimeEvent(event.payload)) {
      emit(event.payload);
    }
  });
  return unlisten;
}
