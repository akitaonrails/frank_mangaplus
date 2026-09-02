import { describe, it, expect } from 'vitest';
import type { Chapter, TitleDetailView } from './types';
import {
  flattenChapters,
  buildChapterList,
  buildOffsets,
  findRowIndex,
  visibleRange,
  rowHeight,
  ROW_H_FULL,
  ROW_H_COMPACT,
  ROW_H_DIVIDER,
  type ChapterRow,
} from './chapterListLogic';

function ch(chapterId: number, name = `#${chapterId}`): Chapter {
  return {
    titleId: 1,
    chapterId,
    name,
    subTitle: '',
    thumbnailUrl: '',
    isUpdated: false,
  };
}

const CHAPTERS = [ch(1), ch(2), ch(3), ch(4)];

function chapterIds(rows: ChapterRow[]): number[] {
  return rows.filter(r => r.type === 'chapter').map(r => r.chapter.chapterId);
}

describe('flattenChapters', () => {
  it('prefers chapterListV2', () => {
    const d = {
      chapterListV2: [ch(7)],
      chapterListGroup: {
        chapterNumbers: '1-10',
        firstChapterList: [ch(1)],
        midChapterList: [],
        lastChapterList: [],
      },
    } as unknown as TitleDetailView;
    expect(flattenChapters(d)).toEqual({ chapters: [ch(7)] });
  });

  it('falls back to the grouped list and its chapterNumbers divider', () => {
    const d = {
      chapterListV2: [],
      chapterListGroup: {
        chapterNumbers: '1-10',
        firstChapterList: [ch(1)],
        midChapterList: [ch(2)],
        lastChapterList: [ch(3)],
      },
    } as unknown as TitleDetailView;
    const out = flattenChapters(d);
    expect(out.chapters.map(c => c.chapterId)).toEqual([1, 2, 3]);
    expect(out.leadingDivider).toBe('1-10');
  });

  it('returns an empty list when neither field is present', () => {
    expect(flattenChapters({ chapterListV2: [] } as unknown as TitleDetailView).chapters).toEqual([]);
  });
});

describe('buildChapterList visibility', () => {
  const readSet = new Set([1, 2]);

  it('"all" keeps every chapter at full height', () => {
    const b = buildChapterList(CHAPTERS, { sortDesc: false, readSet, visibility: 'all' });
    expect(chapterIds(b.rows)).toEqual([1, 2, 3, 4]);
    expect(b.rows.every(r => r.type === 'chapter' && !r.compact)).toBe(true);
    expect(b.hiddenCount).toBe(0);
    expect(b.unreadCount).toBe(2);
    expect(b.totalChapters).toBe(4);
  });

  it('"compact" keeps read chapters but marks them slim', () => {
    const b = buildChapterList(CHAPTERS, { sortDesc: false, readSet, visibility: 'compact' });
    expect(chapterIds(b.rows)).toEqual([1, 2, 3, 4]);
    const compacted = b.rows.flatMap(r =>
      r.type === 'chapter' && r.compact ? [r.chapter.chapterId] : [],
    );
    expect(compacted).toEqual([1, 2]);
    expect(b.hiddenCount).toBe(0);
  });

  it('"hidden" drops read chapters entirely', () => {
    const b = buildChapterList(CHAPTERS, { sortDesc: false, readSet, visibility: 'hidden' });
    expect(chapterIds(b.rows)).toEqual([3, 4]);
    expect(b.hiddenCount).toBe(2);
    expect(b.unreadCount).toBe(2);
    expect(b.totalChapters).toBe(4);
  });

  it('hides everything when all chapters are read', () => {
    const b = buildChapterList(CHAPTERS, {
      sortDesc: false,
      readSet: new Set([1, 2, 3, 4]),
      visibility: 'hidden',
    });
    expect(chapterIds(b.rows)).toEqual([]);
    expect(b.unreadCount).toBe(0);
    expect(b.hiddenCount).toBe(4);
  });

  it('reverses for descending sort without mutating the input', () => {
    const input = [...CHAPTERS];
    const b = buildChapterList(input, { sortDesc: true, readSet: new Set(), visibility: 'all' });
    expect(chapterIds(b.rows)).toEqual([4, 3, 2, 1]);
    expect(input.map(c => c.chapterId)).toEqual([1, 2, 3, 4]);
  });

  it('keeps the leading divider even when every chapter under it is hidden', () => {
    const b = buildChapterList(CHAPTERS, {
      sortDesc: false,
      readSet: new Set([1, 2, 3, 4]),
      visibility: 'hidden',
      leadingDivider: '1-4',
    });
    expect(b.rows).toEqual([{ type: 'divider', label: '1-4' }]);
  });
});

