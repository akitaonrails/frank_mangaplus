import { describe, it, expect, beforeEach } from 'vitest';
import {
  getReadChapters,
  isRead,
  markChapterRead,
  getLastReadChapter,
  getSortDescending,
  setSortDescending,
  getPageMode,
  setPageMode,
} from './readState';

// Minimal in-memory localStorage shim — vitest's jsdom-less default doesn't
// supply one. We swap it in per-test so reads/writes go nowhere persistent.
function installLocalStorageStub() {
  const store = new Map<string, string>();
  const stub: Storage = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    removeItem: (k: string) => void store.delete(k),
    setItem: (k: string, v: string) => void store.set(k, v),
  };
  // @ts-expect-error window may be undefined in pure-node test env
  globalThis.localStorage = stub;
}

beforeEach(() => {
  installLocalStorageStub();
});

describe('readState', () => {
  it('round-trips read chapters per title', () => {
    expect(getReadChapters(42).size).toBe(0);

    markChapterRead(42, 1001);
    markChapterRead(42, 1002);
    markChapterRead(43, 9999); // different title

    const t42 = getReadChapters(42);
    expect(t42.has(1001)).toBe(true);
    expect(t42.has(1002)).toBe(true);
    expect(t42.has(9999)).toBe(false);
    expect(t42.size).toBe(2);

    expect(getReadChapters(43).has(9999)).toBe(true);
  });

  it('isRead matches getReadChapters', () => {
    markChapterRead(7, 100);
    expect(isRead(7, 100)).toBe(true);
    expect(isRead(7, 101)).toBe(false);
  });

  it('returns an empty set when storage holds garbage', () => {
    localStorage.setItem('mp:read:5', '{not-json}');
    expect(getReadChapters(5).size).toBe(0);
  });

  it('getLastReadChapter returns the most-recently-marked chapter', () => {
    markChapterRead(11, 200);
    markChapterRead(11, 201);
    expect(getLastReadChapter(11)).toBe(201);
  });

  it('getLastReadChapter returns null when nothing recorded', () => {
    expect(getLastReadChapter(404)).toBe(null);
  });

  it('sort preference defaults to descending and round-trips', () => {
    expect(getSortDescending()).toBe(true);
    setSortDescending(false);
    expect(getSortDescending()).toBe(false);
    setSortDescending(true);
    expect(getSortDescending()).toBe(true);
  });

  it('page mode defaults to single and round-trips', () => {
    expect(getPageMode()).toBe('single');
    setPageMode('double');
    expect(getPageMode()).toBe('double');
    setPageMode('single');
    expect(getPageMode()).toBe('single');
  });

  it('page mode treats unknown values as single', () => {
    localStorage.setItem('mp:pageMode', 'triple');
    expect(getPageMode()).toBe('single');
  });
});
