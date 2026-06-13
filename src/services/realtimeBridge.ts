import { listen } from "@tauri-apps/api/event";
import { isRealtimeEvent, type RealtimeEvent } from "../domain/events";
import type { StopRealtimeSession } from "./mockRealtime";

export const REALTIME_EVENT_NAME = "realtime-event";

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
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
