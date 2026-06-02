import { describe, it, expect, vi, afterEach } from 'vitest';
import { ipcTimeoutError, withIpcTimeout } from './ipcTimeout';

afterEach(() => {
  vi.useRealTimers();
});

describe('ipcTimeoutError', () => {
  it('formats the existing 12 second timeout message', () => {
    expect(ipcTimeoutError(12_000).message).toBe(
      'Timed out after 12 s — API may be rate-limited or unreachable',
    );
  });
});

describe('withIpcTimeout', () => {
  it('resolves with the IPC result before the timeout', async () => {
    vi.useFakeTimers();
    const result = withIpcTimeout(Promise.resolve({ ok: true }), 50);

    await expect(result).resolves.toEqual({ ok: true });
  });

  it('rejects when the timeout wins', async () => {
    vi.useFakeTimers();
    const result = withIpcTimeout(new Promise(() => undefined), 50);
    const assertion = expect(result).rejects.toThrow('Timed out after 50 ms');

    await vi.advanceTimersByTimeAsync(50);

    await assertion;
  });
});
