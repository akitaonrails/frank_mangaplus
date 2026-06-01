// Pure logic helpers for the search page. Extracted from
// `routes/search/+page.svelte` so each transformation is unit-testable
// without a SvelteKit harness or Tauri IPC mocks. Mirrors the pattern
// from `lib/readerLogic.ts`.
//
// Anything that touches the DOM, Svelte state runes, or the network
// stays in the page component. Anything that's "shape A → shape B" or
// "given (state, input) compute label" lives here.

import type { SearchView, Title } from '$lib/types';

/** State of the inline "+ Library" button, per title. Mirrors the
 *  shape used inside the search page; centralising it here keeps the
 *  label function and the component in lockstep. */
export type FavButtonState = 'pending' | 'error' | undefined;

/** Default visible row cap on the search grid. Re-exported as a
 *  constant so tests don't have to import the page module. */
export const DEFAULT_VISIBLE_CAP = 80;

/**
 * Flatten a curated /title_list/search SearchView into a deduplicated
 * Title list. The wire format groups titles inside `contents[].titleList.featuredTitles`,
 * and the same title can appear under multiple featured lists ("New
 * releases" + "Popular"). Dedupe by `titleId` and preserve
 * encounter order — the order matches the publisher's intended
 * presentation.
 */
export function flattenSearchView(view: SearchView | null | undefined): Title[] {
  if (!view) return [];
  const seen = new Set<number>();
  const out: Title[] = [];
  for (const content of view.contents ?? []) {
    for (const t of content.titleList?.featuredTitles ?? []) {
      if (seen.has(t.titleId)) continue;
      seen.add(t.titleId);
      out.push(t);
    }
  }
  return out;
}

/**
 * Substring match on title name OR author, case-insensitive. Empty
 * query short-circuits to the full input list — same shape, no
 * filtering — so callers can use this without branching on query
 * length themselves.
 */
export function filterTitles(titles: Title[], query: string): Title[] {
  const q = query.trim().toLowerCase();
  if (q === '') return titles;
  return titles.filter(
    t =>
      t.name.toLowerCase().includes(q) ||
      t.author.toLowerCase().includes(q),
  );
}

/**
 * Cap the visible-row count and report how many matches are hidden.
 * The grid renders {visible} only; the hidden count drives the
 * "Showing first N of M — refine your query" footer.
 */
export function paginate<T>(items: T[], cap: number = DEFAULT_VISIBLE_CAP) {
  const visible = items.slice(0, cap);
  const hiddenCount = Math.max(0, items.length - cap);
  return { visible, hiddenCount };
}

/**
 * Compute the label shown on the +/✓ Library button for one title.
 * Pure projection of (inLibrary, transient state) to user-visible
 * text — having this in one place stops the {#if} cascade in the
 * Svelte file from drifting between renders.
 */
export function computeButtonLabel(
  inLibrary: boolean,
  state: FavButtonState,
): string {
  if (inLibrary) return '✓ In Library';
  if (state === 'pending') return 'Adding…';
  if (state === 'error') return 'Retry';
  return '+ Library';
}

/**
 * Decide whether the +/✓ Library button should be disabled. Already-
 * in-library titles are not re-addable; in-flight clicks are blocked
 * so a double-tap can't fire two `add_favorite` IPCs.
 */
export function buttonDisabled(
  inLibrary: boolean,
  state: FavButtonState,
): boolean {
  return inLibrary || state === 'pending';
}
