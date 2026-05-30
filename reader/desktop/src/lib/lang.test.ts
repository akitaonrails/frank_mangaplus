import { describe, it, expect } from 'vitest';
import { langCode, DEFAULT_LANG, DEFAULT_CLANG, DEFAULT_COUNTRY } from './lang';

describe('langCode', () => {
  it('maps the known enum values', () => {
    expect(langCode(0)).toBe('eng');
    expect(langCode(1)).toBe('esp');
    expect(langCode(2)).toBe('fra');
    expect(langCode(3)).toBe('por');
    expect(langCode(4)).toBe('rus');
    expect(langCode(5)).toBe('ind');
  });

  it('falls back to the default for unknown values', () => {
    expect(langCode(999)).toBe('eng');
    expect(langCode(-1)).toBe('eng');
  });
});

describe('defaults', () => {
  it('exposes the locale defaults the desktop ships with', () => {
    expect(DEFAULT_LANG).toBe('eng');
    expect(DEFAULT_CLANG).toBe('eng');
    expect(DEFAULT_COUNTRY).toBe('US');
  });
});
