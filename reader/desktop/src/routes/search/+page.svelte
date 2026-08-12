<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import type {
    SearchView,
    Title,
    AllTitlesPayload,
    AllTitlesRefreshedEvent,
  } from '$lib/types';
  import { getFavorites, addFavorite as ipcAddFavorite } from '$lib/ipcCommands';
  import TitleCard from '$lib/TitleCard.svelte';
  import {
    contentLanguagesInNavbarOrder,
    DEFAULT_LANG,
    filterByContentLanguages,
    isContentLanguage,
    mergeTitles,
    titleHref,
    type ContentLanguage,
  } from '$lib/lang';
  import { contentLanguages } from '$lib/contentLanguagePreference';
  import {
    flattenSearchView,
    filterTitles,
    paginate,
    computeButtonLabel,
    buttonDisabled,
    clearFavoriteErrorState,
    mergePendingCatalogSnapshots,
    DEFAULT_VISIBLE_CAP,
  } from '$lib/searchLogic';
  import { withIpcTimeout } from '$lib/ipcTimeout';

  // Two catalogs, served in tiers:
  //   - curated: ~hundreds of titles from /title_list/search. Fast,
  //     fits in memory trivially, and is what we show on the empty
  //     landing view.
  //   - full: ~thousands of titles from /title_list/all_v3. Lazily
  //     fetched the first time the user types so the landing view
  //     stays light. Once loaded, all subsequent filtering uses it.
  let curated: Title[] = $state([]);
  let full: Title[] = $state([]);
  let fullByLanguage: Map<ContentLanguage, Title[]> = $state(new Map());
  // $state() infers the literal type ('idle') from the initializer unless
  // we widen it explicitly via the generic. Without the generic, later
  // assignments to 'loading'/'ready' fail svelte-check with "types
  // '\"idle\"' and '\"ready\"' have no overlap".
  let fullStatus = $state<'idle' | 'loading' | 'ready'>('idle');
  // Surfaces the SWR source in the result-count row ('fresh', 'stale',
  // 'network' from the Rust side) so users can tell whether they're
  // looking at a warm cache hit or a just-fetched response.
  let catalogSource = $state('');
  let loading = $state(true);
  let error = $state('');
  let query = $state('');

  // Library state for the +/✓ button. Loaded once on mount, then kept
  // in sync optimistically per add (so the user gets instant feedback
  // even while the IPC is in flight).
  let libraryIds = $state<Set<number>>(new Set());
  // Per-title button state: 'pending' while the IPC is in flight,
  // 'error' for ~2s on failure. Idle is the absence of an entry.
  let buttonState = $state<Map<number, 'pending' | 'error'>>(new Map());

  // Catalog selection + filtering + pagination are pure functions
  // (see lib/searchLogic.ts) — keeping them out of the component
  // lets the search-page diff stay focused on state + IPC.
  let activeCatalog = $derived(fullStatus === 'ready' ? full : curated);
  let filtered = $derived(filterTitles(activeCatalog, query));
  let pagination = $derived(paginate(filtered, DEFAULT_VISIBLE_CAP));
  let visible = $derived(pagination.visible);
  let hiddenCount = $derived(pagination.hiddenCount);

  let unlisten: UnlistenFn | null = null;
  let alive = true;
  let mounted = $state(false);
  let localeLoadSeq = 0;
  const favoriteResetTimers = new Map<number, ReturnType<typeof setTimeout>>();

  function combineSelected(
    byLanguage: Map<ContentLanguage, Title[]>,
    selected: readonly ContentLanguage[],
  ): Title[] {
    return mergeTitles(
      contentLanguagesInNavbarOrder(selected).map(code => byLanguage.get(code) ?? []),
    );
  }

  onMount(() => {
    // Register the SWR refresh listener before any catalog fetch can
    // trigger a background refresh. `listen()` itself is async, so guard
    // the late resolution path to avoid leaking after navigation away.
    void listen<AllTitlesRefreshedEvent>(
      'all_titles_refreshed',
      ev => {
        if (
          fullStatus === 'idle' ||
          ev.payload.lang !== DEFAULT_LANG ||
          !isContentLanguage(ev.payload.clang) ||
          !$contentLanguages.includes(ev.payload.clang)
        ) {
          return;
        }
        const code = ev.payload.clang;
        const titles = filterByContentLanguages(ev.payload.titles ?? [], [code]);
        fullByLanguage = new Map(fullByLanguage).set(code, titles);
        full = combineSelected(fullByLanguage, $contentLanguages);
        fullStatus = 'ready';
        catalogSource = 'fresh (refreshed)';
        console.log(`[search] ${code} catalog refreshed: ${ev.payload.titleCount} titles`);
      },
    ).then(fn => {
      if (alive) unlisten = fn;
      else fn();
    }).catch(e => {
      console.warn('[search] all_titles_refreshed listener failed:', e);
    });

    mounted = true;
  });

  $effect(() => {
    const selected = [...$contentLanguages];
    if (!mounted) return;
    void loadCuratedAndLibrary(selected);
  });

  async function loadCuratedAndLibrary(selected: ContentLanguage[] = [...$contentLanguages]) {
    const seq = ++localeLoadSeq;
    loading = true;
    error = '';
    curated = [];
    full = [];
    fullByLanguage = new Map();
    fullStatus = 'idle';
    catalogSource = '';
    // Parallel fetches: curated catalog (fast) + library set (for
    // already-in-library indicators). Either can fail independently
    // without blocking the other.
    const curatedP = Promise.allSettled(selected.map(async code => {
      const view = await withIpcTimeout(invoke<SearchView>('search', {
        lang: DEFAULT_LANG,
        clang: code,
      }));
      return [code, filterByContentLanguages(flattenSearchView(view), [code])] as const;
    }));
    // Attach both handlers immediately so a locale switch cannot leave a
    // rejected, no-longer-awaited favorites request behind.
    const libP = withIpcTimeout(getFavorites()).then(
      view => ({ view, error: null }),
      error => ({ view: null, error }),
    );

    try {
      const results = await curatedP;
      if (!alive || seq !== localeLoadSeq) return;
      const successful = results
        .filter((result): result is PromiseFulfilledResult<readonly [ContentLanguage, Title[]]> => result.status === 'fulfilled')
        .map(result => result.value);
      for (const result of results) {
        if (result.status === 'rejected') {
          console.warn('[search] curated language fetch failed:', result.reason);
        }
      }
      if (successful.length === 0) {
        const firstError = results.find(result => result.status === 'rejected');
        throw firstError && firstError.status === 'rejected'
          ? firstError.reason
          : new Error('No content language catalog could be loaded.');
      }
      const byLanguage = new Map(successful);
      curated = combineSelected(byLanguage, selected);
    } catch (e) {
      if (!alive || seq !== localeLoadSeq) return;
      error = e instanceof Error ? e.message : String(e);
    } finally {
      if (alive && seq === localeLoadSeq) loading = false;
    }

    const libraryResult = await libP;
    if (!alive || seq !== localeLoadSeq) return;
    if (libraryResult.view) {
      libraryIds = new Set((libraryResult.view.titles ?? []).map(t => t.titleId));
    } else {
      console.warn(
        '[search] library fetch failed (button state will degrade):',
        libraryResult.error,
      );
    }

    if (alive && seq === localeLoadSeq && query.trim().length > 0) {
      void loadFullCatalogIfNeeded(selected, seq);
    }
  }

  onDestroy(() => {
    alive = false;
    unlisten?.();
    for (const timer of favoriteResetTimers.values()) clearTimeout(timer);
    favoriteResetTimers.clear();
  });

  /** Lazy-fetch the full catalog. Called the first time the user
   *  types; subsequent calls are no-ops because fullStatus !== 'idle'.
   *  The Rust side handles SWR cache reads + the two-bucket merge —
   *  this returns immediately if there's a warm cache. */
  async function loadFullCatalogIfNeeded(
    selected: ContentLanguage[] = [...$contentLanguages],
    seq = localeLoadSeq,
  ) {
    if (fullStatus !== 'idle') return;
    fullStatus = 'loading';
    try {
      const results = await Promise.allSettled(selected.map(async code => {
        const payload = await withIpcTimeout(invoke<AllTitlesPayload>('get_all_titles_cached', {
          lang: DEFAULT_LANG,
          clang: code,
        }));
        const titles = filterByContentLanguages(payload.titles ?? [], [code]);
        return [code, titles, payload.source] as const;
      }));
      if (!alive || seq !== localeLoadSeq) return;
      const successful = results
        .filter((result): result is PromiseFulfilledResult<readonly [ContentLanguage, Title[], AllTitlesPayload['source']]> => result.status === 'fulfilled')
        .map(result => result.value);
      for (const result of results) {
        if (result.status === 'rejected') {
          console.warn('[search] full language catalog fetch failed:', result.reason);
        }
      }
      if (successful.length === 0) {
        throw new Error('No full content language catalog could be loaded.');
      }
      // While Promise.allSettled was waiting for every language, an SWR
      // refresh event may already have replaced one language with fresher
      // titles. Preserve those current entries instead of overwriting them
      // with the stale snapshots that originally triggered the refresh.
      const refreshedCodes = new Set(fullByLanguage.keys());
      fullByLanguage = mergePendingCatalogSnapshots(
        fullByLanguage,
        successful.map(([code, titles]) => [code, titles] as const),
      );
      full = combineSelected(fullByLanguage, selected);
      fullStatus = 'ready';
      const sources = new Set<string>();
      if (refreshedCodes.size > 0) sources.add('fresh (refreshed)');
      for (const [code, , source] of successful) {
        if (!refreshedCodes.has(code)) sources.add(source);
      }
      catalogSource = [...sources].join(' + ');
      console.log(
        `[search] full catalog loaded (${full.length} titles, source=${catalogSource})`,
      );
    } catch (e) {
      // Network failed AND no cache exists → fall back to curated.
      // We don't surface this as a page-level error because filtering
      // still works against the curated set; the user just sees a
      // smaller match space.
      console.warn('[search] full catalog fetch failed; staying on curated:', e);
      if (alive && seq === localeLoadSeq) fullStatus = 'idle';
    }
  }

  function onQueryInput() {
    if (query.trim().length > 0) {
      void loadFullCatalogIfNeeded();
    }
  }

  async function addFavorite(title: Title) {
    if (libraryIds.has(title.titleId)) return;       // already there
    if (buttonState.get(title.titleId) === 'pending') return; // in flight
    buttonState = new Map(buttonState).set(title.titleId, 'pending');
    try {
      await withIpcTimeout(ipcAddFavorite(title.titleId));
      // Optimistic library add — the next /favorites poll would
      // confirm it; meanwhile the button flips to ✓ In Library.
      libraryIds = new Set(libraryIds).add(title.titleId);
      const next = new Map(buttonState);
      next.delete(title.titleId);
      buttonState = next;
    } catch (e) {
      console.warn(`[search] add_favorite ${title.titleId} failed:`, e);
      buttonState = new Map(buttonState).set(title.titleId, 'error');
      const oldTimer = favoriteResetTimers.get(title.titleId);
      if (oldTimer) clearTimeout(oldTimer);
      const timer = setTimeout(() => {
        favoriteResetTimers.delete(title.titleId);
        buttonState = clearFavoriteErrorState(buttonState, title.titleId);
      }, 2000);
      favoriteResetTimers.set(title.titleId, timer);
    }
  }
