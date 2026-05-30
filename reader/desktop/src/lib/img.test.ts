import { describe, it, expect } from 'vitest';
import { proxied } from './img';

describe('proxied', () => {
  it('rewrites https → mpimg', () => {
    expect(proxied('https://jumpg-assets.tokyo-cdn.com/foo/bar.png')).toBe(
      'mpimg://jumpg-assets.tokyo-cdn.com/foo/bar.png',
    );
  });

  it('rewrites https on the premium-CDN host too', () => {
    expect(proxied('https://jumpg-assets3.tokyo-cdn.com/secure/1.webp?sig=x')).toBe(
      'mpimg://jumpg-assets3.tokyo-cdn.com/secure/1.webp?sig=x',
    );
  });

  it('returns empty string for null/undefined/empty', () => {
    expect(proxied(null)).toBe('');
    expect(proxied(undefined)).toBe('');
    expect(proxied('')).toBe('');
  });

  it('only replaces the leading scheme, not occurrences in the path', () => {
    // path component contains "https:" — should not be touched.
    expect(proxied('https://h/redirect?to=https://other.example')).toBe(
      'mpimg://h/redirect?to=https://other.example',
    );
  });
});
