import { describe, it, expect } from 'vitest';
import {
  buildPageGroups,
  scanChapterBounds,
  chapterIdAfter,
  chapterIdBefore,
  findGroupContainingPage,
  firstGroupOfChapter,
  keyToReaderAction,
  type LoadedPage,
} from './readerLogic';
import type { Chapter, MangaPage } from './types';

// Compact factory — every test uses the same shape so each case stays
// focused on the behaviour rather than the noise.
function page(chapterId: number, name = `Ch${chapterId}`): LoadedPage {
  return {
    mp: {} as MangaPage,
    chapterId,
    chapterName: name,
  };
}

describe('buildPageGroups', () => {
  it('returns empty for empty input regardless of mode', () => {
    expect(buildPageGroups([], 'single')).toEqual([]);
    expect(buildPageGroups([], 'double')).toEqual([]);
    expect(buildPageGroups([], 'double-cover')).toEqual([]);
  });

  it('single mode: every page is its own group', () => {
    const pages = [page(1), page(1), page(1)];
    const groups = buildPageGroups(pages, 'single');
    expect(groups).toHaveLength(3);
    expect(groups.map(g => g.firstPageIndex)).toEqual([0, 1, 2]);
    expect(groups.every(g => g.pages.length === 1)).toBe(true);
  });

  it('double mode: even page count pairs perfectly', () => {
    const pages = Array.from({ length: 6 }, () => page(1));
    const groups = buildPageGroups(pages, 'double');
    expect(groups.map(g => g.firstPageIndex)).toEqual([0, 2, 4]);
    expect(groups.every(g => g.pages.length === 2)).toBe(true);
  });

  it('double mode: odd page count leaves a solo at the end', () => {
    const pages = Array.from({ length: 5 }, () => page(1));
    const groups = buildPageGroups(pages, 'double');
    expect(groups).toHaveLength(3);
    expect(groups[0].pages.length).toBe(2);
    expect(groups[1].pages.length).toBe(2);
    expect(groups[2].pages.length).toBe(1); // solo trailing
    expect(groups[2].firstPageIndex).toBe(4);
  });

  it('double mode: pairs never cross chapter boundaries', () => {
    // 3 pages of chapter 1, then 3 pages of chapter 2
    const pages = [page(1), page(1), page(1), page(2), page(2), page(2)];
    const groups = buildPageGroups(pages, 'double');
    // Expected: [c1p1,c1p2], [c1p3 solo], [c2p1,c2p2], [c2p3 solo]
    expect(groups).toHaveLength(4);
    expect(groups[0].pages.map(p => p.chapterId)).toEqual([1, 1]);
    expect(groups[1].pages.map(p => p.chapterId)).toEqual([1]); // odd leftover from chapter 1
    expect(groups[2].pages.map(p => p.chapterId)).toEqual([2, 2]);
    expect(groups[3].pages.map(p => p.chapterId)).toEqual([2]); // odd leftover from chapter 2
  });

  it('double-cover mode: first page solo, then pairs', () => {
    const pages = Array.from({ length: 5 }, () => page(1));
    const groups = buildPageGroups(pages, 'double-cover');
    expect(groups).toHaveLength(3);
    expect(groups[0].pages.length).toBe(1); // cover solo
    expect(groups[1].pages.length).toBe(2);
    expect(groups[2].pages.length).toBe(2);
  });

  it('double-cover mode: cover offset resets at each chapter boundary', () => {
    const pages = [page(1), page(1), page(1), page(2), page(2), page(2)];
    const groups = buildPageGroups(pages, 'double-cover');
    // Chapter 1: [solo], [pair]    — 3 pages
    // Chapter 2: [solo], [pair]    — 3 pages
    expect(groups).toHaveLength(4);
    expect(groups[0].pages.length).toBe(1); // c1 cover
    expect(groups[1].pages.length).toBe(2); // c1 rest
    expect(groups[2].pages.length).toBe(1); // c2 cover  ← reset
    expect(groups[3].pages.length).toBe(2); // c2 rest
  });
});

