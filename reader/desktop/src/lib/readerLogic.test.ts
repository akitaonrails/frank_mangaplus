import { describe, it, expect } from 'vitest';
import {
  buildPageGroups,
  scanChapterBounds,
  chapterIdAfter,
  chapterIdBefore,
  chapterLockLabel,
  findGroupContainingPage,
  firstGroupOfChapter,
  imgLoadingMode,
  isUrlExpired,
  PRELOAD_AHEAD,
  PRELOAD_BEHIND,
  urlExpiresAt,
  URL_EXPIRY_MARGIN_SECS,
  isSubscriptionLockError,
  keyToReaderAction,
  PAGE_TYPE_LEFT,
  PAGE_TYPE_RIGHT,
  retryImageSrc,
  RETRY_PARAM,
  type LoadedPage,
} from './readerLogic';
import type { Chapter, MangaPage } from './types';

// Compact factory — every test uses the same shape so each case stays
// focused on the behaviour rather than the noise.
function page(chapterId: number, name = `Ch${chapterId}`, type = 0): LoadedPage {
  const mp: MangaPage = { imageUrl: '', width: 0, height: 0, type, encryptionKey: '' };
  return { mp, chapterId, chapterName: name };
}

// One split spread as manga_viewer_v3 delivers it with split=yes:
// the RIGHT half, then the LEFT half.
function spread(chapterId: number): LoadedPage[] {
  return [
    page(chapterId, `Ch${chapterId}`, PAGE_TYPE_RIGHT),
    page(chapterId, `Ch${chapterId}`, PAGE_TYPE_LEFT),
  ];
}

// Group shape as page types, so a spread reads as [2, 1].
function shape(groups: ReturnType<typeof buildPageGroups>): number[][] {
  return groups.map(g => g.pages.map(p => p.mp.type));
}

