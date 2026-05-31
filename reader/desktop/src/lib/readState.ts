// Persistent read-state for chapters, backed by localStorage.
//
// Storage shape:
//   mp:read:<titleId>    → JSON array of read chapterIds (deduped)
//   mp:last:<titleId>    → JSON { chapterId, t } for "continue reading"
//
// All ops are sync and safe to call from Svelte effects; localStorage is
// available in the Tauri WebView (it's webkit / wry).

const KEY_READ = (titleId: number) => `mp:read:${titleId}`;
const KEY_LAST = (titleId: number) => `mp:last:${titleId}`;

export function getReadChapters(titleId: number): Set<number> {
  try {
    const raw = localStorage.getItem(KEY_READ(titleId));
    if (!raw) return new Set();
    const arr = JSON.parse(raw) as number[];
    return new Set(arr);
  } catch {
    return new Set();
  }
}

export function isRead(titleId: number, chapterId: number): boolean {
  return getReadChapters(titleId).has(chapterId);
}

export function markChapterRead(titleId: number, chapterId: number) {
  try {
    const set = getReadChapters(titleId);
    set.add(chapterId);
    localStorage.setItem(KEY_READ(titleId), JSON.stringify([...set]));
    localStorage.setItem(
      KEY_LAST(titleId),
      JSON.stringify({ chapterId, t: Date.now() }),
    );
  } catch (e) {
    console.warn('markChapterRead failed', e);
  }
}

// Per-chapter page-position memory. When the user leaves a chapter
// mid-read and comes back, the reader scrolls to this page rather than
// starting from page 1. 1-indexed (matches the visible page indicator).
const KEY_CHAPTER_PAGE = (chapterId: number) => `mp:chpage:${chapterId}`;
export function getLastReadPage(chapterId: number): number | null {
  try {
    const raw = localStorage.getItem(KEY_CHAPTER_PAGE(chapterId));
    if (!raw) return null;
    const n = parseInt(raw, 10);
    return Number.isFinite(n) && n >= 1 ? n : null;
  } catch {
    return null;
  }
}
export function setLastReadPage(chapterId: number, page: number) {
  try {
    if (!Number.isFinite(page) || page < 1) return;
    localStorage.setItem(KEY_CHAPTER_PAGE(chapterId), String(page));
  } catch (e) {
    console.warn('setLastReadPage failed', e);
  }
}

export function getLastReadChapter(titleId: number): number | null {
  try {
    const raw = localStorage.getItem(KEY_LAST(titleId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { chapterId?: number };
    return parsed.chapterId ?? null;
  } catch {
    return null;
  }
}

// Sort-order preference for the chapter list (per-title is overkill; one
// global flag is fine for v1). Default: descending = true.
const KEY_SORT_DESC = 'mp:sortDesc';
export function getSortDescending(): boolean {
  const v = localStorage.getItem(KEY_SORT_DESC);
  // default true (newest first)
  return v == null ? true : v === '1';
}
export function setSortDescending(v: boolean) {
  localStorage.setItem(KEY_SORT_DESC, v ? '1' : '0');
}

// Reader page layout.
//   - "single"       — one page per frame, the default
//   - "double"       — sequential pairs starting from page 1: [1,2],[3,4],…
//   - "double-cover" — first page solo, then pairs: [1],[2,3],[4,5],…
//                      (matches printed manga where the cover is a single
//                       page and the binding starts on the next spread)
export type PageMode = 'single' | 'double' | 'double-cover';
const KEY_PAGE_MODE = 'mp:pageMode';
export function getPageMode(): PageMode {
  const v = localStorage.getItem(KEY_PAGE_MODE);
  if (v === 'double') return 'double';
  if (v === 'double-cover') return 'double-cover';
  return 'single';
}
export function setPageMode(mode: PageMode) {
  localStorage.setItem(KEY_PAGE_MODE, mode);
}
// Cycle order driven by the D key / toggle button: single → double →
// double-cover → single.
export function nextPageMode(mode: PageMode): PageMode {
  return mode === 'single' ? 'double' : mode === 'double' ? 'double-cover' : 'single';
}

// Reading-comfort filter applied to all manga pages. Warms the whites
// toward sepia so a bright-white background isn't harsh on the eyes
// at night, without flattening contrast (sepia preserves luminance
// range, just shifts hue).
//   - off:  no filter
//   - low:  subtle warm
//   - med:  noticeable amber
//   - high: heavy sepia, late-night
export type EyeFilter = 'off' | 'low' | 'med' | 'high';
const KEY_EYE_FILTER = 'mp:eyeFilter';
const EYE_FILTER_VALUES: EyeFilter[] = ['off', 'low', 'med', 'high'];
export function getEyeFilter(): EyeFilter {
  const v = localStorage.getItem(KEY_EYE_FILTER);
  return (EYE_FILTER_VALUES as string[]).includes(v ?? '') ? (v as EyeFilter) : 'off';
}
export function setEyeFilter(level: EyeFilter) {
  localStorage.setItem(KEY_EYE_FILTER, level);
}
// Cycle order driven by the F key / toggle button: off → low → med →
// high → off.
export function nextEyeFilter(level: EyeFilter): EyeFilter {
  const i = EYE_FILTER_VALUES.indexOf(level);
  return EYE_FILTER_VALUES[(i + 1) % EYE_FILTER_VALUES.length];
}

// Tracks whether the user has already seen the reader help modal once.
// The first time they open a chapter, the modal auto-opens to surface
// the keybindings + click-zone explanation; after that it only opens
// on explicit "?" key or button click.
const KEY_HELP_SEEN = 'mp:helpSeen';
export function getHelpSeen(): boolean {
  return localStorage.getItem(KEY_HELP_SEEN) === '1';
}
export function setHelpSeen(seen: boolean) {
  localStorage.setItem(KEY_HELP_SEEN, seen ? '1' : '0');
}
