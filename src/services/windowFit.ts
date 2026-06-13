import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { isTauriRuntime } from "./realtimeBridge";

function measureNaturalContentHeight(element: HTMLElement): number {
  const previousMinHeight = element.style.minHeight;
  element.style.minHeight = "";
  const height = Math.ceil(element.getBoundingClientRect().height);
  element.style.minHeight = previousMinHeight;
  return height;
}

export function setupMainWindowFit(element: HTMLElement | null): () => void {
  if (!isTauriRuntime() || !element) {
    return () => {};
  }

  const window = getCurrentWindow();
  let frame = 0;
  let lastContentHeight = 0;
  let initialSyncDone = false;

  const applyShellFill = async () => {
    const [currentSize, scaleFactor] = await Promise.all([
      window.innerSize(),
      window.scaleFactor(),
    ]);
    const windowHeight = Math.ceil(currentSize.height / scaleFactor);
    element.style.minHeight = `${windowHeight}px`;
  };

  const finishInitialSync = async () => {
    initialSyncDone = true;
    await applyShellFill();
  };

  const sync = (initial = false) => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const contentHeight = measureNaturalContentHeight(element);
      if (contentHeight <= 0) {
        if (initial) {
          void finishInitialSync();
        }
        return;
      }

      if (contentHeight === lastContentHeight) {
        if (initial) {
          void finishInitialSync();
        }
        return;
      }

      lastContentHeight = contentHeight;

      void Promise.all([window.innerSize(), window.scaleFactor()])
        .then(async ([currentSize, scaleFactor]) => {
          const currentLogicalWidth = Math.ceil(currentSize.width / scaleFactor);
          const currentLogicalHeight = Math.ceil(
            currentSize.height / scaleFactor,
          );

          const nextHeight = initial
            ? contentHeight
            : Math.max(contentHeight, currentLogicalHeight);

          if (!initial && nextHeight === currentLogicalHeight) {
            return;
          }

          if (initial && nextHeight === currentLogicalHeight) {
            await finishInitialSync();
            return;
          }

          await window.setSize(
            new LogicalSize(currentLogicalWidth, nextHeight),
          );

          if (initial) {
            await finishInitialSync();
          } else {
            await applyShellFill();
          }
        })
        .catch((error) => {
          console.error("Failed to resize main window", error);
          if (initial) {
            void finishInitialSync();
          }
        });
    });
  };

  const handleResize = () => {
    if (!initialSyncDone) {
      return;
    }
    void applyShellFill();
  };

  globalThis.addEventListener("resize", handleResize);
  const mutationObserver = new MutationObserver(() => sync(false));
  mutationObserver.observe(element, {
    attributes: true,
    characterData: true,
    childList: true,
    subtree: true,
  });
  sync(true);

  return () => {
    cancelAnimationFrame(frame);
    mutationObserver.disconnect();
    globalThis.removeEventListener("resize", handleResize);
    element.style.minHeight = "";
  };
}
