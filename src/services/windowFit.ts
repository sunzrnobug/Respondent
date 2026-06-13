import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { isTauriRuntime } from "./realtimeBridge";

export function setupMainWindowFit(element: HTMLElement | null): () => void {
  if (!isTauriRuntime() || !element) {
    return () => {};
  }

  const window = getCurrentWindow();
  let frame = 0;
  let lastWidth = 0;
  let lastHeight = 0;

  const sync = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const { width, height } = element.getBoundingClientRect();
      const nextHeight = Math.ceil(height);
      const measuredWidth = Math.ceil(width);

      if (
        measuredWidth <= 0 ||
        nextHeight <= 0 ||
        (measuredWidth === lastWidth && nextHeight === lastHeight)
      ) {
        return;
      }

      lastWidth = measuredWidth;
      lastHeight = nextHeight;
      void Promise.all([window.innerSize(), window.scaleFactor()])
        .then(([currentSize, scaleFactor]) => {
          const currentLogicalWidth = Math.ceil(currentSize.width / scaleFactor);
          return window.setSize(new LogicalSize(currentLogicalWidth, nextHeight));
        })
        .catch((error) => {
          console.error("Failed to resize main window", error);
        });
    });
  };

  const observer = new ResizeObserver(sync);
  observer.observe(element);
  const mutationObserver = new MutationObserver(sync);
  mutationObserver.observe(element, {
    attributes: true,
    characterData: true,
    childList: true,
    subtree: true,
  });
  sync();

  return () => {
    cancelAnimationFrame(frame);
    observer.disconnect();
    mutationObserver.disconnect();
  };
}
