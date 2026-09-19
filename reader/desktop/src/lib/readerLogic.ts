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
 * same chapter (double mode). `firstPageIndex` is the
 * offset into the flat `LoadedPage[]` of the leftmost (in reading order)
 * page in the group — used by the page indicator and resume logic.
 */
export type PageGroup = {
  pages: LoadedPage[];
  firstPageIndex: number;
};

/** MangaPage.type values (PageOuterClass.java, v2.3.0): 0 SINGLE,
 *  1 LEFT, 2 RIGHT, 3 DOUBLE. manga_viewer_v3 is requested with
 *  split=yes, which delivers every spread as a RIGHT half followed by
 *  its LEFT half (CDN files `Nr.webp` then `Nl.webp`); DOUBLE only
 *  appears with split=no. Verified against live responses. */
export const PAGE_TYPE_LEFT = 1;
export const PAGE_TYPE_RIGHT = 2;

/** True when pages[i] and pages[i + 1] are the two halves of one split
 *  spread in the same chapter. */
function startsSpread(pages: LoadedPage[], i: number): boolean {
  const a = pages[i];
  const b = pages[i + 1];
  return (
    a !== undefined &&
    b !== undefined &&
    a.chapterId === b.chapterId &&
    a.mp.type === PAGE_TYPE_RIGHT &&
    b.mp.type === PAGE_TYPE_LEFT
  );
}

/**
 * Whether the chapter starting at `start` opens with a solo cover in
 * double mode. Print keeps every spread on facing pages, so the
 * chapter's first split spread fixes where its pairs start: a RIGHT
 * half at an even offset (a cover spread included) means pairs start
 * on page 1, an odd offset means the cover binds solo. A chapter with
 * no spread binds its cover solo, as printed volumes do.
 */
function coverBindsSolo(pages: LoadedPage[], start: number): boolean {
  const chapterId = pages[start].chapterId;
  for (let i = start; i < pages.length && pages[i].chapterId === chapterId; i++) {
    if (startsSpread(pages, i)) return (i - start) % 2 === 1;
  }
  return true;
}

/**
 * Group pages into frames according to the layout mode. Pairs never
 * cross chapter boundaries — if a chapter has an odd page count, its
 * trailing page is solo, and the next chapter starts a fresh group.
 *
 *   single → [p1] [p2] [p3] [p4]
 *   double → [p1] [p2, p3] [p4r, p5l]   first spread at an odd offset
 *            [p1, p2] [p3r, p4l]        first spread at an even offset
 *            [p1r, p2l] [p3, p4]        cover is a split spread
 *
 * In double mode each chapter decides on its own where pairs start
 * (see coverBindsSolo), and the decision resets at every chapter
 * boundary.
 *
 * The two halves of a split spread (RIGHT then LEFT, see
 * PAGE_TYPE_RIGHT) always share a frame: should a later spread fall
 * off the chapter's parity, the page that would otherwise pair with
 * its RIGHT half renders solo instead, so no spread is ever cut across
 * two frames.
 */
