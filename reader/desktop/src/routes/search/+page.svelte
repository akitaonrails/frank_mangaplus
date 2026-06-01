<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import type {
    SearchView,
    Title,
    AllTitlesPayload,
    AllTitlesRefreshedEvent,
    SubscribedTitlesView,
  } from '$lib/types';
  import TitleCard from '$lib/TitleCard.svelte';
  import { DEFAULT_LANG, DEFAULT_CLANG } from '$lib/lang';
  import {
    flattenSearchView,
    filterTitles,
    paginate,
    computeButtonLabel,
    buttonDisabled,
    DEFAULT_VISIBLE_CAP,
  } from '$lib/searchLogic';

  // Two catalogs, served in tiers:
  //   - curated: ~hundreds of titles from /title_list/search. Fast,
  //     fits in memory trivially, and is what we show on the empty
  //     landing view.
  //   - full: ~thousands of titles from /title_list/all_v3. Lazily
  //     fetched the first time the user types so the landing view
  //     stays light. Once loaded, all subsequent filtering uses it.
  let curated: Title[] = $state([]);
  let full: Title[] = $state([]);
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
  // Locale currently in effect for this page session. Captured once at
  // mount so every IPC and every refresh-event filter agrees, even if
  // the module-level DEFAULT_LANG ever becomes user-configurable.
  // Matching on these (not on the module constant) means a future
  // language-switcher won't drop the in-flight refresh event for the
  // previous locale.
  const activeLang = DEFAULT_LANG;
  const activeClang = DEFAULT_CLANG;

  onMount(async () => {
    // Parallel fetches: curated catalog (fast) + library set (for
    // already-in-library indicators). Either can fail independently
    // without blocking the other.
    const curatedP = invoke<SearchView>('search', {
      lang: activeLang,
      clang: activeClang,
    });
    const libP = invoke<SubscribedTitlesView>('get_favorites');

    try {
      const view = await curatedP;
      curated = flattenSearchView(view);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }

    try {
      const libView = await libP;
      libraryIds = new Set((libView.titles ?? []).map(t => t.titleId));
    } catch (e) {
      console.warn('[search] library fetch failed (button state will degrade):', e);
    }

    // Listen for SWR background-refresh completions. The Rust side
    // emits this after a stale read has been re-fetched; payload
    // carries the merged title list inline so we don't have to
    // re-invoke.
    unlisten = await listen<AllTitlesRefreshedEvent>(
      'all_titles_refreshed',
      ev => {
        if (ev.payload.lang !== activeLang || ev.payload.clang !== activeClang) {
          return;
        }
        full = ev.payload.titles ?? [];
        fullStatus = 'ready';
        catalogSource = 'fresh (refreshed)';
        console.log(`[search] catalog refreshed: ${ev.payload.titleCount} titles`);
      },
    );
  });

  onDestroy(() => {
    unlisten?.();
  });

  /** Lazy-fetch the full catalog. Called the first time the user
   *  types; subsequent calls are no-ops because fullStatus !== 'idle'.
   *  The Rust side handles SWR cache reads + the two-bucket merge —
   *  this returns immediately if there's a warm cache. */
  async function loadFullCatalogIfNeeded() {
    if (fullStatus !== 'idle') return;
    fullStatus = 'loading';
    try {
      const payload = await invoke<AllTitlesPayload>('get_all_titles_cached', {
        lang: activeLang,
        clang: activeClang,
      });
      full = payload.titles ?? [];
      fullStatus = 'ready';
      catalogSource = payload.source;
      console.log(
        `[search] full catalog loaded (${full.length} titles, source=${payload.source})`,
      );
    } catch (e) {
      // Network failed AND no cache exists → fall back to curated.
      // We don't surface this as a page-level error because filtering
      // still works against the curated set; the user just sees a
      // smaller match space.
      console.warn('[search] full catalog fetch failed; staying on curated:', e);
      fullStatus = 'idle';
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
      await invoke<void>('add_favorite', { titleId: title.titleId });
      // Optimistic library add — the next /favorites poll would
      // confirm it; meanwhile the button flips to ✓ In Library.
      libraryIds = new Set(libraryIds).add(title.titleId);
      const next = new Map(buttonState);
      next.delete(title.titleId);
      buttonState = next;
    } catch (e) {
      console.warn(`[search] add_favorite ${title.titleId} failed:`, e);
      buttonState = new Map(buttonState).set(title.titleId, 'error');
      setTimeout(() => {
        const next = new Map(buttonState);
        next.delete(title.titleId);
        buttonState = next;
      }, 2000);
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
    <div class="empty-state"><p>Error: {error}</p></div>
  {:else if filtered.length === 0}
    <div class="empty-state">
      <p>No titles match "{query}"{fullStatus === 'ready' ? ' in the full catalog.' : '.'}</p>
    </div>
  {:else}
    <div class="title-grid">
      {#each visible as title (title.titleId)}
        <TitleCard {title}>
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
