import { afterEach, describe, expect, it, vi } from 'vitest';
import { createReaderResizeAnchor } from './readerResize';

afterEach(() => {
  vi.useRealTimers();
});

describe('createReaderResizeAnchor', () => {
  it('keeps the first page through a resize burst and releases it after settling', () => {
    vi.useFakeTimers();
    let currentPage = 12;
    const restored: number[] = [];
    const frames: FrameRequestCallback[] = [];
    const cancelFrame = vi.fn();
    const anchor = createReaderResizeAnchor({
      canAnchor: () => true,
      getPageIndex: () => currentPage,
      restorePage: page => restored.push(page),
      requestFrame: callback => frames.push(callback),
      cancelFrame,
    });

    anchor.onResize();
    currentPage = 99;
    anchor.onResize();

    expect(anchor.active).toBe(true);
    expect(cancelFrame).toHaveBeenCalledWith(1);
    frames[1](0);
    expect(restored).toEqual([12]);

    vi.advanceTimersByTime(250);
    expect(restored).toEqual([12, 12]);
    expect(anchor.active).toBe(false);
  });

  it('does nothing when the reader cannot anchor', () => {
    const restorePage = vi.fn();
    const requestFrame = vi.fn();
    const anchor = createReaderResizeAnchor({
      canAnchor: () => false,
      getPageIndex: () => 4,
      restorePage,
      requestFrame,
    });

    anchor.onResize();

    expect(anchor.active).toBe(false);
    expect(requestFrame).not.toHaveBeenCalled();
    expect(restorePage).not.toHaveBeenCalled();
  });

  it('cancels pending restoration when the reader state resets', () => {
    vi.useFakeTimers();
    const restorePage = vi.fn();
    const frames: FrameRequestCallback[] = [];
    const anchor = createReaderResizeAnchor({
      canAnchor: () => true,
      getPageIndex: () => 7,
      restorePage,
      requestFrame: callback => frames.push(callback),
      cancelFrame: vi.fn(),
    });

    anchor.onResize();
    anchor.cancel();
    frames[0](0);
    vi.runAllTimers();

    expect(anchor.active).toBe(false);
    expect(restorePage).not.toHaveBeenCalled();
  });
});
