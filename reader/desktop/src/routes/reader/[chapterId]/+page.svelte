<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy, tick } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { MangaViewer, MangaPage, Chapter, TitleDetailView } from '$lib/types';
  import {
    markChapterRead,
    getPageMode,
    setPageMode,
    nextPageMode,
    getLastReadPage,
    setLastReadPage,
    type PageMode,
  } from '$lib/readState';
  import { proxied } from '$lib/img';
  import { DEFAULT_LANG, DEFAULT_CLANG, DEFAULT_COUNTRY } from '$lib/lang';

  // Start fetching the next chapter when the user is this close (in
  // currently-loaded pages) to the end of the loaded scroll. Tuned to
  // overlap the network round-trip with the last couple of pages so the
  // join is invisible.
  const PREFETCH_TRIGGER_DISTANCE = 2;

  // The reader inherits locale from the title page via URL params.
  // Defaults apply when navigating to a chapter URL directly.
  let lang = $derived($page.url.searchParams.get('lang') ?? DEFAULT_LANG);
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
    // In double-cover mode the first page of each chapter ships solo so
    // pairing starts on the next page — matches how printed manga binds
    // a cover singly before the first spread.
    let coverOffsetActive = pageMode === 'double-cover';
    let currentChapter = loadedPages[0]?.chapterId;
    while (i < loadedPages.length) {
      const a = loadedPages[i];
      if (a.chapterId !== currentChapter) {
        currentChapter = a.chapterId;
        if (pageMode === 'double-cover') coverOffsetActive = true;
      }
      if (coverOffsetActive) {
        groups.push({ pages: [a], firstPageIndex: i });
        coverOffsetActive = false;
        i += 1;
        continue;
      }
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
      // The manga_viewer_v3 response's `chapters` field is NOT the full
      // chapter list — for most titles past chapter 3-ish it's truncated
      // to the first few chapters. Kaiju No. 8 chapter 54 returns only
      // chapters 1-3 in its viewer.chapters, which made nextChapterIdAfter
      // think the user was past the end of the title and trigger the
      // end-of-title indicator incorrectly.
      // Use viewer.chapters as a temporary list so navigation works in
      // the first second, then replace with the canonical list from
      // title_detail once that arrives.
      allChapters = [...(v.chapters ?? [])].sort((a, b) => a.chapterId - b.chapterId);
      appendChapter(v);
      if (v.titleId && v.chapterId) markChapterRead(v.titleId, v.chapterId);

      // Kick off title_detail in the background to get the authoritative
      // chapter list. Doesn't block the user seeing the first pages.
      if (v.titleId) {
        void invoke<TitleDetailView>('get_title_detail', {
          titleId: v.titleId,
          lang,
          clang,
          countryCode: country,
        })
          .then(detail => {
            const canonical: Chapter[] =
              detail.chapterListV2 && detail.chapterListV2.length > 0
                ? detail.chapterListV2
                : detail.chapterListGroup
                  ? [
                      ...detail.chapterListGroup.firstChapterList,
                      ...detail.chapterListGroup.midChapterList,
                      ...detail.chapterListGroup.lastChapterList,
                    ]
                  : [];
            if (canonical.length > allChapters.length) {
              // Preserve title_detail's natural order — that's the
              // publisher's intended reading order. Sorting by
              // chapterId breaks for titles where ids aren't
              // monotonic with chapter number; Kaiju No. 8 chapters
              // 124/125 are a live example, where #125 has a lower
              // id than #124 because the API reassigned ids during
              // a re-upload at some point.
              allChapters = [...canonical];
            }
          })
          .catch(e => console.warn('[reader] title_detail fetch failed (using viewer.chapters):', e));
      }

      // Resume reading: if we left this chapter mid-read last time,
      // scroll to that page. Wait for frames to bind first, then find
      // the group containing the saved page and jump there. Suppresses
      // the chapter-change flash since this is the initial mount.
      const resumePage = getLastReadPage(chapterId);
      if (resumePage && resumePage > 1) {
        queueMicrotask(() => {
          const targetGroup = pageGroups.findIndex(g =>
            g.firstPageIndex <= resumePage - 1 &&
            resumePage - 1 < g.firstPageIndex + g.pages.length
          );
          if (targetGroup > 0 && frameEls[targetGroup]) {
            // 'instant' so the user lands where they were without a
            // visible scroll animation from page 1.
            frameEls[targetGroup].scrollIntoView({ behavior: 'instant' as ScrollBehavior, block: 'start' });
          }
        });
      }
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
  async function maybePrefetchNext(force = false) {
    if (fetchingNext || loadedPages.length === 0) return;
    const distanceToEnd = loadedPages.length - currentPage;
    if (!force && distanceToEnd > PREFETCH_TRIGGER_DISTANCE) return;

    const lastLoadedChapter = loadedPages[loadedPages.length - 1].chapterId;
    const nextId = nextChapterIdAfter(lastLoadedChapter);
    if (nextId == null || loadedChapterIds.has(nextId)) return;

    fetchingNext = true;
    try {
      const timeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error('prefetch timed out')), 12_000),
      );
      const v = await Promise.race([
        invoke<MangaViewer>('get_chapter_pages', {
          chapterId: nextId,
          imgQuality: 'super_high',
          viewerMode: 'vertical',
          clang,
          countryCode: country,
        }),
        timeout,
      ]);
      appendChapter(v);
    } catch (e) {
      console.warn('[reader] prefetch next chapter failed:', e);
    } finally {
      // Always release the flag so the manual button (or another
      // currentPage move) can retry, even if the call timed out.
      fetchingNext = false;
    }
  }

  // Mark chapters as read as the user scrolls through them.
  // chapterFlashKey is bumped on each transition so the header chapter
  // text re-mounts with the .flash class and the glow animation reruns.
  let lastMarkedChapter = $state(0);
  let chapterFlashKey = $state(0);
  $effect(() => {
    if (visibleChapterId && visibleChapterId !== lastMarkedChapter && initialViewer) {
      markChapterRead(initialViewer.titleId, visibleChapterId);
      // Suppress the very first flash on initial mount — only flash on
      // genuine mid-read chapter transitions.
      if (lastMarkedChapter !== 0) chapterFlashKey += 1;
      lastMarkedChapter = visibleChapterId;
    }
    // Persist the user's current reading position per-chapter so
    // re-opening this chapter later resumes here.
    if (visibleChapterId && currentPage >= 1 && loadedPages[currentPageIndex]) {
      // currentPage is 1-indexed across all loaded chapters; convert
      // to chapter-local page number by subtracting the index of the
      // chapter's first page.
      const chapterFirstIdx = loadedPages.findIndex(p => p.chapterId === visibleChapterId);
      if (chapterFirstIdx >= 0) {
        setLastReadPage(visibleChapterId, currentPageIndex - chapterFirstIdx + 1);
      }
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

  // Horizontal page-flip animation for manga-RTL gestures (left/right
  // click zones and ArrowLeft/ArrowRight). Distinct from goToGroupIndex
  // which the vertical-scroll keys (Space, ArrowDown, j, PageDown) keep
  // using — those preserve the familiar smooth-scroll feel.
  //
  // Three phases:
  //  1. slide the current view off horizontally (left for forward, right
  //     for back)
  //  2. while the stack is off-screen, jump-scroll vertically to the
  //     target frame and reposition the stack on the *opposite* side
  //  3. slide the stack back to center, revealing the new frame
  //
  // The IntersectionObserver still updates currentGroup naturally at the
  // end of phase 3 once the new frame is past 50% visibility.
  let pageStackEl: HTMLElement | undefined = $state();
  let flipping = false;

  async function pageFlip(direction: 'forward' | 'back') {
    if (flipping || !scrollRoot || !pageStackEl) return;
    const targetGroup = currentGroup + (direction === 'forward' ? 1 : -1);
    if (targetGroup < 0 || targetGroup >= pageGroups.length) return;
    const targetEl = frameEls[targetGroup];
    if (!targetEl) return goToGroupIndex(targetGroup);

    flipping = true;
    // Forward = next page = visually-left direction in manga RTL, so the
    // current view slides off to the LEFT and the new view comes in
    // from the RIGHT. Back is the mirror.
    const slideOut = direction === 'forward' ? '-100%' : '100%';
    const slideIn = direction === 'forward' ? '100%' : '-100%';
    const DURATION = 220;

    try {
      pageStackEl.style.willChange = 'transform';
      pageStackEl.style.transition = `transform ${DURATION}ms ease-out`;
      pageStackEl.style.transform = `translateX(${slideOut})`;
      await new Promise(r => setTimeout(r, DURATION));

      // Mid-flip: nothing is visible, so we can jump the scroll and
      // reset the transform without the user seeing either change.
      pageStackEl.style.transition = 'none';
      targetEl.scrollIntoView({ behavior: 'instant' as ScrollBehavior, block: 'start' });
      pageStackEl.style.transform = `translateX(${slideIn})`;
      void pageStackEl.offsetHeight; // force reflow so the next transition takes

      pageStackEl.style.transition = `transform ${DURATION}ms ease-out`;
      pageStackEl.style.transform = 'translateX(0)';
      await new Promise(r => setTimeout(r, DURATION));
    } finally {
      pageStackEl.style.transition = '';
      pageStackEl.style.willChange = '';
      pageStackEl.style.transform = '';
      flipping = false;
    }
  }

  function togglePageMode() {
    pageMode = nextPageMode(pageMode);
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

  // Unified navigation: every forward/back input (keys, click zones)
  // routes through advance() so they all behave the same way at chapter
  // boundaries. Within a chapter it's just the local group move. At the
  // last loaded page going forward, it force-fetches the next chapter
  // (no more silent no-op). At the first loaded page going back, it
  // navigates to the previous chapter's URL — fresh mount, so auto-
  // advance correctly considers the new chapter's pages.
  async function advance(direction: 'forward' | 'back', animation: 'scroll' | 'flip') {
    if (direction === 'forward') {
      if (currentGroup + 1 < pageGroups.length) {
        if (animation === 'flip') void pageFlip('forward');
        else goToGroupIndex(currentGroup + 1);
        return;
      }
      // At end of loaded scroll — pull next chapter and then jump in.
      const lastChId = loadedPages[loadedPages.length - 1]?.chapterId;
      if (lastChId == null) return;
      if (nextChapterIdAfter(lastChId) == null) return; // truly end of title
      await maybePrefetchNext(true);
      // Wait for Svelte to flush the new pageGroups → frameEls bindings.
      // Without this tick(), goToGroupIndex hits frameEls[N+1] before
      // bind:this has fired on the new frame, frameEls[N+1] is undefined,
      // and the call silently no-ops. That was the "space bar doesn't
      // cross the chapter boundary" bug — mouse scrolling worked because
      // it doesn't depend on frameEls.
      await tick();
      if (currentGroup + 1 < pageGroups.length) {
        if (animation === 'flip') void pageFlip('forward');
        else goToGroupIndex(currentGroup + 1);
      }
    } else {
      if (currentGroup > 0) {
        if (animation === 'flip') void pageFlip('back');
        else goToGroupIndex(currentGroup - 1);
        return;
      }
      // At the start of the loaded scroll. Navigate sideways to the
      // previous chapter rather than trying to prepend pages — fresh
      // mount keeps the IntersectionObserver and scroll state sane.
      const firstChId = loadedPages[0]?.chapterId;
      if (firstChId == null) return;
      const prevId = prevChapterIdBefore(firstChId);
      if (prevId == null) return;
      const qs = new URLSearchParams();
      if (clang !== DEFAULT_CLANG) qs.set('clang', clang);
      if (country !== DEFAULT_COUNTRY) qs.set('country', country);
      const suffix = qs.toString();
      goto(`/reader/${prevId}${suffix ? '?' + suffix : ''}`);
    }
  }

  function onKey(e: KeyboardEvent) {
    // Vertical scroll keys: smooth-scroll style, no horizontal flip.
    if (
      e.key === 'ArrowDown' || e.key === 'j' || e.key === ' ' || e.key === 'PageDown'
    ) {
      e.preventDefault();
      void advance('forward', 'scroll');
    } else if (
      e.key === 'ArrowUp' || e.key === 'k' || e.key === 'PageUp'
    ) {
      e.preventDefault();
      void advance('back', 'scroll');
    }
    // Horizontal manga-RTL keys: page-flip animation.
    else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      void advance('forward', 'flip');
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      void advance('back', 'flip');
    }
    // Other shortcuts.
    else if (e.key === 'd' || e.key === 'D') {
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
  // half = back (previous). Routes through advance() so chapter-boundary
  // loading kicks in automatically — same code path as the arrow keys.
  function onZoneClick(direction: 'prev' | 'next') {
    void advance(direction === 'next' ? 'forward' : 'back', 'flip');
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
      <!-- {#key} re-mounts the span on each chapter transition so the
           .flash CSS animation reruns from the start. -->
      {#key chapterFlashKey}
        <span class="reader-chapter" class:flash={chapterFlashKey > 0}>
          {visibleChapterName || initialViewer.chapterName}
        </span>
      {/key}
    {/if}

    <!-- right-side controls -->
    <button
      class="mode-toggle"
      onclick={togglePageMode}
      title="Cycle page layout: single → double → cover-offset (press D)"
      aria-label="Cycle page layout"
    >
      {#if pageMode === 'single'}
        <!-- single page -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <rect x="6" y="3" width="12" height="18" rx="1.5" fill="none" stroke="currentColor" stroke-width="2"/>
        </svg>
      {:else if pageMode === 'double'}
        <!-- two equal pages side-by-side -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <rect x="2"  y="4" width="9" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
          <rect x="13" y="4" width="9" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
        </svg>
      {:else}
        <!-- cover-offset: solo first then a pair -->
        <svg viewBox="0 0 30 24" width="22" height="18" aria-hidden="true">
          <rect x="1"  y="4" width="6" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
          <rect x="11" y="4" width="7" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
          <rect x="20" y="4" width="7" height="16" rx="1" fill="none" stroke="currentColor" stroke-width="2"/>
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
<div class="page-stack" bind:this={pageStackEl}>
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
              <!--
                width + height attrs let the browser reserve the correct
                aspect-ratio'd space BEFORE the image bytes arrive — no
                Cumulative Layout Shift. Without these, each image's
                arrival reflows the page-stack and bumps the user's
                scroll position downward (very visible in double mode
                because two images load in parallel with possibly
                different aspect ratios). max-width / max-height in CSS
                still cap the rendered size; these attrs only fix the
                pre-load reservation.
              -->
              <img
                src={proxied(lp.mp.imageUrl)}
                alt="Page {group.firstPageIndex + pi + 1}"
                width={lp.mp.width}
                height={lp.mp.height}
                loading={group.firstPageIndex + pi < 3 ? 'eager' : 'lazy'}
                decoding="async"
                class="manga-page"
              />
            {/each}
            <!-- RTL click zones: left half advances, right half goes back -->
            <button
              class="click-zone zone-next"
              type="button"
              aria-label="Next page"
              onclick={(e) => { e.stopPropagation(); onZoneClick('next'); }}
            ></button>
            <button
              class="click-zone zone-prev"
              type="button"
              aria-label="Previous page"
              onclick={(e) => { e.stopPropagation(); onZoneClick('prev'); }}
            ></button>
          </div>
        {/each}
        {#if fetchingNext}
          <div class="loading-next"><div class="spinner"></div><span>loading next chapter…</span></div>
        {:else if loadedPages.length > 0 && nextChapterIdAfter(loadedPages[loadedPages.length - 1].chapterId) == null}
          <!-- True end of the title — no further chapter exists in the
               catalog at this language/country. Make the message big
               and obvious so the user knows the reader didn't just
               stall on a load. -->
          <div class="end-of-title">
            <div class="end-of-title-glyph">🏁</div>
            <h2>You've reached the end</h2>
            <p>No further chapters are available right now. New releases drop on the schedule MANGA Plus publishes — check back later.</p>
            <button class="back-to-title-btn" onclick={goBack}>Back to title page</button>
          </div>
        {:else if loadedPages.length > 0}
          <!-- A next chapter exists but isn't loaded yet. Auto-prefetch
               normally handles this once currentPage moves to within
               PREFETCH_TRIGGER_DISTANCE of the end, but a hung or
               cancelled prefetch can leave the user stuck — give them
               a manual hatch. -->
          <button class="load-next-btn" onclick={() => void maybePrefetchNext(true)}>
            Load next chapter ▶
          </button>
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

  /* Brief glow when a chapter transition crosses while reading, so the
     reader sees that the header label just changed. Re-trigger handled
     by the {#key} wrapper around the element. */
  .reader-chapter.flash {
    animation: chapter-flash 1.4s ease-out;
  }
  @keyframes chapter-flash {
    0%   { color: var(--text-muted); text-shadow: none; }
    15%  { color: var(--accent);     text-shadow: 0 0 12px var(--accent), 0 0 4px var(--accent); }
    100% { color: var(--text-muted); text-shadow: none; }
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
    /* Browsers default to overflow-anchor: auto, which keeps the
       viewport pinned to whatever's visible when content above shifts.
       Explicit here as a belt-and-suspenders against layout shift while
       images further down the stack are still streaming in. */
    overflow-anchor: auto;
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
    width: 50%;
    background: transparent;
    border: none;
    cursor: pointer;
    /* Hit-target only — needs to sit above the manga-page (which has
       pointer-events: none) and above any flex-arranged siblings in
       double-pair mode. Both zones use left-based positioning so the
       flex container can't accidentally re-interpret `right` during
       layout. */
    z-index: 2;
    pointer-events: auto;
  }
  /* Manga RTL: left half advances, right half goes back. */
  .zone-next { left: 0; }
  .zone-prev { left: 50%; }
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

  .loading-next {
    width: 100%;
    color: var(--text-muted);
    font-size: 0.85rem;
    padding: 36px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .end-of-title {
    width: 100%;
    max-width: 460px;
    margin: 80px auto;
    padding: 40px 32px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 14px;
  }

  .end-of-title-glyph {
    font-size: 2.6rem;
    line-height: 1;
  }

  .end-of-title h2 {
    font-size: 1.3rem;
    font-weight: 700;
    color: var(--text);
    margin: 0;
  }

  .end-of-title p {
    color: var(--text-muted);
    font-size: 0.95rem;
    line-height: 1.5;
    margin: 0;
  }

  .back-to-title-btn {
    margin-top: 6px;
    background: var(--accent);
    color: #fff;
    border: none;
    padding: 10px 22px;
    border-radius: 6px;
    font-size: 0.92rem;
    font-weight: 600;
    transition: background 0.15s;
  }

  .back-to-title-btn:hover {
    background: var(--accent-hover);
  }

  .load-next-btn {
    margin: 36px auto;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    padding: 10px 22px;
    border-radius: 6px;
    font-size: 0.9rem;
    transition: color 0.15s, border-color 0.15s;
  }

  .load-next-btn:hover {
    color: var(--text);
    border-color: var(--accent);
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