</script>

<svelte:head>
  <title>Search — FRANK MANGA+</title>
</svelte:head>

<div class="search-page">
  <div class="search-bar-wrap">
    <input
      class="search-input"
      type="search"
      placeholder="Search titles or authors…"
      bind:value={query}
      oninput={onQueryInput}
    />
    {#if !loading}
      <span class="result-count">
        {filtered.length} title{filtered.length !== 1 ? 's' : ''}
        {#if fullStatus === 'loading'}
          <span class="catalog-hint">(loading full catalog…)</span>
        {:else if fullStatus === 'ready'}
          <span class="catalog-hint">· full catalog{catalogSource ? ` (${catalogSource})` : ''}</span>
        {:else}
          <span class="catalog-hint">· curated</span>
        {/if}
      </span>
    {/if}
  </div>

  {#if loading}
    <div class="spinner"></div>
  {:else if error}
    <div class="empty-state">
      <p>Error: {error}</p>
      <p><button class="retry-btn" onclick={() => void loadCuratedAndLibrary()}>↻ Retry</button></p>
    </div>
  {:else if filtered.length === 0}
    <div class="empty-state">
      <p>No titles match "{query}"{fullStatus === 'ready' ? ' in the full catalog.' : '.'}</p>
    </div>
  {:else}
    <div class="title-grid">
      {#each visible as title (title.titleId)}
        <TitleCard {title} href={titleHref(title)}>
          {#snippet action()}
            {@const inLibrary = libraryIds.has(title.titleId)}
            {@const st = buttonState.get(title.titleId)}
            <button
              type="button"
              class="fav-btn"
              class:in-library={inLibrary}
              class:pending={st === 'pending'}
              class:error={st === 'error'}
              disabled={buttonDisabled(inLibrary, st)}
              onclick={() => addFavorite(title)}
            >
              {computeButtonLabel(inLibrary, st)}
            </button>
          {/snippet}
        </TitleCard>
      {/each}
    </div>
    {#if hiddenCount > 0}
      <div class="more-hint">
        Showing first {visible.length} of {filtered.length} matches. Refine your query to see more.
      </div>
    {/if}
  {/if}
</div>

<style>
  .search-page {
    display: flex;
    flex-direction: column;
  }

  .search-bar-wrap {
    position: sticky;
    top: var(--header-h);
    z-index: 10;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .search-input {
    flex: 1;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 14px;
    font-size: 1rem;
    color: var(--text);
    outline: none;
    transition: border-color 0.15s;
  }

  .search-input:focus {
    border-color: var(--accent);
  }

  .result-count {
    font-size: 0.8rem;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .catalog-hint {
    color: var(--text-muted);
    opacity: 0.7;
    margin-left: 4px;
  }

  .more-hint {
    text-align: center;
    color: var(--text-muted);
    font-size: 0.85rem;
    padding: 16px;
  }

  .fav-btn {
    margin-top: 6px;
    width: 100%;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
    font-size: 0.72rem;
    padding: 4px 6px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .fav-btn:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .fav-btn:disabled {
    cursor: default;
  }

  .fav-btn.in-library {
    background: #2e7d32;
    border-color: #2e7d32;
    color: #fff;
    opacity: 0.85;
  }

  .fav-btn.pending {
    opacity: 0.7;
  }

  .fav-btn.error {
    background: #b71c1c;
    border-color: #b71c1c;
    color: #fff;
  }
</style>