describe('row geometry', () => {
  it('sizes rows by kind', () => {
    expect(rowHeight({ type: 'divider', label: 'x' })).toBe(ROW_H_DIVIDER);
    expect(rowHeight({ type: 'chapter', chapter: ch(1), read: false, compact: false })).toBe(ROW_H_FULL);
    expect(rowHeight({ type: 'chapter', chapter: ch(1), read: true, compact: true })).toBe(ROW_H_COMPACT);
  });

  it('builds prefix sums ending at the total height', () => {
    const rows: ChapterRow[] = [
      { type: 'divider', label: 'x' },
      { type: 'chapter', chapter: ch(1), read: true, compact: true },
      { type: 'chapter', chapter: ch(2), read: false, compact: false },
    ];
    const offsets = buildOffsets(rows);
    expect(offsets).toEqual([
      0,
      ROW_H_DIVIDER,
      ROW_H_DIVIDER + ROW_H_COMPACT,
      ROW_H_DIVIDER + ROW_H_COMPACT + ROW_H_FULL,
    ]);
  });

  it('offsets an empty list to a zero-height table', () => {
    expect(buildOffsets([])).toEqual([0]);
    expect(visibleRange(buildOffsets([]), 0, 800)).toEqual({ start: 0, end: 0 });
  });
});

describe('findRowIndex / visibleRange', () => {
  // 10 full rows: boundaries at 0, 72, 144, …
  const rows: ChapterRow[] = Array.from({ length: 10 }, (_, i) => ({
    type: 'chapter' as const,
    chapter: ch(i + 1),
    read: false,
    compact: false,
  }));
  const offsets = buildOffsets(rows);

  it('finds the row containing a scroll position', () => {
    expect(findRowIndex(offsets, 0)).toBe(0);
    expect(findRowIndex(offsets, ROW_H_FULL - 1)).toBe(0);
    expect(findRowIndex(offsets, ROW_H_FULL)).toBe(1);
    expect(findRowIndex(offsets, ROW_H_FULL * 3 + 5)).toBe(3);
  });

  it('clamps negative and past-the-end positions', () => {
    expect(findRowIndex(offsets, -50)).toBe(0);
    expect(findRowIndex(offsets, 99999)).toBe(rows.length - 1);
  });

  it('covers the viewport plus overscan, clamped to the list', () => {
    const { start, end } = visibleRange(offsets, ROW_H_FULL * 4, ROW_H_FULL * 2, 1);
    expect(start).toBe(3);
    expect(end).toBe(8); // rows 4..6 visible, +1 overscan each side
  });

  it('never returns a window outside the row list', () => {
    const top = visibleRange(offsets, 0, ROW_H_FULL * 3, 10);
    expect(top.start).toBe(0);
    expect(top.end).toBe(rows.length);
    const bottom = visibleRange(offsets, ROW_H_FULL * 100, ROW_H_FULL * 3, 10);
    expect(bottom.end).toBe(rows.length);
    expect(bottom.start).toBeGreaterThanOrEqual(0);
  });

  it('handles mixed heights — a compact run is denser per pixel', () => {
    const mixed: ChapterRow[] = [
      { type: 'chapter', chapter: ch(1), read: true, compact: true },
      { type: 'chapter', chapter: ch(2), read: true, compact: true },
      { type: 'chapter', chapter: ch(3), read: false, compact: false },
    ];
    const off = buildOffsets(mixed);
    expect(findRowIndex(off, ROW_H_COMPACT)).toBe(1);
    expect(findRowIndex(off, ROW_H_COMPACT * 2)).toBe(2);
  });
});
