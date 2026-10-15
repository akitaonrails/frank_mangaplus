export interface ReaderResizeAnchorOptions {
  canAnchor: () => boolean;
  getPageIndex: () => number;
  restorePage: (pageIndex: number) => void;
  settleMs?: number;
  requestFrame?: (callback: FrameRequestCallback) => number;
  cancelFrame?: (handle: number) => void;
  setTimer?: (callback: () => void, delayMs: number) => TimerHandle;
  clearTimer?: (handle: TimerHandle) => void;
}

type TimerHandle = ReturnType<typeof setTimeout> | number;

/**
 * Keeps the first visible page stable while a burst of window resize
 * events changes the height of every reader frame.
 */
export function createReaderResizeAnchor(options: ReaderResizeAnchorOptions) {
  const settleMs = options.settleMs ?? 250;
  const requestFrame = options.requestFrame ?? ((callback: FrameRequestCallback) => requestAnimationFrame(callback));
  const cancelFrame = options.cancelFrame ?? ((handle: number) => cancelAnimationFrame(handle));
  const setTimer = options.setTimer ?? ((callback: () => void, delayMs: number) => setTimeout(callback, delayMs));
  const clearTimer = options.clearTimer ?? ((handle: TimerHandle) => clearTimeout(handle));

  let anchorPage: number | null = null;
  let frameHandle: number | null = null;
  let settleTimer: TimerHandle | undefined;

  function restore() {
    if (anchorPage != null) options.restorePage(anchorPage);
  }

  function onResize() {
    if (!options.canAnchor()) return;

    // Keep the first page through the entire burst. Later resize events
    // may arrive after layout has already moved the viewport elsewhere.
    if (anchorPage == null) anchorPage = options.getPageIndex();

    if (frameHandle != null) cancelFrame(frameHandle);
    frameHandle = requestFrame(() => {
      frameHandle = null;
      restore();
    });

    if (settleTimer != null) clearTimer(settleTimer);
    settleTimer = setTimer(() => {
      settleTimer = undefined;
      restore();
      anchorPage = null;
      if (frameHandle != null) {
        cancelFrame(frameHandle);
        frameHandle = null;
      }
    }, settleMs);
  }

  function cancel() {
    anchorPage = null;
    if (frameHandle != null) {
      cancelFrame(frameHandle);
      frameHandle = null;
    }
    if (settleTimer != null) {
      clearTimer(settleTimer);
      settleTimer = undefined;
    }
  }

  return {
    get active() {
      return anchorPage != null;
    },
    onResize,
    cancel,
  };
}
