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
  nextPageMode,
  getLastReadPage,
  setLastReadPage,
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

  it('page mode defaults to single and round-trips all three states', () => {
    expect(getPageMode()).toBe('single');
    setPageMode('double');
    expect(getPageMode()).toBe('double');
    setPageMode('double-cover');
    expect(getPageMode()).toBe('double-cover');
    setPageMode('single');
    expect(getPageMode()).toBe('single');
  });

  it('page mode treats unknown values as single', () => {
    localStorage.setItem('mp:pageMode', 'triple');
    expect(getPageMode()).toBe('single');
  });

  it('nextPageMode cycles single → double → double-cover → single', () => {
    expect(nextPageMode('single')).toBe('double');
    expect(nextPageMode('double')).toBe('double-cover');
    expect(nextPageMode('double-cover')).toBe('single');
  });

  it('per-chapter last-read page round-trips', () => {
    expect(getLastReadPage(42)).toBe(null);
    setLastReadPage(42, 7);
    expect(getLastReadPage(42)).toBe(7);
    setLastReadPage(43, 1);
    expect(getLastReadPage(43)).toBe(1);
    expect(getLastReadPage(42)).toBe(7); // independent per chapter
  });

  it('per-chapter last-read page rejects non-positive values', () => {
    setLastReadPage(11, 5);
    setLastReadPage(11, 0);
    expect(getLastReadPage(11)).toBe(5); // 0 ignored
    setLastReadPage(11, -1);
    expect(getLastReadPage(11)).toBe(5); // -1 ignored
  });

  it('per-chapter last-read page returns null for garbage in storage', () => {
    localStorage.setItem('mp:chpage:99', 'not-a-number');
    expect(getLastReadPage(99)).toBe(null);
  });
});
