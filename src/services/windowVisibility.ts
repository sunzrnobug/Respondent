import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./realtimeBridge";

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
    return true;
  }
  return target.isContentEditable;
}

export function isNumpadEnter(event: KeyboardEvent): boolean {
  return (
    event.key === "Enter" &&
    event.code === "NumpadEnter" &&
    event.location === KeyboardEvent.DOM_KEY_LOCATION_NUMPAD
  );
}

export async function toggleMainWindowVisibility(): Promise<boolean> {
  return invoke<boolean>("toggle_main_window_visibility");
}

export function setupEnterVisibilityToggle(): () => void {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  const onKeyDown = (event: KeyboardEvent) => {
    if (!isNumpadEnter(event) || isEditableTarget(event.target)) {
      return;
    }
    event.preventDefault();
    void toggleMainWindowVisibility();
  };

  window.addEventListener("keydown", onKeyDown);
  return () => window.removeEventListener("keydown", onKeyDown);
}
