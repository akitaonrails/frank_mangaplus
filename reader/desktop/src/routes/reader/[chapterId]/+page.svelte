<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { MangaViewer, MangaPage, Chapter } from '$lib/types';
  import {
    markChapterRead,
    getPageMode,
    setPageMode,
    type PageMode,
  } from '$lib/readState';
  import { proxied } from '$lib/img';
  import { DEFAULT_CLANG, DEFAULT_COUNTRY } from '$lib/lang';

  // Start fetching the next chapter when the user is this close (in
  // currently-loaded pages) to the end of the loaded scroll. Tuned to
  // overlap the network round-trip with the last couple of pages so the
  // join is invisible.
  const PREFETCH_TRIGGER_DISTANCE = 2;

  // The reader inherits locale from the title page via URL params.
  // Defaults apply when navigating to a chapter URL directly.
  let clang = $derived($page.url.searchParams.get('clang') ?? DEFAULT_CLANG);
  let country = $derived($page.url.searchParams.get('country') ?? DEFAULT_COUNTRY);

  // ---------- state ----------

  let loading = $state(true);
  let error = $state('');

  // The first chapter the user opened. We never replace this — it owns
  // the title list, header info, etc.
  let initialViewer: MangaViewer | null = $state(null);

  // Flat list of (page, owning-chapter-id) so we can show a header
  // when chapter changes mid-scroll.
  type LoadedPage = { mp: MangaPage; chapterId: number; chapterName: string };
  let loadedPages: LoadedPage[] = $state([]);
  let loadedChapterIds = new Set<number>();

  // Ordered chapter list of the parent title, ascending by chapter_id
  // (so "next" means next in publication order).
  let allChapters: Chapter[] = $state([]);

  // Auto-advance state
  let fetchingNext = $state(false);

  // Layout: single page per frame or two pages side-by-side. Wide
  // monitors benefit from double. Persisted via localStorage so the
  // choice survives reloads.
  let pageMode: PageMode = $state('single');

  // Pages bundled into render frames. In single mode every page is its
  // own group; in double mode adjacent pages from the *same chapter*
  // pair up. Never pair across chapter boundaries — would mix two
  // chapters into a single rendered frame.
  type PageGroup = {
    pages: LoadedPage[];
    /** Index in loadedPages of the first page in this group. */
    firstPageIndex: number;
  };
  let pageGroups: PageGroup[] = $derived.by(() => {
    if (pageMode === 'single') {
      return loadedPages.map((p, i) => ({ pages: [p], firstPageIndex: i }));
    }
    const groups: PageGroup[] = [];
    let i = 0;
    while (i < loadedPages.length) {
      const a = loadedPages[i];
      const b = loadedPages[i + 1];
      if (b && b.chapterId === a.chapterId) {
        groups.push({ pages: [a, b], firstPageIndex: i });
        i += 2;
      } else {
        groups.push({ pages: [a], firstPageIndex: i });
        i += 1;
      }
    }
    return groups;
  });

  // Currently-visible group (0-indexed into pageGroups).
  let currentGroup = $state(0);
  let frameEls: HTMLElement[] = $state([]);
  let scrollRoot: HTMLElement | undefined = $state();

  let observer: IntersectionObserver | null = null;

  // Derived: the leftmost page index in the visible group (for the
  // header indicator + read-state tracking).
  let currentPageIndex = $derived(pageGroups[currentGroup]?.firstPageIndex ?? 0);
  let currentPage = $derived(currentPageIndex + 1);
  let currentGroupSize = $derived(pageGroups[currentGroup]?.pages.length ?? 1);

  // Currently-visible chapter (derived from the current group's first page)
  let visibleChapterName = $derived(loadedPages[currentPageIndex]?.chapterName ?? '');
  let visibleChapterId = $derived(loadedPages[currentPageIndex]?.chapterId ?? 0);

  // ---------- load ----------

  onMount(() => {
    pageMode = getPageMode();
    void loadInitial();
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      observer?.disconnect();
    };
  });

  onDestroy(() => observer?.disconnect());

  async function loadInitial() {
    const chapterId = parseInt($page.params.chapterId, 10);
    try {
      const v = await invoke<MangaViewer>('get_chapter_pages', {
        chapterId,
        imgQuality: 'super_high',
        viewerMode: 'vertical',
        clang,
        countryCode: country,
      });
      initialViewer = v;
      // Ascending chapter order (the API sometimes returns mixed).
      allChapters = [...(v.chapters ?? [])].sort((a, b) => a.chapterId - b.chapterId);
      appendChapter(v);
      if (v.titleId && v.chapterId) markChapterRead(v.titleId, v.chapterId);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function appendChapter(v: MangaViewer) {
    if (loadedChapterIds.has(v.chapterId)) return;
    loadedChapterIds.add(v.chapterId);
    const pagesOnly = (v.pages ?? [])
      .map(p => p.data?.mangaPage)
      .filter((mp): mp is MangaPage => !!mp);
    loadedPages = [
      ...loadedPages,
      ...pagesOnly.map(mp => ({ mp, chapterId: v.chapterId, chapterName: v.chapterName })),
    ];
  }

  function nextChapterIdAfter(chId: number): number | null {
    const i = allChapters.findIndex(c => c.chapterId === chId);
    if (i < 0 || i === allChapters.length - 1) return null;
    return allChapters[i + 1].chapterId;
  }

  // When the user is within PREFETCH_TRIGGER_DISTANCE pages of the end
  // of the last-loaded chapter, pre-fetch the next one and append.
  // Resulting pages flow continuously.
  async function maybePrefetchNext() {
    if (fetchingNext || loadedPages.length === 0) return;
    const distanceToEnd = loadedPages.length - currentPage;
    if (distanceToEnd > PREFETCH_TRIGGER_DISTANCE) return;

    const lastLoadedChapter = loadedPages[loadedPages.length - 1].chapterId;
    const nextId = nextChapterIdAfter(lastLoadedChapter);
    if (nextId == null || loadedChapterIds.has(nextId)) return;

    fetchingNext = true;
    try {
      const v = await invoke<MangaViewer>('get_chapter_pages', {
        chapterId: nextId,
        imgQuality: 'super_high',
        viewerMode: 'vertical',
        clang,
        countryCode: country,
      });
      appendChapter(v);
    } catch (e) {
      console.warn('[reader] prefetch next chapter failed:', e);
    } finally {
      fetchingNext = false;
    }
  }

  // Mark chapters as read as the user scrolls through them.
  let lastMarkedChapter = $state(0);
  $effect(() => {
    if (visibleChapterId && visibleChapterId !== lastMarkedChapter && initialViewer) {
      markChapterRead(initialViewer.titleId, visibleChapterId);
      lastMarkedChapter = visibleChapterId;
    }
    // Also fire prefetch check whenever currentPage moves.
    void maybePrefetchNext();
  });

  // ---------- nav ----------

  function setupObserver() {
    observer?.disconnect();
    if (frameEls.length === 0 || !scrollRoot) return;
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && entry.intersectionRatio > 0.5) {
            const idx = Number((entry.target as HTMLElement).dataset.groupIndex);
            if (!isNaN(idx)) currentGroup = idx;
          }
        }
      },
      { root: scrollRoot, threshold: [0.5] }
    );
    for (const el of frameEls) if (el) observer.observe(el);
  }

  // Re-bind the observer whenever the frame set changes — that includes
  // appending a new chapter AND toggling pageMode (which regroups).
  $effect(() => {
    // Touch pageGroups so we re-run when grouping changes too.
    void pageGroups.length;
    if (frameEls.length > 0 && !loading) setupObserver();
  });

  function goToGroupIndex(idx: number) {
    if (idx < 0 || idx >= pageGroups.length) return;
    frameEls[idx]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  function togglePageMode() {
    pageMode = pageMode === 'single' ? 'double' : 'single';
    setPageMode(pageMode);
    // After regrouping, settle on the group that contains the page
    // the user was just on, so the toggle doesn't jump them around.
    const oldFirstPage = currentPageIndex;
    queueMicrotask(() => {
      const target = pageGroups.findIndex(g =>
        g.pages.some((_, i) => g.firstPageIndex + i === oldFirstPage)
      );
      if (target >= 0) goToGroupIndex(target);
    });
  }

  function onKey(e: KeyboardEvent) {
    // Forward (next page in manga RTL = visually-left direction).
    // Includes ArrowLeft for the RTL convention plus the conventional
    // scroll-down keys that everyone has muscle memory for.
    if (
      e.key === 'ArrowDown' || e.key === 'j' || e.key === ' ' ||
      e.key === 'PageDown' || e.key === 'ArrowLeft'
    ) {
      e.preventDefault();
      goToGroupIndex(currentGroup + 1);
    } else if (
      e.key === 'ArrowUp' || e.key === 'k' || e.key === 'PageUp' || e.key === 'ArrowRight'
    ) {
      e.preventDefault();
      goToGroupIndex(currentGroup - 1);
    } else if (e.key === 'd' || e.key === 'D') {
      e.preventDefault();
      togglePageMode();
    } else if (e.key === 'Escape') {
      goBack();
    }
  }

  function goBack() {
    if (initialViewer) goto(`/title/${initialViewer.titleId}`);
    else history.back();
  }

  // Click zones map to manga RTL: left half = forward (next), right
  // half = back (previous). Matches the physical motion of flipping a
  // bound manga page from right to left.
  function onZoneClick(direction: 'prev' | 'next') {
    if (direction === 'next') goToGroupIndex(currentGroup + 1);
    else goToGroupIndex(currentGroup - 1);
  }
</script>

<svelte:head>
  <title>
    {initialViewer ? `${initialViewer.titleName} — ${visibleChapterName || initialViewer.chapterName}` : 'Reader'} — FRANK MANGA+
  </title>
</svelte:head>

<div class="reader">
  <header class="reader-header">
    <button class="back-btn" onclick={goBack}>← Back</button>
    {#if initialViewer}
      <span class="reader-title">{initialViewer.titleName}</span>
      <span class="reader-chapter">{visibleChapterName || initialViewer.chapterName}</span>
    {/if}

    <!-- right-side controls -->
    <button
      class="mode-toggle"
      onclick={togglePageMode}
      title="Toggle single/double page (press D)"
      aria-label={pageMode === 'single' ? 'Switch to double-page' : 'Switch to single-page'}
    >
      {#if pageMode === 'single'}
        <!-- single page icon -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <rect x="6" y="3" width="12" height="18" rx="1.5" fill="none" stroke="currentColor" stroke-width="2"/>
        </svg>
      {:else}
        <!-- double page icon -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <rect x="2"  y="4" width="9" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
          <rect x="13" y="4" width="9" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
        </svg>
      {/if}
    </button>

    <span class="page-indicator">
      {#if loadedPages.length > 0}
        {#if currentGroupSize === 2}
          {currentPage}-{currentPage + 1} / {loadedPages.length}{#if fetchingNext}…{/if}
        {:else}
          {currentPage} / {loadedPages.length}{#if fetchingNext}…{/if}
        {/if}
      {/if}
    </span>
  </header>

  <main class="reader-main" bind:this={scrollRoot}>
    {#if loading}
      <div class="spinner"></div>
    {:else if error}
      <div class="empty-state"><p>Error: {error}</p></div>
    {:else if loadedPages.length === 0}
      <div class="empty-state"><p>No pages found for this chapter.</p></div>
    {:else}
      <div class="page-stack">
        {#each pageGroups as group, gi (gi)}
          {@const prevPageInPriorGroup =
            group.firstPageIndex > 0
              ? loadedPages[group.firstPageIndex - 1].chapterId
              : 0}
          {@const groupChapterId = group.pages[0].chapterId}
          {#if groupChapterId !== prevPageInPriorGroup && group.firstPageIndex > 0}
            <div class="chapter-divider">▼ {group.pages[0].chapterName}</div>
          {/if}
          <div
            class="page-frame"
            class:is-pair={group.pages.length === 2}
            data-group-index={gi}
            bind:this={frameEls[gi]}
          >
            {#each group.pages as lp, pi (lp.mp.imageUrl)}
              <img
                src={proxied(lp.mp.imageUrl)}
                alt="Page {group.firstPageIndex + pi + 1}"
                loading={group.firstPageIndex + pi < 3 ? 'eager' : 'lazy'}
                decoding="async"
                class="manga-page"
              />
            {/each}
            <!-- RTL click zones: left = next, right = prev (manga reading direction) -->
            <button
              class="click-zone zone-next"
              type="button"
              aria-label="Next page"
              onclick={() => onZoneClick('next')}
            ></button>
            <button
              class="click-zone zone-prev"
              type="button"
              aria-label="Previous page"
              onclick={() => onZoneClick('prev')}
            ></button>
          </div>
        {/each}
        {#if fetchingNext}
          <div class="loading-next"><div class="spinner"></div><span>loading next chapter…</span></div>
        {:else if loadedPages.length > 0 && nextChapterIdAfter(loadedPages[loadedPages.length - 1].chapterId) == null}
          <div class="end-of-title">— end of available chapters —</div>
        {/if}
      </div>
    {/if}
  </main>

  {#if !loading && loadedPages.length > 0}
    <footer class="reader-footer">
      <!-- Bar fills from the right edge to reflect manga RTL reading direction. -->
      <div
        class="progress-bar"
        style:width={(currentPage / loadedPages.length) * 100 + '%'}
      ></div>
      <span class="progress-label">Page {currentPage} of {loadedPages.length}</span>
    </footer>
  {/if}
</div>

<style>
  .reader {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #111;
  }

  .reader-header {
    position: sticky;
    top: 0;
    z-index: 100;
    height: 48px;
    background: rgba(10, 10, 10, 0.92);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    font-size: 0.85rem;
    flex-shrink: 0;
  }

  .back-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 0.85rem;
    padding: 4px 8px;
    border-radius: 4px;
    transition: color 0.15s;
    flex-shrink: 0;
  }

  .back-btn:hover {
    color: var(--text);
  }

  .reader-title {
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 260px;
  }

  .reader-chapter {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 1;
    min-width: 0;
  }

  .mode-toggle {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 6px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    transition: color 0.15s, background 0.15s;
    flex-shrink: 0;
  }

  .mode-toggle:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.06);
  }

  .page-indicator {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  .reader-main {
    flex: 1;
    overflow-y: auto;
    scroll-behavior: smooth;
    /* Snap each .page-frame to the top of the viewport. mandatory means
       the browser always settles on a page boundary, never between. */
    scroll-snap-type: y mandatory;
  }

  .page-stack {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .page-frame {
    /* Size to the contained image — no fixed viewport-height frame.
       That used to letterbox shorter/landscape pages with equal top+
       bottom whitespace, which the user (rightly) read as a huge gap.
       Now the frame is just-the-image, and the only inter-page gap is
       the margin below. */
    width: auto;
    max-width: 100%;
    margin: 0 auto 4px;
    display: block;
    position: relative;
    scroll-snap-align: start;
    scroll-snap-stop: always;
  }

  /* Double-page pair: lay out side-by-side in manga RTL order so the
     first page of the pair sits on the right and the second on the
     left. The user reads right-page first, then jumps to left-page,
     then scrolls/clicks to the next pair. */
  .page-frame.is-pair {
    display: flex;
    flex-direction: row-reverse;
    align-items: flex-start;
    justify-content: center;
    gap: 2px;
  }

  .manga-page {
    /* Cap a single page to the visible reader area (viewport minus
       header + footer + a little for the gap). Width is then
       proportional via the image's intrinsic aspect ratio. */
    max-height: calc(100vh - 48px - 32px - 4px);
    max-width: 100%;
    width: auto;
    height: auto;
    display: block;
    background: #1a1a1a;
    user-select: none;
    pointer-events: none; /* clicks go through to the zones below */
  }

  .page-frame.is-pair .manga-page {
    /* Each image in a pair shares the available width. */
    max-width: 50%;
  }

  .click-zone {
    position: absolute;
    top: 0;
    bottom: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    /* No visual; just a hit-target. */
  }
  /* Manga RTL: left half advances, right half goes back. */
  .zone-next { left: 0;  width: 50%; }
  .zone-prev { right: 0; width: 50%; }
  .click-zone:focus-visible {
    outline: 2px dashed var(--accent);
    outline-offset: -4px;
  }

  .chapter-divider {
    width: 100%;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.9rem;
    font-weight: 600;
    padding: 28px 16px;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.4);
    scroll-snap-align: start;
    margin-bottom: 4px;
  }

  .loading-next, .end-of-title {
    width: 100%;
    color: var(--text-muted);
    font-size: 0.85rem;
    padding: 36px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .reader-footer {
    position: relative;
    height: 32px;
    background: rgba(10, 10, 10, 0.85);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    border-top: 1px solid var(--border);
    font-size: 0.75rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .progress-bar {
    /* Manga RTL: fill grows from the right edge leftward, so the
       "consumed" portion of the bar visually trails the reader's
       direction of travel (right→left). */
    position: absolute;
    bottom: 100%;
    right: 0;
    height: 2px;
    background: var(--accent);
    transition: width 0.2s;
  }

  .progress-label {
    font-variant-numeric: tabular-nums;
  }
</style>
