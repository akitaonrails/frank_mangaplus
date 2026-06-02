export const DEFAULT_IPC_TIMEOUT_MS = 12_000;

export function ipcTimeoutError(ms: number): Error {
  const seconds = Math.trunc(ms / 1000);
  const label = seconds > 0 && ms % 1000 === 0 ? `${seconds} s` : `${ms} ms`;
  return new Error(`Timed out after ${label} — API may be rate-limited or unreachable`);
}

/**
 * Race a Tauri IPC promise against a timeout without depending on Tauri.
 * Kept pure enough for Vitest coverage so pages don't each open-code their
 * own hung-IPC handling.
 */
export function withIpcTimeout<T>(promise: Promise<T>, ms = DEFAULT_IPC_TIMEOUT_MS): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(ipcTimeoutError(ms)), ms);
  });

  return Promise.race([
    promise,
    timeout,
  ]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}
