import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest';
import { clearMocks, mockConvertFileSrc } from '@tauri-apps/api/mocks';
import { proxied } from './img';

describe('proxied', () => {
  beforeEach(() => {
    vi.stubGlobal('window', globalThis);
  });

  afterEach(() => {
    clearMocks();
    vi.unstubAllGlobals();
  });

  it('uses Tauri\'s Windows-compatible custom-protocol URL', () => {
    vi.stubGlobal('isTauri', true);
    mockConvertFileSrc('windows');

    expect(proxied('https://jumpg-assets.tokyo-cdn.com/foo/bar.png')).toBe(
      'http://mpimg.localhost/https%3A%2F%2Fjumpg-assets.tokyo-cdn.com%2Ffoo%2Fbar.png',
    );
  });

  it('uses Tauri\'s native custom-protocol URL and preserves signed queries', () => {
    vi.stubGlobal('isTauri', true);
    mockConvertFileSrc('linux');

    expect(proxied('https://jumpg-assets3.tokyo-cdn.com/secure/1.webp?sig=x')).toBe(
      'mpimg://localhost/https%3A%2F%2Fjumpg-assets3.tokyo-cdn.com%2Fsecure%2F1.webp%3Fsig%3Dx',
    );
  });

  it('returns empty string for null/undefined/empty', () => {
    expect(proxied(null)).toBe('');
    expect(proxied(undefined)).toBe('');
    expect(proxied('')).toBe('');
  });

  it('leaves non-HTTPS sources untouched', () => {
    expect(proxied('data:image/png;base64,abc')).toBe('data:image/png;base64,abc');
  });

  it('leaves HTTPS sources untouched outside Tauri', () => {
    expect(proxied('https://example.invalid/cover.webp')).toBe(
      'https://example.invalid/cover.webp',
    );
  });
});
