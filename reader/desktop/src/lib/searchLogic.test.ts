import { describe, it, expect } from 'vitest';
import {
  flattenSearchView,
  filterTitles,
  paginate,
  computeButtonLabel,
  buttonDisabled,
  clearFavoriteErrorState,
  DEFAULT_VISIBLE_CAP,
} from './searchLogic';
import type { SearchView, Title } from './types';

function title(id: number, name = `t${id}`, author = ''): Title {
  return {
    titleId: id,
    name,
    author,
    portraitImageUrl: '',
    language: 0,
  };
}

describe('flattenSearchView', () => {
  it('returns [] for null/undefined/empty', () => {
    expect(flattenSearchView(null)).toEqual([]);
    expect(flattenSearchView(undefined)).toEqual([]);
    expect(flattenSearchView({ contents: [] })).toEqual([]);
  });

  it('flattens nested contents into a single list', () => {
    const v: SearchView = {
      contents: [
        { titleList: { featuredTitles: [title(1), title(2)] } },
        { titleList: { featuredTitles: [title(3)] } },
      ],
    };
    const flat = flattenSearchView(v);
    expect(flat.map(t => t.titleId)).toEqual([1, 2, 3]);
  });

  it('dedupes by titleId across contents — same title under multiple featured lists keeps the first encountered', () => {
    const v: SearchView = {
      contents: [
        { titleList: { featuredTitles: [title(1, 'first')] } },
        { titleList: { featuredTitles: [title(1, 'second-occurrence')] } },
        { titleList: { featuredTitles: [title(2)] } },
      ],
    };
    const flat = flattenSearchView(v);
    expect(flat.map(t => t.titleId)).toEqual([1, 2]);
    expect(flat[0].name).toBe('first');
  });

  it('tolerates missing titleList', () => {
    const v: SearchView = { contents: [{}, { titleList: { featuredTitles: [title(7)] } }] } as SearchView;
    expect(flattenSearchView(v).map(t => t.titleId)).toEqual([7]);
  });
});

describe('filterTitles', () => {
  const titles = [
    title(1, 'Bleach', 'Tite Kubo'),
    title(2, 'One Piece', 'Eiichiro Oda'),
    title(3, 'Naruto', 'Masashi Kishimoto'),
  ];

  it('empty query returns the input list unchanged', () => {
    expect(filterTitles(titles, '')).toEqual(titles);
    expect(filterTitles(titles, '   ')).toEqual(titles); // whitespace-only too
  });

  it('matches case-insensitively against name', () => {
    expect(filterTitles(titles, 'bleach').map(t => t.titleId)).toEqual([1]);
    expect(filterTitles(titles, 'BLEACH').map(t => t.titleId)).toEqual([1]);
    expect(filterTitles(titles, 'Bleach').map(t => t.titleId)).toEqual([1]);
  });

  it('matches against author too', () => {
    expect(filterTitles(titles, 'oda').map(t => t.titleId)).toEqual([2]);
    expect(filterTitles(titles, 'kishimoto').map(t => t.titleId)).toEqual([3]);
  });

  it('substring match — partial word inside name', () => {
    expect(filterTitles(titles, 'piece').map(t => t.titleId)).toEqual([2]);
    expect(filterTitles(titles, 'ru').map(t => t.titleId).sort()).toEqual([3]);
  });

  it('no match returns []', () => {
    expect(filterTitles(titles, 'zzzzz')).toEqual([]);
  });
});

describe('paginate', () => {
  it('returns the input unchanged when below cap', () => {
    const items = [1, 2, 3];
    const { visible, hiddenCount } = paginate(items, 10);
    expect(visible).toEqual([1, 2, 3]);
    expect(hiddenCount).toBe(0);
  });

  it('truncates to cap and reports hidden count', () => {
    const items = Array.from({ length: 200 }, (_, i) => i);
    const { visible, hiddenCount } = paginate(items, 80);
    expect(visible).toHaveLength(80);
    expect(hiddenCount).toBe(120);
    expect(visible[0]).toBe(0);
    expect(visible[79]).toBe(79);
  });

  it('uses DEFAULT_VISIBLE_CAP when no cap supplied', () => {
    const items = Array.from({ length: DEFAULT_VISIBLE_CAP + 5 }, (_, i) => i);
    const { visible, hiddenCount } = paginate(items);
    expect(visible).toHaveLength(DEFAULT_VISIBLE_CAP);
    expect(hiddenCount).toBe(5);
  });

  it('hiddenCount cannot go negative even with a misconfigured cap', () => {
    const { hiddenCount } = paginate([1, 2], 100);
    expect(hiddenCount).toBe(0);
  });
});

describe('computeButtonLabel', () => {
  it('in-library wins regardless of transient state', () => {
    expect(computeButtonLabel(true, undefined)).toBe('✓ In Library');
    expect(computeButtonLabel(true, 'pending')).toBe('✓ In Library');
    expect(computeButtonLabel(true, 'error')).toBe('✓ In Library');
  });

  it('pending shows the spinner copy', () => {
    expect(computeButtonLabel(false, 'pending')).toBe('Adding…');
  });

  it('error shows retry copy', () => {
    expect(computeButtonLabel(false, 'error')).toBe('Retry');
  });

  it('idle default is the add affordance', () => {
    expect(computeButtonLabel(false, undefined)).toBe('+ Library');
  });
});

describe('buttonDisabled', () => {
  it('disabled when already in library', () => {
    expect(buttonDisabled(true, undefined)).toBe(true);
  });

  it('disabled while a click is in flight — prevents double-add', () => {
    expect(buttonDisabled(false, 'pending')).toBe(true);
  });

  it('error state stays enabled so the user can retry', () => {
    expect(buttonDisabled(false, 'error')).toBe(false);
  });

  it('idle is enabled', () => {
    expect(buttonDisabled(false, undefined)).toBe(false);
  });
});

describe('clearFavoriteErrorState', () => {
  it('clears an expired error state for the matching title only', () => {
    const states = new Map<number, 'pending' | 'error'>([
      [1, 'error'],
      [2, 'pending'],
    ]);

    const next = clearFavoriteErrorState(states, 1);

    expect(next.has(1)).toBe(false);
    expect(next.get(2)).toBe('pending');
    expect(states.get(1)).toBe('error');
  });

  it('does not erase a newer pending retry when an older error timer fires', () => {
    const states = new Map<number, 'pending' | 'error'>([[7, 'pending']]);

    const next = clearFavoriteErrorState(states, 7);

    expect(next).toBe(states);
    expect(next.get(7)).toBe('pending');
  });
});