describe('scanChapterBounds', () => {
  const pages = [
    page(10), page(10), page(10), page(10),       // chapter 10: indices 0-3
    page(20), page(20),                            // chapter 20: indices 4-5
    page(30), page(30), page(30),                  // chapter 30: indices 6-8
  ];

  it('returns the bounds of the chapter containing the index', () => {
    expect(scanChapterBounds(pages, 0)).toEqual({ firstIndex: 0, count: 4 });
    expect(scanChapterBounds(pages, 2)).toEqual({ firstIndex: 0, count: 4 });
    expect(scanChapterBounds(pages, 4)).toEqual({ firstIndex: 4, count: 2 });
    expect(scanChapterBounds(pages, 5)).toEqual({ firstIndex: 4, count: 2 });
    expect(scanChapterBounds(pages, 6)).toEqual({ firstIndex: 6, count: 3 });
    expect(scanChapterBounds(pages, 8)).toEqual({ firstIndex: 6, count: 3 });
  });

  it('correctly bounds the chapter ex → #077 transition (regression for Kaiju "11 / 19" bug)', () => {
    // Simulating Kaiju "ex" (10 pages) followed by "#077" (19 pages)
    const exPages = Array.from({ length: 10 }, () => page(1015153, 'ex'));
    const ch77Pages = Array.from({ length: 19 }, () => page(1015155, '#077'));
    const all = [...exPages, ...ch77Pages];

    // On #077 page 1 (index 10), bounds should be { firstIndex: 10, count: 19 }
    // — NOT { firstIndex: 0 } which was the bug in v0.7.2 with findIndex.
    expect(scanChapterBounds(all, 10)).toEqual({ firstIndex: 10, count: 19 });
    // Local page number = currentIndex - firstIndex + 1 = 10 - 10 + 1 = 1 (not 11).
  });

  it('out-of-range index falls back to (0, total)', () => {
    expect(scanChapterBounds(pages, -1)).toEqual({ firstIndex: 0, count: pages.length });
    expect(scanChapterBounds(pages, 999)).toEqual({ firstIndex: 0, count: pages.length });
    expect(scanChapterBounds([], 0)).toEqual({ firstIndex: 0, count: 0 });
  });
});

describe('chapterIdAfter / chapterIdBefore', () => {
  function ch(id: number, name = `c${id}`): Chapter {
    return {
      titleId: 1,
      chapterId: id,
      name,
      subTitle: '',
      thumbnailUrl: '',
      isUpdated: false,
    };
  }

  // Publication order is the array order — IDs are intentionally NOT
  // monotonic to simulate the Kaiju 124/125/ex case.
  const list = [ch(100), ch(120), ch(115), ch(125), ch(130)];

  it('returns the next entry in publication order, regardless of id', () => {
    expect(chapterIdAfter(list, 100)).toBe(120);
    expect(chapterIdAfter(list, 120)).toBe(115); // not sorted by id!
    expect(chapterIdAfter(list, 115)).toBe(125);
    expect(chapterIdAfter(list, 125)).toBe(130);
  });

  it('returns null at the end of the list or for unknown ids', () => {
    expect(chapterIdAfter(list, 130)).toBe(null);
    expect(chapterIdAfter(list, 999)).toBe(null);
    expect(chapterIdAfter([], 1)).toBe(null);
  });

  it('chapterIdBefore is the mirror', () => {
    expect(chapterIdBefore(list, 120)).toBe(100);
    expect(chapterIdBefore(list, 115)).toBe(120);
    expect(chapterIdBefore(list, 125)).toBe(115);
    expect(chapterIdBefore(list, 100)).toBe(null); // already first
    expect(chapterIdBefore(list, 999)).toBe(null); // unknown
    expect(chapterIdBefore([], 1)).toBe(null);
  });
});

describe('findGroupContainingPage', () => {
  it('returns the group index whose range covers pageIndex', () => {
    const pages = Array.from({ length: 5 }, () => page(1));
    const groups = buildPageGroups(pages, 'double');
    // groups: [0-1], [2-3], [4]
    expect(findGroupContainingPage(groups, 0)).toBe(0);
    expect(findGroupContainingPage(groups, 1)).toBe(0);
    expect(findGroupContainingPage(groups, 2)).toBe(1);
    expect(findGroupContainingPage(groups, 3)).toBe(1);
    expect(findGroupContainingPage(groups, 4)).toBe(2);
  });

  it('returns -1 when the page is out of range', () => {
    const groups = buildPageGroups([page(1), page(1)], 'single');
    expect(findGroupContainingPage(groups, 5)).toBe(-1);
    expect(findGroupContainingPage([], 0)).toBe(-1);
  });
});

