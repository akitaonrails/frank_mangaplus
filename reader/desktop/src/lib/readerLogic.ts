// Pure logic extracted from the reader page so it can be unit-tested
// without spinning up Svelte. Everything here is referentially
// transparent — no $state, no DOM, no localStorage, no global IO.
//
// The reader page composes these with its reactive state ($derived /
// $effect) and event handlers; the .svelte file should remain a thin
// wiring layer over what's here.

import type { Chapter, MangaPage } from './types';
import type { PageMode } from './readState';

/** One page in the flat per-chapter scroll stack the reader maintains. */
export type LoadedPage = {
  mp: MangaPage;
  chapterId: number;
  chapterName: string;
};

/**
 * A rendered frame: one page (single mode), one or two pages from the
 * same chapter (double / double-cover modes). `firstPageIndex` is the
 * offset into the flat `LoadedPage[]` of the leftmost (in reading order)
 * page in the group — used by the page indicator and resume logic.
 */
export type PageGroup = {
  pages: LoadedPage[];
  firstPageIndex: number;
};

/**
 * Group pages into frames according to the layout mode. Pairs never
 * cross chapter boundaries — if a chapter has an odd page count, its
 * trailing page is solo, and the next chapter starts a fresh group.
 *
 *   single        → [p1] [p2] [p3] [p4]
 *   double        → [p1, p2] [p3, p4]
 *   double-cover  → [p1] [p2, p3] [p4, p5] (cover binds singly)
 *
 * In double-cover mode the "cover" resets at every chapter boundary —
 * matches how printed manga rebinds covers per volume.
 */
export function buildPageGroups(pages: LoadedPage[], mode: PageMode): PageGroup[] {
  if (mode === 'single' || pages.length === 0) {
    return pages.map((p, i) => ({ pages: [p], firstPageIndex: i }));
  }
  const groups: PageGroup[] = [];
  let i = 0;
  let coverOffsetActive = mode === 'double-cover';
  let currentChapter = pages[0].chapterId;
  while (i < pages.length) {
    const a = pages[i];
    if (a.chapterId !== currentChapter) {
      currentChapter = a.chapterId;
      if (mode === 'double-cover') coverOffsetActive = true;
    }
    if (coverOffsetActive) {
      groups.push({ pages: [a], firstPageIndex: i });
      coverOffsetActive = false;
      i += 1;
      continue;
    }
    const b = pages[i + 1];
    if (b && b.chapterId === a.chapterId) {
      groups.push({ pages: [a, b], firstPageIndex: i });
      i += 2;
    } else {
      groups.push({ pages: [a], firstPageIndex: i });
      i += 1;
    }
  }
  return groups;
}

/**
 * Scan the contiguous run of pages around `currentIndex` that share the
 * same chapterId. Returns the first index and the page count for that
 * chapter, as it currently exists in `pages`.
 *
 * Pages of a given chapter are always contiguous because the reader
 * appends whole chapters at the end of `loadedPages`. This scan is
 * preferred over `findIndex(p.chapterId === X)` because it doesn't
 * depend on a separately-derived "visible chapter id" that can briefly
 * drift out of sync with the array during reactive updates — Kaiju
 * No. 8's ex → #077 transition showed exactly that bug in v0.7.2.
 */
export function scanChapterBounds(
  pages: LoadedPage[],
  currentIndex: number,
): { firstIndex: number; count: number } {
  const here = pages[currentIndex];
  if (!here) return { firstIndex: 0, count: pages.length };
  const chId = here.chapterId;
  let first = currentIndex;
  while (first > 0 && pages[first - 1]?.chapterId === chId) first--;
  let count = 0;
  for (let i = first; i < pages.length && pages[i].chapterId === chId; i++) {
    count++;
  }
  return { firstIndex: first, count };
}

/**
 * Return the chapterId immediately after `currentId` in the publication
 * order list, or null if currentId isn't found or is already the last.
 *
 * The list is whatever `title_detail.chapter_list_v2` returned — its
 * natural order IS publication order. We never sort by chapterId
 * because that breaks for series like Kaiju No. 8 where IDs aren't
 * monotonic with chapter numbers (e.g., #125 has a lower id than #124
 * because the API reassigned ids during a re-upload).
 */
