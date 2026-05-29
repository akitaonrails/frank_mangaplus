<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { MangaViewer, MangaPage, Chapter } from '$lib/types';
  import { markChapterRead } from '$lib/readState';

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

  // Currently-on-screen page (1-indexed, across all loaded chapters)
  let currentPage = $state(1);
  let frameEls: HTMLElement[] = $state([]);
  let scrollRoot: HTMLElement | undefined = $state();

  let observer: IntersectionObserver | null = null;

  // Currently-visible chapter (derived from currentPage)
  let visibleChapterName = $derived(loadedPages[currentPage - 1]?.chapterName ?? '');
  let visibleChapterId = $derived(loadedPages[currentPage - 1]?.chapterId ?? 0);

  // ---------- load ----------

  onMount(() => {
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
        clang: 'eng',
        countryCode: 'US',
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

  function prevChapterIdBefore(chId: number): number | null {
    const i = allChapters.findIndex(c => c.chapterId === chId);
    if (i <= 0) return null;
    return allChapters[i - 1].chapterId;
  }

  // When the user is within 2 pages of the end of the last-loaded chapter,
  // pre-fetch the next one and append. Resulting pages flow continuously.
  async function maybePrefetchNext() {
    if (fetchingNext || loadedPages.length === 0) return;
    const distanceToEnd = loadedPages.length - currentPage;
    if (distanceToEnd > 2) return;

    const lastLoadedChapter = loadedPages[loadedPages.length - 1].chapterId;
    const nextId = nextChapterIdAfter(lastLoadedChapter);
    if (nextId == null || loadedChapterIds.has(nextId)) return;

    fetchingNext = true;
    try {
      const v = await invoke<MangaViewer>('get_chapter_pages', {
        chapterId: nextId,
        imgQuality: 'super_high',
        viewerMode: 'vertical',
        clang: 'eng',
        countryCode: 'US',
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
            const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
            if (!isNaN(idx)) currentPage = idx + 1;
          }
        }
      },
      { root: scrollRoot, threshold: [0.5] }
    );
    for (const el of frameEls) if (el) observer.observe(el);
  }

  $effect(() => {
    if (frameEls.length > 0 && !loading) setupObserver();
  });

  function goToPageIndex(idx: number) {
    if (idx < 0 || idx >= loadedPages.length) return;
    frameEls[idx]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'j' || e.key === ' ' || e.key === 'PageDown') {
      e.preventDefault();
      goToPageIndex(currentPage); // currentPage is 1-indexed → next idx
    } else if (e.key === 'ArrowUp' || e.key === 'k' || e.key === 'PageUp') {
      e.preventDefault();
      goToPageIndex(currentPage - 2);
    } else if (e.key === 'Escape') {
      goBack();
    }
  }

  function goBack() {
    if (initialViewer) goto(`/title/${initialViewer.titleId}`);
    else history.back();
  }

  // Click on the page: top half → prev, bottom half → next.
  function onZoneClick(direction: 'prev' | 'next') {
    if (direction === 'next') goToPageIndex(currentPage);
    else goToPageIndex(currentPage - 2);
  }
</script>

<svelte:head>
  <title>
    {initialViewer ? `${initialViewer.titleName} — ${visibleChapterName || initialViewer.chapterName}` : 'Reader'} — MANGA+
  </title>
</svelte:head>

<div class="reader">
  <header class="reader-header">
    <button class="back-btn" onclick={goBack}>← Back</button>
    {#if initialViewer}
      <span class="reader-title">{initialViewer.titleName}</span>
      <span class="reader-chapter">{visibleChapterName || initialViewer.chapterName}</span>
    {/if}
    <span class="page-indicator">
      {#if loadedPages.length > 0}
        {currentPage} / {loadedPages.length}{#if fetchingNext}…{/if}
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
        {#each loadedPages as lp, i (i)}
          {@const prevChapterId = i > 0 ? loadedPages[i - 1].chapterId : 0}
          {#if lp.chapterId !== prevChapterId && i > 0}
            <div class="chapter-divider">▼ {lp.chapterName}</div>
          {/if}
          <div
            class="page-frame"
            data-page-index={i}
            bind:this={frameEls[i]}
          >
            <img
              src={lp.mp.imageUrl.replace(/^https:/, 'mpimg:')}
              alt="Page {i + 1}"
              loading={i < 3 ? 'eager' : 'lazy'}
              decoding="async"
              class="manga-page"
            />
            <button
              class="click-zone zone-prev"
              type="button"
              aria-label="Previous page"
              onclick={() => onZoneClick('prev')}
            ></button>
            <button
              class="click-zone zone-next"
              type="button"
              aria-label="Next page"
              onclick={() => onZoneClick('next')}
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

  .page-indicator {
    margin-left: auto;
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

  .click-zone {
    position: absolute;
    left: 0;
    right: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    /* No visual; just a hit-target. */
  }
  .zone-prev { top: 0; height: 35%; }
  .zone-next { bottom: 0; height: 65%; }
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
    position: absolute;
    bottom: 100%;
    left: 0;
    height: 2px;
    background: var(--accent);
    transition: width 0.2s;
  }

  .progress-label {
    font-variant-numeric: tabular-nums;
  }
</style>
