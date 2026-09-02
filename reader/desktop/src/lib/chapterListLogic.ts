// Pure logic for the title page's chapter list. Extracted from
// `routes/title/[id]/+page.svelte` so row building, read-filtering and
// the variable-height virtualization math are unit-testable without a
// Svelte/Tauri harness. Mirrors the pattern from `lib/readerLogic.ts`:
// no $state, no DOM, no localStorage in here.

import type { Chapter, TitleDetailView } from './types';
import type { ReadVisibility } from './readState';

/** One rendered row in the virtual chapter list. */
export type ChapterRow =
  | { type: 'chapter'; chapter: Chapter; read: boolean; compact: boolean }
  | { type: 'divider'; label: string };

/** Row heights, in px. The virtualizer needs these up front — rows are
 *  absolutely positioned, so declared height must match rendered height
 *  (see the matching CSS in the title page). */
export const ROW_H_FULL = 72;
export const ROW_H_COMPACT = 34;
export const ROW_H_DIVIDER = 72;

export function rowHeight(row: ChapterRow): number {
  if (row.type === 'divider') return ROW_H_DIVIDER;
  return row.compact ? ROW_H_COMPACT : ROW_H_FULL;
}

/**
 * Flatten a TitleDetailView into the server's ascending chapter order.
 * `chapterListV2` is the modern field; older responses only carry
 * `chapterListGroup`, whose `chapterNumbers` string becomes a leading
 * divider label.
 */
export function flattenChapters(d: TitleDetailView): {
  chapters: Chapter[];
  leadingDivider?: string;
} {
  if (d.chapterListV2 && d.chapterListV2.length > 0) {
    return { chapters: [...d.chapterListV2] };
  }
  const grp = d.chapterListGroup;
  if (!grp) return { chapters: [] };
  return {
    chapters: [...grp.firstChapterList, ...grp.midChapterList, ...grp.lastChapterList],
    leadingDivider: grp.chapterNumbers || undefined,
  };
}

export type ChapterListOptions = {
  /** Newest-first when true (the server sends oldest-first). */
  sortDesc: boolean;
  readSet: ReadonlySet<number>;
  /** How already-read chapters are presented. */
  visibility: ReadVisibility;
  leadingDivider?: string;
};

export type BuiltChapterList = {
  rows: ChapterRow[];
  /** Prefix sums of row heights; length is rows.length + 1, so
   *  `offsets[i]` is the top of row i and the last entry is the total
   *  scroll height. */
  offsets: number[];
  totalChapters: number;
  unreadCount: number;
  /** Chapters omitted from `rows` because they're read and the
   *  visibility mode is 'hidden'. */
  hiddenCount: number;
};

/**
 * Build the flat row list plus its virtualization offsets.
 *
 *   'all'     — every chapter at full height (the original behaviour)
 *   'compact' — read chapters collapse to a slim one-line row
 *   'hidden'  — read chapters are dropped entirely; only unread remain
 *
 * The leading divider survives filtering: it labels the whole list, not
 * a chapter, so it's kept even when every chapter under it is hidden.
 */
export function buildChapterList(
  chapters: Chapter[],
  opts: ChapterListOptions,
): BuiltChapterList {
  const ordered = opts.sortDesc ? [...chapters].reverse() : [...chapters];

  const rows: ChapterRow[] = [];
  if (opts.leadingDivider) rows.push({ type: 'divider', label: opts.leadingDivider });

  let unreadCount = 0;
  let hiddenCount = 0;
  for (const chapter of ordered) {
    const read = opts.readSet.has(chapter.chapterId);
    if (!read) unreadCount++;
    if (read && opts.visibility === 'hidden') {
      hiddenCount++;
      continue;
    }
    rows.push({ type: 'chapter', chapter, read, compact: read && opts.visibility === 'compact' });
  }

  const offsets = buildOffsets(rows);
  return { rows, offsets, totalChapters: ordered.length, unreadCount, hiddenCount };
}

/** Prefix sums of row heights. `offsets[rows.length]` is the total height. */
export function buildOffsets(rows: ChapterRow[]): number[] {
  const offsets = new Array<number>(rows.length + 1);
  offsets[0] = 0;
  for (let i = 0; i < rows.length; i++) offsets[i + 1] = offsets[i] + rowHeight(rows[i]);
  return offsets;
}

/** Index of the row containing `y` — the largest i with offsets[i] <= y. */
export function findRowIndex(offsets: number[], y: number): number {
  const n = offsets.length - 1;
  if (n <= 0) return 0;
  const target = Math.max(0, y);
  let lo = 0;
  let hi = n - 1;
  let ans = 0;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid] <= target) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

/**
 * Half-open [start, end) row window covering the viewport, padded by
 * `overscan` rows on each side. Rows have variable heights, so this
 * binary-searches the offset table instead of dividing by a constant.
 */
export function visibleRange(
  offsets: number[],
  scrollTop: number,
  viewportHeight: number,
  overscan = 10,
): { start: number; end: number } {
  const n = offsets.length - 1;
  if (n <= 0) return { start: 0, end: 0 };
  const first = findRowIndex(offsets, scrollTop);
  const last = findRowIndex(offsets, scrollTop + Math.max(0, viewportHeight));
  return {
    start: Math.max(0, first - overscan),
    end: Math.min(n, last + 1 + overscan),
  };
}