export function chapterIdAfter(chapters: Chapter[], currentId: number): number | null {
  const i = chapters.findIndex(c => c.chapterId === currentId);
  if (i < 0 || i === chapters.length - 1) return null;
  return chapters[i + 1].chapterId;
}

/** Mirror of `chapterIdAfter` for backward navigation. */
export function chapterIdBefore(chapters: Chapter[], currentId: number): number | null {
  const i = chapters.findIndex(c => c.chapterId === currentId);
  if (i <= 0) return null;
  return chapters[i - 1].chapterId;
}

/** Find the index of the group that contains the page at `pageIndex`,
 *  or -1 if no group covers it. Used by:
 *   - resume scroll (after appendChapter), finds where the saved
 *     last-read page lives in the just-rendered grid
 *   - pageMode toggle, finds where the user was before regrouping so
 *     the toggle doesn't jump them around */
export function findGroupContainingPage(groups: PageGroup[], pageIndex: number): number {
  for (let i = 0; i < groups.length; i++) {
    const g = groups[i];
    if (g.firstPageIndex <= pageIndex && pageIndex < g.firstPageIndex + g.pages.length) {
      return i;
    }
  }
  return -1;
}

/** Find the group that contains the first page of `chapterId`, or -1
 *  if no page in `loadedPages` belongs to that chapter. Used by the
 *  chapter-boundary advance path: after a prefetch appends a new
 *  chapter, scrolling to "currentGroup + 1" is unreliable because
 *  currentGroup might have drifted during the await. Looking up the
 *  new chapter's first group by id is stable. */
export function firstGroupOfChapter(
  pages: LoadedPage[],
  groups: PageGroup[],
  chapterId: number,
): number {
  const firstPageIdx = pages.findIndex(p => p.chapterId === chapterId);
  if (firstPageIdx < 0) return -1;
  return findGroupContainingPage(groups, firstPageIdx);
}

/**
 * Reader actions are the only side-effect operations that can be
 * triggered by a key press. Centralising the keymap here means the
 * onKey handler is a one-liner ("look up + call") and the bindings
 * themselves are testable.
 *
 *   advanceForwardScroll  — vertical scroll keys (Space, ArrowDown, j, PageDown)
 *   advanceBackScroll     — vertical scroll keys reversed (ArrowUp, k, PageUp)
 *   advanceForwardFlip    — manga-RTL keys (ArrowLeft) — page-flip animation
 *   advanceBackFlip       — manga-RTL keys (ArrowRight)
 *   jumpChapterStart      — Home, jumps to the first page of the current chapter
 *   jumpChapterEnd        — End, jumps to the last page of the current chapter
 *   togglePageMode        — D
 *   toggleEyeFilter       — F
 *   goBack                — Escape
 */
export type ReaderAction =
  | 'advance-forward-scroll'
  | 'advance-back-scroll'
  | 'advance-forward-flip'
  | 'advance-back-flip'
  | 'jump-chapter-start'
  | 'jump-chapter-end'
  | 'toggle-page-mode'
  | 'toggle-eye-filter'
  | 'reload-images'
  | 'open-help'
  | 'go-back';

const KEY_MAP: Record<string, ReaderAction> = {
  ArrowDown:  'advance-forward-scroll',
  j:          'advance-forward-scroll',
  ' ':        'advance-forward-scroll',
  PageDown:   'advance-forward-scroll',
  ArrowUp:    'advance-back-scroll',
  k:          'advance-back-scroll',
  PageUp:     'advance-back-scroll',
  ArrowLeft:  'advance-forward-flip',
  ArrowRight: 'advance-back-flip',
  Home:       'jump-chapter-start',
  End:        'jump-chapter-end',
  d:          'toggle-page-mode',
  D:          'toggle-page-mode',
  f:          'toggle-eye-filter',
  F:          'toggle-eye-filter',
  r:          'reload-images',
  R:          'reload-images',
  '?':        'open-help',
  Escape:     'go-back',
};