describe('firstGroupOfChapter', () => {
  // Simulating: chapter 100 (3 pages), then chapter 200 (2 pages),
  // then chapter 300 (4 pages).
  const pages = [
    page(100), page(100), page(100),
    page(200), page(200),
    page(300), page(300), page(300), page(300),
  ];

  it('finds the group containing each chapter\'s first page in single mode', () => {
    const groups = buildPageGroups(pages, 'single');
    expect(firstGroupOfChapter(pages, groups, 100)).toBe(0); // page idx 0
    expect(firstGroupOfChapter(pages, groups, 200)).toBe(3); // page idx 3
    expect(firstGroupOfChapter(pages, groups, 300)).toBe(5); // page idx 5
  });

  it('finds the right group in double mode (pairs never cross chapter boundary)', () => {
    const groups = buildPageGroups(pages, 'double');
    // groups: [c100 p1,p2], [c100 p3 solo], [c200 p1,p2], [c300 p1,p2], [c300 p3,p4]
    expect(firstGroupOfChapter(pages, groups, 100)).toBe(0);
    expect(firstGroupOfChapter(pages, groups, 200)).toBe(2); // after the c100 leftover solo
    expect(firstGroupOfChapter(pages, groups, 300)).toBe(3);
  });

  it('returns -1 when the chapter id has no pages loaded', () => {
    const groups = buildPageGroups(pages, 'single');
    expect(firstGroupOfChapter(pages, groups, 999)).toBe(-1);
    expect(firstGroupOfChapter([], [], 100)).toBe(-1);
  });

  it('regression: advance() at boundary uses firstGroupOfChapter for the post-prefetch jump', () => {
    // Scenario from the user report: chapter N (108 pages) reaches end,
    // chapter N+1 (43 pages) is appended. The user lands on page 1 of
    // N+1 — group index = N's group count, not "stale currentGroup + 1".
    const pagesN  = Array.from({ length: 108 }, () => page(1));
    const pagesN1 = Array.from({ length:  43 }, () => page(2));
    const all = [...pagesN, ...pagesN1];
    const groups = buildPageGroups(all, 'single');
    // In single mode, every page is its own group, so chapter N+1's
    // first group is at index 108 (right after N's 108 groups).
    expect(firstGroupOfChapter(all, groups, 2)).toBe(108);
  });
});

describe('keyToReaderAction', () => {
  it('maps the vertical-scroll keys to scroll actions', () => {
    expect(keyToReaderAction('ArrowDown')).toBe('advance-forward-scroll');
    expect(keyToReaderAction('j')).toBe('advance-forward-scroll');
    expect(keyToReaderAction(' ')).toBe('advance-forward-scroll');
    expect(keyToReaderAction('PageDown')).toBe('advance-forward-scroll');
    expect(keyToReaderAction('ArrowUp')).toBe('advance-back-scroll');
    expect(keyToReaderAction('k')).toBe('advance-back-scroll');
    expect(keyToReaderAction('PageUp')).toBe('advance-back-scroll');
  });

  it('maps the horizontal manga-RTL keys to flip actions', () => {
    expect(keyToReaderAction('ArrowLeft')).toBe('advance-forward-flip');
    expect(keyToReaderAction('ArrowRight')).toBe('advance-back-flip');
  });

  it('maps Home/End to chapter-jump actions', () => {
    expect(keyToReaderAction('Home')).toBe('jump-chapter-start');
    expect(keyToReaderAction('End')).toBe('jump-chapter-end');
  });

  it('maps "?" to the help modal', () => {
    expect(keyToReaderAction('?')).toBe('open-help');
  });

  it('maps the toggles + escape', () => {
    expect(keyToReaderAction('d')).toBe('toggle-page-mode');
    expect(keyToReaderAction('D')).toBe('toggle-page-mode');
    expect(keyToReaderAction('f')).toBe('toggle-eye-filter');
    expect(keyToReaderAction('F')).toBe('toggle-eye-filter');
    expect(keyToReaderAction('Escape')).toBe('go-back');
  });

  it('returns null for unmapped keys', () => {
    expect(keyToReaderAction('a')).toBe(null);
    expect(keyToReaderAction('Enter')).toBe(null);
    expect(keyToReaderAction('Tab')).toBe(null);
    // Case sensitivity: only 'd'/'D' are mapped, not other casings.
    expect(keyToReaderAction('e')).toBe(null);
  });
});
