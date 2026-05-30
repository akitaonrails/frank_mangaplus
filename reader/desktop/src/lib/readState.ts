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