/** Resolve a KeyboardEvent.key to a reader action, or null if the key
 *  isn't bound (caller should NOT preventDefault in that case). */
export function keyToReaderAction(key: string): ReaderAction | null {
  return KEY_MAP[key] ?? null;
}

/** Query parameter used to bust a failed image load. Stripped by the
 *  `mpimg://` scheme handler in `src-tauri/src/lib.rs` before the URL is
 *  forwarded to the CDN, so the signed query the CDN checks is
 *  unchanged. Keep the two in sync. */
export const RETRY_PARAM = 'mpretry';

/**
 * Build the `<img src>` for a page, given how many reload attempts it
 * has taken. Attempt 0 is the untouched proxied URL.
 *
 * This MUST be a query parameter, not a fragment. A fragment is never
 * sent to a URL loader, so the Rust scheme handler cannot see it, and
 * WebKit drops the fragment when computing its memory-cache key — so a
 * fragment-only change resolves to the same already-failed resource and
 * the load short-circuits instead of re-requesting. A query parameter
 * changes the resource identity the loader and cache actually key on.
 *
 * Note that a changed src alone is still not enough to recover a broken
 * `<img>`: WebKit also remembers the failed load on the element itself.
 * The reader keys its `{#each}` on the attempt count so the element is
 * recreated, which is what leaving the chapter and coming back does.
 */
export function retryImageSrc(base: string, attempt: number): string {
  if (!base || attempt <= 0) return base;
  const sep = base.includes('?') ? '&' : '?';
  return `${base}${sep}${RETRY_PARAM}=${attempt}`;
}

/** Query parameter carrying the signed URL's hard expiry, in unix
 *  seconds. MANGA Plus CDN URLs are signed *and* time-limited — once
 *  `expires` passes, the CDN answers 410 Gone and no amount of retrying
 *  the same URL will ever succeed. Fresh URLs come only from another
 *  `get_chapter_pages` call. */
export const EXPIRES_PARAM = 'expires';

/**
 * Read the expiry (unix seconds) out of a signed CDN URL, or null when
 * the URL carries no usable `expires`. Parsed by hand rather than with
 * `URL` so this stays pure and works on the `mpimg://` form too.
 */
export function urlExpiresAt(url: string): number | null {
  const q = url.indexOf('?');
  if (q < 0) return null;
  for (const part of url.slice(q + 1).split('&')) {
    const eq = part.indexOf('=');
    if (eq < 0) continue;
    if (part.slice(0, eq) !== EXPIRES_PARAM) continue;
    const n = Number(part.slice(eq + 1));
    return Number.isFinite(n) && n > 0 ? n : null;
  }
  return null;
}

/**
 * True when a signed CDN URL is past its expiry, or close enough that a
 * fetch started now would likely lose the race.
 *
 * `skewSeconds` covers both clock drift against the CDN and the gap
 * between deciding to load and the request actually landing. A URL with
 * no `expires` is never considered expired — we can't know, and treating
 * it as dead would break any unsigned/cached URL.
 */
export function isUrlExpired(url: string, nowMs = Date.now(), skewSeconds = 60): boolean {
  const exp = urlExpiresAt(url);
  if (exp == null) return false;
  return exp - skewSeconds <= Math.floor(nowMs / 1000);
}

/**
 * Chapter ids whose loaded page URLs have expired, in first-seen order.
 *
 * Used on resume: after the laptop wakes, every URL the reader is
 * holding may be dead at once, and refreshing per-chapter is far cheaper
 * than letting each `<img>` discover its own 410.
 */
export function expiredChapterIds(
  pages: LoadedPage[],
  nowMs = Date.now(),
  skewSeconds = 60,
): number[] {
  const out: number[] = [];
  const seen = new Set<number>();
  for (const p of pages) {
    if (seen.has(p.chapterId)) continue;
    if (isUrlExpired(p.mp.imageUrl, nowMs, skewSeconds)) {
      seen.add(p.chapterId);
      out.push(p.chapterId);
    }
  }
  return out;
}