export function buildPageGroups(pages: LoadedPage[], mode: PageMode): PageGroup[] {
  if (mode === 'single' || pages.length === 0) {
    return pages.map((p, i) => ({ pages: [p], firstPageIndex: i }));
  }
  const groups: PageGroup[] = [];
  let i = 0;
  let atChapterStart = true;
  let currentChapter = pages[0].chapterId;
  while (i < pages.length) {
    const a = pages[i];
    if (a.chapterId !== currentChapter) {
      currentChapter = a.chapterId;
      atChapterStart = true;
    }
    if (startsSpread(pages, i)) {
      groups.push({ pages: [a, pages[i + 1]], firstPageIndex: i });
      atChapterStart = false;
      i += 2;
      continue;
    }
    if (atChapterStart) {
      atChapterStart = false;
      if (coverBindsSolo(pages, i)) {
        groups.push({ pages: [a], firstPageIndex: i });
        i += 1;
        continue;
      }
    }
    const b = pages[i + 1];
    if (b && b.chapterId === a.chapterId && !startsSpread(pages, i + 1)) {
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

// ---------- image preloading ----------

// Sliding eager-load window around the reading position. Browser-native
// lazy loading only starts a fetch when the image is nearly on screen —
// with full-viewport page frames and flip/jump navigation that meant
// the reader routinely landed on a placeholder and had to wait. Pages
// inside the window get loading="eager" so they fetch immediately;
// far-away pages stay lazy so opening a chapter doesn't pull the whole
// title's worth of super_high images at once. Flipping an <img> from
// lazy to eager mid-life triggers its load per the HTML spec, so the
// window follows the reader as currentPageIndex advances.
export const PRELOAD_AHEAD = 10;
export const PRELOAD_BEHIND = 3;

export function imgLoadingMode(
  pageIndex: number,
  currentPageIndex: number,
): 'eager' | 'lazy' {
  return pageIndex >= currentPageIndex - PRELOAD_BEHIND &&
    pageIndex <= currentPageIndex + PRELOAD_AHEAD
    ? 'eager'
    : 'lazy';
}

// ---------- signed-URL expiry ----------

// Page image URLs are signed: `...webp?hash=...&expires=<unix seconds>`,
// valid for roughly 1-2 hours after the manga_viewer_v3 call that
// minted them. A reader left open (or a machine that slept) comes back
// with URLs the CDN now refuses — and the plus_vw_token cookie premium
// fetches need has gone equally stale. Retrying such a URL can never
// succeed; the chapter has to be re-fetched to mint fresh signatures
// (which also refreshes the cookie). These helpers let the reader tell
// a transient blip (retry the same URL) from an expired signature
// (refresh the chapter).

export function urlExpiresAt(url: string): number | null {
  const m = /[?&]expires=(\d+)/.exec(url);
  if (!m) return null;
  const n = Number.parseInt(m[1], 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

// Refresh slightly before the deadline so an image that starts loading
// near the edge doesn't get its request refused mid-flight.
export const URL_EXPIRY_MARGIN_SECS = 300;

/** True when the URL's signature is past (or within the margin of) its
 *  expiry. URLs without an expires param never count as expired. */
export function isUrlExpired(url: string, nowSecs: number): boolean {
  const exp = urlExpiresAt(url);
  return exp != null && nowSecs >= exp - URL_EXPIRY_MARGIN_SECS;
}

// ---------- retry cache-busting ----------

/** Query parameter used to bust a failed image load. Stripped by the
 *  `mpimg://` scheme handler in `src-tauri/src/lib.rs` before the URL is
 *  forwarded to the CDN, so the signed query the CDN checks is
 *  unchanged. Keep the two in sync. */
export const RETRY_PARAM = 'mpretry';

/**
 * Build the `<img src>` for a page, given how many reload attempts it
 * has taken. Attempt 0 is the untouched proxied URL.
 *
 * This MUST be a query parameter, not a fragment. Per the HTML spec, an
 * <img> whose src changes only in the fragment is not re-fetched (the
 * fragment is excluded from the resource identity), and a fragment never
 * reaches the URL loader, so the Rust scheme handler cannot see it
 * either — a fragment-only change resolves to the same already-failed
 * resource and the load short-circuits. A query parameter changes the
 * identity the loader and cache actually key on.
 *
 * A changed src alone is still not enough to recover a broken `<img>`:
 * WebKit also records the failed load on the element itself. The reader
 * keys its `{#each}` on the attempt count so the element is recreated —
 * which is what leaving the chapter and coming back does.
 */
export function retryImageSrc(base: string, attempt: number): string {
  if (!base || attempt <= 0) return base;
  const sep = base.includes('?') ? '&' : '?';
  return `${base}${sep}${RETRY_PARAM}=${attempt}`;
}

// ---------- subscription-locked chapters ----------

/** Chapter.chapterType → the label of the paywall badge, or null for
 *  freely readable chapters. Enum values from ChapterOuterClass.java
 *  (v2.3.0): 0 FREE, 1 FREE_FOR_FIRST_TIME, 2 STANDARD, 3 DELUXE,
 *  4 LOCKED_AFTER_FREE_READ. The badge names the MANGA Plus MAX tier
 *  that unlocks the chapter — whether the *user's* plan covers it is
 *  only known server-side, so this is informational, not a hard gate. */
export function chapterLockLabel(chapterType: number | undefined): string | null {
  switch (chapterType) {
    case 2: return 'MAX';
    case 3: return 'MAX Deluxe';
    case 4: return 'Locked';
    default: return null;
  }
}

/** True when an API error message is the server refusing a
 *  subscription-locked chapter to an account whose plan doesn't cover
 *  it. Live-observed as english_popup "Invalid user: Invalid user
 *  access(11301)" (e.g. free/basic plan opening a DELUXE Bleach
 *  chapter). The reader shows a friendly paywall explanation for this
 *  instead of the raw error + useless Retry. */
export function isSubscriptionLockError(message: string): boolean {
  return /invalid user access\(11301\)/i.test(message);
}