describe('buildPageGroups', () => {
  it('returns empty for empty input regardless of mode', () => {
    expect(buildPageGroups([], 'single')).toEqual([]);
    expect(buildPageGroups([], 'double')).toEqual([]);
  });

  it('single mode: every page is its own group', () => {
    const pages = [page(1), page(1), page(1)];
    const groups = buildPageGroups(pages, 'single');
    expect(groups).toHaveLength(3);
    expect(groups.map(g => g.firstPageIndex)).toEqual([0, 1, 2]);
    expect(groups.every(g => g.pages.length === 1)).toBe(true);
  });

  it('double mode: a single-page cover binds solo, then pages pair', () => {
    const odd = Array.from({ length: 5 }, () => page(1));
    expect(buildPageGroups(odd, 'double').map(g => g.pages.length)).toEqual([1, 2, 2]);
    const even = Array.from({ length: 6 }, () => page(1));
    const groups = buildPageGroups(even, 'double');
    expect(groups.map(g => g.pages.length)).toEqual([1, 2, 2, 1]); // solo trailing
    expect(groups.map(g => g.firstPageIndex)).toEqual([0, 1, 3, 5]);
  });

  it('double mode: the cover rule resets at each chapter boundary', () => {
    const pages = [page(1), page(1), page(1), page(2), page(2), page(2)];
    const groups = buildPageGroups(pages, 'double');
    // Chapter 1: [cover], [pair]; chapter 2: [cover], [pair]
    expect(groups.map(g => g.pages.map(p => p.chapterId))).toEqual([[1], [1, 1], [2], [2, 2]]);
  });

  it('double mode: a cover spread fills the first frame and pairs start from page 1', () => {
    const pages = [...spread(1), page(1), page(1), page(1)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[2, 1], [0, 0], [0]]);
  });

  it('double mode: a cover spread in a later chapter also pairs from its page 1', () => {
    const pages = [page(1), page(1), page(1), ...spread(2), page(2), page(2)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[0], [0, 0], [2, 1], [0, 0]]);
  });

  // Live-observed layout (first chapter of "Hey! Devil Girl!"): three
  // single pages, then a spread whose RIGHT half sits at index 3.
  it('double mode: live-observed chapter keeps its spread whole', () => {
    const pages = [page(1), page(1), page(1), ...spread(1), page(1), page(1)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[0], [0, 0], [2, 1], [0, 0]]);
    expect(groups.map(g => g.firstPageIndex)).toEqual([0, 1, 3, 5]);
  });

  // Live-observed layout (first chapter of "Home at the Horizon"): the
  // cover is a single page, but the first spread sits at index 2, so
  // print pairs from page 1.
  it('double mode: a first spread at an even offset starts pairs on page 1', () => {
    const pages = [page(1), page(1), ...spread(1), page(1), page(1)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[0, 0], [2, 1], [0, 0]]);
  });

  it('double mode: a chapter with no spread binds its cover solo', () => {
    const pages = [page(1), page(1), page(1)];
    expect(shape(buildPageGroups(pages, 'double'))).toEqual([[0], [0, 0]]);
  });

  it('double mode: the page before an off-parity spread goes solo', () => {
    // First spread at offset 1 (cover solo), second at offset 4.
    const pages = [page(1), ...spread(1), page(1), ...spread(1), page(1)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[0], [2, 1], [0], [2, 1], [0]]);
  });

  it('double mode: keeps every spread whole wherever it falls', () => {
    const pages = [page(1), ...spread(1), page(1), page(1), page(1), ...spread(1), ...spread(1)];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[0], [2, 1], [0, 0], [0], [2, 1], [2, 1]]);
  });

  it('double mode: treats unmatched halves as ordinary pages', () => {
    // LEFT before RIGHT is not a spread; a RIGHT half at a chapter end has no partner.
    const pages = [
      page(1, 'Ch1', PAGE_TYPE_LEFT),
      page(1, 'Ch1', PAGE_TYPE_RIGHT),
      page(1),
      page(1, 'Ch1', PAGE_TYPE_RIGHT),
      page(2, 'Ch2', PAGE_TYPE_LEFT),
    ];
    const groups = buildPageGroups(pages, 'double');
    expect(shape(groups)).toEqual([[1], [2, 0], [2], [1]]);
  });

  it('double mode: never pairs across a chapter boundary, spread halves included', () => {
    const pages = [page(1), page(1), page(1, 'Ch1', PAGE_TYPE_RIGHT), page(2, 'Ch2', PAGE_TYPE_LEFT), page(2), page(2)];
    const groups = buildPageGroups(pages, 'double');
    expect(groups.map(g => g.pages.map(p => p.chapterId))).toEqual([[1], [1, 1], [2], [2, 2]]);
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
    // groups: [0], [1-2], [3-4]
    expect(findGroupContainingPage(groups, 0)).toBe(0);
    expect(findGroupContainingPage(groups, 1)).toBe(1);
    expect(findGroupContainingPage(groups, 2)).toBe(1);
    expect(findGroupContainingPage(groups, 3)).toBe(2);
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
    // groups: [c100 p1], [c100 p2,p3], [c200 p1], [c200 p2], [c300 p1], [c300 p2,p3], [c300 p4]
    expect(firstGroupOfChapter(pages, groups, 100)).toBe(0);
    expect(firstGroupOfChapter(pages, groups, 200)).toBe(2); // each chapter opens on its solo cover
    expect(firstGroupOfChapter(pages, groups, 300)).toBe(4);
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

  it('maps R/r to reload-images', () => {
    expect(keyToReaderAction('r')).toBe('reload-images');
    expect(keyToReaderAction('R')).toBe('reload-images');
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

describe('imgLoadingMode', () => {
  it('eager-loads the window around the reading position', () => {
    expect(imgLoadingMode(20, 20)).toBe('eager'); // current page
    expect(imgLoadingMode(20 + PRELOAD_AHEAD, 20)).toBe('eager'); // window edge ahead
    expect(imgLoadingMode(20 - PRELOAD_BEHIND, 20)).toBe('eager'); // window edge behind
  });

  it('keeps far-away pages lazy', () => {
    expect(imgLoadingMode(20 + PRELOAD_AHEAD + 1, 20)).toBe('lazy');
    expect(imgLoadingMode(20 - PRELOAD_BEHIND - 1, 20)).toBe('lazy');
    expect(imgLoadingMode(60, 0)).toBe('lazy');
  });

  it('covers the start of a freshly opened chapter', () => {
    // On mount currentPageIndex is 0 — the first pages must all be
    // eager so the reader never opens onto a placeholder.
    for (let i = 0; i <= PRELOAD_AHEAD; i++) {
      expect(imgLoadingMode(i, 0)).toBe('eager');
    }
  });
});

describe('signed-URL expiry', () => {
  // Real shape from the live CDN.
  const URL =
    'https://jumpg-assets3.tokyo-cdn.com/secure/title/100004/chapter/1000193/manga_page/super_high/1.webp?hash=UoqyznSTlmS2gPm6M6AAIQ&expires=1788559200';

  it('extracts the expires param', () => {
    expect(urlExpiresAt(URL)).toBe(1788559200);
    // Works regardless of param order.
    expect(urlExpiresAt('https://x/y.webp?expires=42&hash=abc')).toBe(42);
  });

  it('returns null when there is no expires param', () => {
    expect(urlExpiresAt('https://x/y.webp?hash=abc')).toBe(null);
    expect(urlExpiresAt('https://x/y.webp')).toBe(null);
    // "expires" as a path fragment must not match.
    expect(urlExpiresAt('https://x/expires=9/y.webp')).toBe(null);
  });

  it('flags URLs past their expiry', () => {
    expect(isUrlExpired(URL, 1788559200 + 1)).toBe(true);
    expect(isUrlExpired(URL, 1788559200)).toBe(true);
  });

  it('flags URLs inside the refresh margin as expired', () => {
    // Refresh shortly BEFORE the deadline so an in-flight request
    // can't get refused at the edge.
    expect(isUrlExpired(URL, 1788559200 - URL_EXPIRY_MARGIN_SECS)).toBe(true);
    expect(isUrlExpired(URL, 1788559200 - URL_EXPIRY_MARGIN_SECS - 1)).toBe(false);
  });

  it('never expires URLs without an expires param', () => {
    expect(isUrlExpired('https://x/y.webp?hash=abc', Number.MAX_SAFE_INTEGER)).toBe(false);
  });
});

describe('chapterLockLabel', () => {
  it('labels the paid ChapterType values', () => {
    expect(chapterLockLabel(2)).toBe('MAX');         // STANDARD
    expect(chapterLockLabel(3)).toBe('MAX Deluxe');  // DELUXE
    expect(chapterLockLabel(4)).toBe('Locked');      // LOCKED_AFTER_FREE_READ
  });

  it('returns null for freely readable chapters', () => {
    expect(chapterLockLabel(0)).toBe(null);  // FREE
    expect(chapterLockLabel(1)).toBe(null);  // FREE_FOR_FIRST_TIME
    // Absent field (older cached payloads without chapterType).
    expect(chapterLockLabel(undefined)).toBe(null);
    // Unknown future enum values shouldn't fabricate a lock.
    expect(chapterLockLabel(99)).toBe(null);
  });
});

describe('isSubscriptionLockError', () => {
  it('matches the live-observed 11301 refusal in any wrapping', () => {
    // Exactly as the reader receives it after the ApiError Display pass.
    expect(isSubscriptionLockError(
      'API error: Invalid user: Invalid user access(11301) (action=0)'
    )).toBe(true);
    expect(isSubscriptionLockError('invalid user access(11301)')).toBe(true);
  });

  it('does not match other errors', () => {
    expect(isSubscriptionLockError('Timed out after 12000ms')).toBe(false);
    expect(isSubscriptionLockError('API error: maintenance (action=2)')).toBe(false);
    expect(isSubscriptionLockError('')).toBe(false);
    // A different numeric code must not be treated as the paywall.
    expect(isSubscriptionLockError('Invalid user access(11302)')).toBe(false);
  });
});

describe('retryImageSrc', () => {
  const signed = 'mpimg://cdn.example/secure/1.webp?hash=abc';

  it('leaves attempt 0 untouched', () => {
    expect(retryImageSrc(signed, 0)).toBe(signed);
    expect(retryImageSrc(signed, -1)).toBe(signed);
  });

  it('appends the retry param to an already-queried URL', () => {
    expect(retryImageSrc(signed, 2)).toBe(`${signed}&${RETRY_PARAM}=2`);
  });

  it('starts a query when the URL has none', () => {
    expect(retryImageSrc('mpimg://cdn.example/a.webp', 1)).toBe(
      `mpimg://cdn.example/a.webp?${RETRY_PARAM}=1`,
    );
  });

  it('uses a query param, never a fragment — a fragment never reaches the loader', () => {
    const out = retryImageSrc(signed, 3);
    expect(out).not.toContain('#');
    expect(out).toContain(`${RETRY_PARAM}=3`);
  });

  it('produces a distinct src per attempt so the resource identity changes', () => {
    const seen = new Set([1, 2, 3].map(n => retryImageSrc(signed, n)));
    expect(seen.size).toBe(3);
  });

  it('handles an empty base', () => {
    expect(retryImageSrc('', 2)).toBe('');
  });
});
