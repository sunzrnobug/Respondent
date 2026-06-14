import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { isTauriRuntime } from "./realtimeBridge";

const windowFitRemeasureRegistry = new WeakMap<
  HTMLElement,
  (options?: { forceContentShrink?: boolean }) => void
>();

function measureNaturalContentHeight(element: HTMLElement): number {
  const previousMinHeight = element.style.minHeight;
  const previousHeight = element.style.height;
  element.style.minHeight = "";
  element.style.height = "auto";
  const height = Math.ceil(element.getBoundingClientRect().height);
  element.style.minHeight = previousMinHeight;
  element.style.height = previousHeight;
  return height;
}

export function remeasureMainWindowFit(
  element: HTMLElement | null,
  options?: { forceContentShrink?: boolean },
) {
  if (!element) return;
  windowFitRemeasureRegistry.get(element)?.(options);
}

export function setupMainWindowFit(element: HTMLElement | null): () => void {
  if (!isTauriRuntime() || !element) {
    return () => {};
  }

  const window = getCurrentWindow();
  let frame = 0;
  let lastContentHeight = 0;
  let initialSyncDone = false;
  let userExpandedAboveContent = false;
  let ignoreResizeUntil = 0;
  let forceContentShrink = false;
  let lastProgrammaticHeight = 0;

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

  const sync = (initial = false, options?: { forceContentShrink?: boolean }) => {
    if (options?.forceContentShrink) {
      forceContentShrink = true;
    }
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const contentHeight = measureNaturalContentHeight(element);
      if (contentHeight <= 0) {
        if (initial) {
          void finishInitialSync();
        }
        return;
      }

      void Promise.all([window.innerSize(), window.scaleFactor()])
        .then(async ([currentSize, scaleFactor]) => {
          const currentLogicalWidth = Math.ceil(currentSize.width / scaleFactor);
          const currentLogicalHeight = Math.ceil(
            currentSize.height / scaleFactor,
          );

          const nextHeight = (() => {
            if (initial || forceContentShrink) {
              forceContentShrink = false;
              return contentHeight;
            }
            if (
              contentHeight < currentLogicalHeight &&
              (userExpandedAboveContent ||
                currentLogicalHeight > lastProgrammaticHeight)
            ) {
              return currentLogicalHeight;
            }
            return contentHeight;
          })();

          const alreadyFits = Math.abs(nextHeight - currentLogicalHeight) <= 1;
          if (
            !initial &&
            alreadyFits &&
            contentHeight === lastContentHeight
          ) {
            return;
          }

          if (initial && alreadyFits) {
            lastContentHeight = contentHeight;
            await finishInitialSync();
            return;
          }

          if (!initial && alreadyFits && nextHeight !== contentHeight) {
            return;
          }

          if (Math.abs(nextHeight - currentLogicalHeight) <= 1) {
            lastContentHeight = contentHeight;
            if (initial) {
              await finishInitialSync();
            } else {
              await applyShellFill();
            }
            return;
          }

          ignoreResizeUntil = Date.now() + 200;
          await window.setSize(
            new LogicalSize(currentLogicalWidth, nextHeight),
          );

          lastProgrammaticHeight = nextHeight;
          lastContentHeight = contentHeight;
          if (nextHeight === contentHeight) {
            userExpandedAboveContent = false;
          }

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
    if (Date.now() < ignoreResizeUntil) {
      void applyShellFill();
      return;
    }
    void Promise.all([window.innerSize(), window.scaleFactor()])
      .then(async ([currentSize, scaleFactor]) => {
        const windowHeight = Math.ceil(currentSize.height / scaleFactor);
        const naturalHeight = measureNaturalContentHeight(element);
        if (windowHeight > lastProgrammaticHeight) {
          userExpandedAboveContent = true;
        } else if (windowHeight <= naturalHeight) {
          userExpandedAboveContent = false;
        }
        await applyShellFill();
      })
      .catch((error) => {
        console.error("Failed to stretch main window shell", error);
      });
  };

  const remeasure = (options?: { forceContentShrink?: boolean }) =>
    sync(false, options);

  windowFitRemeasureRegistry.set(element, remeasure);

  globalThis.addEventListener("resize", handleResize);
  const mutationObserver = new MutationObserver(() => remeasure());
  mutationObserver.observe(element, {
    attributes: true,
    characterData: true,
    childList: true,
    subtree: true,
  });

  const resizeObserver = new ResizeObserver(() => remeasure());
  resizeObserver.observe(element);

  sync(true);

  return () => {
    cancelAnimationFrame(frame);
    mutationObserver.disconnect();
    resizeObserver.disconnect();
    globalThis.removeEventListener("resize", handleResize);
    windowFitRemeasureRegistry.delete(element);
    element.style.minHeight = "";
    element.style.height = "";
  };
}

export function setupDialogWindowFit(element: HTMLElement | null): () => void {
  if (!isTauriRuntime() || !element) {
    return () => {};
  }

  const window = getCurrentWindow();
  let frame = 0;
  let lastHeight = 0;

  const measureTarget = () =>
    (element.closest(".dialogWindowRoot") as HTMLElement | null) ?? element;

  const sync = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const height = Math.ceil(measureTarget().getBoundingClientRect().height);
      if (height <= 0 || height === lastHeight) {
        return;
      }
      lastHeight = height;

      void Promise.all([window.innerSize(), window.scaleFactor()])
        .then(async ([currentSize, scaleFactor]) => {
          const logicalWidth = Math.ceil(currentSize.width / scaleFactor);
          await window.setSize(new LogicalSize(logicalWidth, height));
        })
        .catch((error) => {
          console.error("Failed to resize dialog window", error);
        });
    });
  };

  const mutationObserver = new MutationObserver(sync);
  mutationObserver.observe(element, {
    attributes: true,
    characterData: true,
    childList: true,
    subtree: true,
  });

  const resizeObserver = new ResizeObserver(sync);
  resizeObserver.observe(element);
  const root = measureTarget();
  if (root !== element) {
    resizeObserver.observe(root);
  }

  sync();

  return () => {
    cancelAnimationFrame(frame);
    mutationObserver.disconnect();
    resizeObserver.disconnect();
  };
}
