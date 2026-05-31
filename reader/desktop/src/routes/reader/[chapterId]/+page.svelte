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
    getEyeFilter,
    setEyeFilter,
    nextEyeFilter,
    getHelpSeen,
    setHelpSeen,
    type PageMode,
    type EyeFilter,
  } from '$lib/readState';
  import HelpModal from '$lib/HelpModal.svelte';
  import {
    buildPageGroups,
    scanChapterBounds,
    chapterIdAfter,
    chapterIdBefore,
    findGroupContainingPage,
    firstGroupOfChapter,
    keyToReaderAction,
    type LoadedPage,
    type PageGroup,
  } from '$lib/readerLogic';
  import { proxied } from '$lib/img';
  import { DEFAULT_LANG, DEFAULT_CLANG, DEFAULT_COUNTRY } from '$lib/lang';

  /** Immutable Set update helpers — Svelte 5 needs a new reference to
   *  notice the change, and `new Set(old).add(x)` was repeated in 6+
   *  places before this refactor. */
  function setWith(s: Set<number>, n: number): Set<number> {
    return new Set(s).add(n);
  }
  function setWithout(s: Set<number>, n: number): Set<number> {
    const out = new Set(s);
    out.delete(n);
    return out;
  }

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
  // when chapter changes mid-scroll. LoadedPage shape lives in
  // lib/readerLogic.ts so the pure helpers can be unit-tested.
  let loadedPages: LoadedPage[] = $state([]);
  // $state so it's reactive on a par with prefetching/failedChapterIds;
  // immutable updates use the setWith / setWithout helpers above.
  let loadedChapterIds: Set<number> = $state(new Set());

  // Ordered chapter list of the parent title, ascending by chapter_id
  // (so "next" means next in publication order).
  let allChapters: Chapter[] = $state([]);

  // Auto-advance state. Three sets keep us defensive against duplicate
  // and runaway prefetches:
  //   loadedChapterIds    — chapters whose pages are already in
  //                          loadedPages, no point fetching again
  //   prefetchingChapterIds — currently mid-flight; a second fetch
  //                          for the same chapter would just race
  //                          itself
  //   failedChapterIds    — chapter ids the last fetch failed for
  //                          (timeout, network, server error); we
  //                          stop auto-retrying these. The UI offers
  //                          a manual retry that clears the entry
  //                          before re-fetching.
  let prefetchingChapterIds: Set<number> = $state(new Set());
  let failedChapterIds: Set<number> = $state(new Set());
  // Derived for the existing "loading next chapter…" indicator.
  let fetchingNext = $derived(prefetchingChapterIds.size > 0);

  // Timeout used for every chapter fetch (initial and prefetch). A
  // genuine slow connection sometimes needs more than 12s — but on
  // hangs, returning control is more important than waiting forever.
  const CHAPTER_FETCH_TIMEOUT_MS = 12_000;

  /** Wraps invoke('get_chapter_pages') with the same timeout for both
   * loadInitial() and maybePrefetchNext(). Returns null on timeout/
   * error rather than throwing — callers handle the null path. */
  async function fetchChapter(chapterId: number): Promise<MangaViewer | null> {
    try {
      const timeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(`timed out after ${CHAPTER_FETCH_TIMEOUT_MS}ms`)), CHAPTER_FETCH_TIMEOUT_MS),
      );
      return await Promise.race([
        invoke<MangaViewer>('get_chapter_pages', {
          chapterId,
          imgQuality: 'super_high',
          viewerMode: 'vertical',
          clang,
          countryCode: country,
        }),
        timeout,
      ]);
    } catch (e) {
      console.warn(`[reader] fetchChapter(${chapterId}) failed:`, e);
      return null;
    }
  }

  // Layout: single page per frame or two pages side-by-side. Wide
  // monitors benefit from double. Persisted via localStorage so the
  // choice survives reloads.
  let pageMode: PageMode = $state('single');

  // Eye-protection sepia filter; cycles off → low → med → high. Also
  // persisted, also survives reloads.
  let eyeFilter: EyeFilter = $state('off');

  // Help modal: open state controlled here, persistence ("seen once,
  // don't auto-open again") delegated to readState's getHelpSeen /
  // setHelpSeen helpers. First-launch opens it from loadInitial when
  // helpSeen is false.
  let helpOpen = $state(false);
  function openHelp() {
    helpOpen = true;
    setHelpSeen(true);
  }
  function closeHelp() {
    helpOpen = false;
  }

  // Broken-image tracking. The CDN occasionally drops a page mid-fetch
  // or returns a partial response; the <img> fires onerror and shows
  // empty space (the width/height attributes still reserve the right
  // box, so the surrounding layout stays intact). The user gets a
  // per-image "↻ Retry" overlay button AND a global R keybinding to
  // refetch every failed image at once.
  //
  // imageRetryGen is a counter appended to image URLs as a fragment so
  // the browser treats a retry as a fresh request — the same trick the
  // browser DevTools "Empty cache + hard reload" button uses.
  let failedImageUrls: Set<string> = $state(new Set());
  let imageRetryGen = $state(0);

  function imageSrc(url: string): string {
    const base = proxied(url);
    // Fragments don't reach the server (the mpimg handler strips them
    // before forwarding to the CDN), but the browser sees the full
    // URL as distinct from the previous one, so it refetches.
    return imageRetryGen > 0 && failedImageUrls.has(url)
      ? `${base}#retry=${imageRetryGen}`
      : base;
  }

  function onImageError(url: string) {
    if (!failedImageUrls.has(url)) {
      failedImageUrls = setWithStr(failedImageUrls, url);
    }
  }

  function onImageLoad(url: string) {
    // Clear from the failed set if a previous retry succeeded.
    if (failedImageUrls.has(url)) {
      failedImageUrls = setWithoutStr(failedImageUrls, url);
    }
  }

  function retryImage(url: string) {
    // Force a refetch by bumping the gen counter; the imageSrc() helper
    // appends it to this URL's src so the <img> re-requests through
    // mpimg://.
    imageRetryGen += 1;
  }

  function reloadAllImages() {
    if (failedImageUrls.size === 0) return;
    imageRetryGen += 1;
  }

  // String-keyed Set helpers — the existing setWith / setWithout are
  // typed for number (chapter ids); these mirror them for string URLs.
  function setWithStr(s: Set<string>, v: string): Set<string> {
    return new Set(s).add(v);
  }
  function setWithoutStr(s: Set<string>, v: string): Set<string> {
    const out = new Set(s);
    out.delete(v);
    return out;
  }

  // Pages bundled into render frames. See lib/readerLogic.ts for the
  // pure grouping logic + its unit tests.
  let pageGroups: PageGroup[] = $derived(buildPageGroups(loadedPages, pageMode));

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

  // Chapter-local stats for the header indicator + footer progress bar.
  // Single $derived call to scanChapterBounds — see lib/readerLogic.ts
  // for the impl + tests. Pulling the scan out as a pure function
  // dropped the previous three derived chains down to one and gives us
  // explicit regression coverage on the Kaiju ex → #077 transition.
  let chapterBounds = $derived(scanChapterBounds(loadedPages, currentPageIndex));
  let currentChapterFirstIndex = $derived(chapterBounds.firstIndex);
  let chapterPageCount = $derived(chapterBounds.count);
  let pageInChapter = $derived(currentPageIndex - currentChapterFirstIndex + 1);

  // ---------- load ----------

  onMount(() => {
    pageMode = getPageMode();
    eyeFilter = getEyeFilter();
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
    error = '';
    loading = true;
    try {
      const v = await fetchChapter(chapterId);
      if (!v) {
        // Surface a retry path instead of an infinite spinner. The
        // user-visible error block has a Retry button that re-runs
        // loadInitial.
        error = `Couldn't load chapter (timed out after ${CHAPTER_FETCH_TIMEOUT_MS / 1000}s). API may be rate-limited.`;
        return;
      }
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

      // First-time-user help: surface the keymap once on the very first
      // chapter the user opens. setHelpSeen latches it so this doesn't
      // re-appear; the "?" key and the header help button can still
      // re-open it on demand.
      if (!getHelpSeen()) openHelp();

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
      // scroll to that page. await tick() flushes the bind:this on the
      // new frames; without it, the scrollIntoView call below targets a
      // DOM element that hasn't been bound yet and silently no-ops.
      // (queueMicrotask was the prior fix — tick() is the correct one,
      // since Svelte's reactive updates may happen later than a single
      // microtask hop.)
      const resumePage = getLastReadPage(chapterId);
      if (resumePage && resumePage > 1) {
        await tick();
        const targetGroup = findGroupContainingPage(pageGroups, resumePage - 1);
        if (targetGroup > 0 && frameEls[targetGroup]) {
          // 'instant' so the user lands where they were without a
          // visible scroll animation from page 1.
          frameEls[targetGroup].scrollIntoView({ behavior: 'instant' as ScrollBehavior, block: 'start' });
        }
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function appendChapter(v: MangaViewer) {
    if (loadedChapterIds.has(v.chapterId)) return;
    loadedChapterIds = setWith(loadedChapterIds, v.chapterId);
    const pagesOnly = (v.pages ?? [])
      .map(p => p.data?.mangaPage)
      .filter((mp): mp is MangaPage => !!mp);
    loadedPages = [
      ...loadedPages,
      ...pagesOnly.map(mp => ({ mp, chapterId: v.chapterId, chapterName: v.chapterName })),
    ];
  }

  // When the user is within PREFETCH_TRIGGER_DISTANCE pages of the end
  // of the last-loaded chapter, pre-fetch the next one and append.
  // Resulting pages flow continuously.
  //
  // Defensive guards (in order):
  //   1. nothing loaded yet → nothing to extend from
  //   2. user is too far from the end and force=false → wait
  //   3. the canonical chapter list says there's no next chapter
  //   4. next chapter is already loaded (pages in loadedPages)
  //   5. next chapter is already being fetched (avoid the duplicate-
  //      invoke storm that happened in v0.7.x when a slow fetch
  //      timed out but the underlying invoke was still in flight)
  //   6. next chapter was marked as failed and this isn't a forced
  //      retry → don't auto-retry forever
  async function maybePrefetchNext(force = false) {
    if (loadedPages.length === 0) return;
    const distanceToEnd = loadedPages.length - currentPage;
    if (!force && distanceToEnd > PREFETCH_TRIGGER_DISTANCE) return;

    const lastLoadedChapter = loadedPages[loadedPages.length - 1].chapterId;
    const nextId = chapterIdAfter(allChapters, lastLoadedChapter);
    if (nextId == null) return;
    if (loadedChapterIds.has(nextId)) return;
    if (prefetchingChapterIds.has(nextId)) return;
    if (!force && failedChapterIds.has(nextId)) return;

    // Mark in-flight (a fresh Set so the $derived fetchingNext flips).
    prefetchingChapterIds = setWith(prefetchingChapterIds, nextId);
    // On forced retry, clear any prior failure so the same chapter can
    // be revisited if it fails again.
    if (force) failedChapterIds = setWithout(failedChapterIds, nextId);

    const v = await fetchChapter(nextId);

    prefetchingChapterIds = setWithout(prefetchingChapterIds, nextId);
    if (v) {
      appendChapter(v);
    } else {
      // Auto-prefetch stops re-firing for this chapter; manual retry
      // (Load next / Retry button) clears the flag via force=true.
      failedChapterIds = setWith(failedChapterIds, nextId);
    }
  }

  /** Id, name, failure-status of the chapter sitting just after the
   *  last loaded chapter. Returns null when no next chapter exists or
   *  loadedPages is empty. The footer UI consumes this for the
   *  prefetch-error block and the "Load next chapter" hatch. */
  let nextChapterInfo = $derived.by(() => {
    if (loadedPages.length === 0) return null;
    const lastChId = loadedPages[loadedPages.length - 1].chapterId;
    const nextId = chapterIdAfter(allChapters, lastChId);
    if (nextId == null) return null;
    const name = allChapters.find(c => c.chapterId === nextId)?.name ?? '';
    return { id: nextId, name, failed: failedChapterIds.has(nextId) };
  });

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
    if (visibleChapterId && pageInChapter >= 1) {
      setLastReadPage(visibleChapterId, pageInChapter);
    }
  });

  // Separate effect for prefetch checks: depends only on currentPage,
  // not on visibleChapterId/pageInChapter. The previous combined effect
  // re-ran on every reactive dep change in this component, including
  // loadedPages updates from a successful prefetch — which then
  // re-triggered maybePrefetchNext immediately and queued speculative
  // fetches for chapters far past the user. Keeping the trigger narrow
  // means at most one extra prefetch attempt per page move.
  let lastPrefetchPage = $state(-1);
  $effect(() => {
    if (currentPage !== lastPrefetchPage) {
      lastPrefetchPage = currentPage;
      void maybePrefetchNext();
    }
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

  function toggleEyeFilter() {
    eyeFilter = nextEyeFilter(eyeFilter);
    setEyeFilter(eyeFilter);
  }

  async function togglePageMode() {
    pageMode = nextPageMode(pageMode);
    setPageMode(pageMode);
    // After regrouping, settle on the group that contains the page the
    // user was just on, so the toggle doesn't visually jump them.
    // await tick() flushes the pageGroups recomputation + frame
    // re-bindings before we look up the new group index.
    const oldPageIndex = currentPageIndex;
    await tick();
    const target = findGroupContainingPage(pageGroups, oldPageIndex);
    if (target >= 0) goToGroupIndex(target);
  }

  // Unified navigation: every forward/back input (keys, click zones)
  // routes through advance() so they all behave the same way at chapter
  // boundaries. Within a chapter it's just the local group move. At the
  // last loaded page going forward, it force-fetches the next chapter
  // (no more silent no-op). At the first loaded page going back, it
  // navigates to the previous chapter's URL — fresh mount, so auto-
  // advance correctly considers the new chapter's pages.
  // Re-entrancy guard. Rapid space-bar / click-zone tapping at a chapter
  // boundary used to queue multiple in-flight advance() calls; each
  // continuation, when its prefetch eventually resolved, read whatever
  // currentGroup happened to be at THAT moment (potentially shifted by
  // the IntersectionObserver during the user's mid-wait scrolling) and
  // jumped to `that + 1`. The user landed at random places in the new
  // chapter. With the guard, only one advance can be active at a time.
  let advancing = false;

  async function advance(direction: 'forward' | 'back', animation: 'scroll' | 'flip') {
    if (advancing) return;
    advancing = true;
    try {
      if (direction === 'forward') {
        if (currentGroup + 1 < pageGroups.length) {
          // Inside the loaded scroll — local move.
          if (animation === 'flip') await pageFlip('forward');
          else goToGroupIndex(currentGroup + 1);
          return;
        }
        // At end of loaded scroll — pull next chapter and land on its
        // first group explicitly (NOT currentGroup + 1, which can shift
        // during the await if the IntersectionObserver re-fires).
        const lastChIdAtCall = loadedPages[loadedPages.length - 1]?.chapterId;
        if (lastChIdAtCall == null) return;
        const nextChId = chapterIdAfter(allChapters, lastChIdAtCall);
        if (nextChId == null) return; // truly end of title
        await maybePrefetchNext(true);
        // Flush pending bind:this for the new frames before we scroll.
        await tick();
        // Find the new chapter's first group by id, not by index math.
        // Stable even if currentGroup drifted during the prefetch wait.
        const targetGroup = firstGroupOfChapter(loadedPages, pageGroups, nextChId);
        if (targetGroup < 0) return; // prefetch failed silently
        // For both 'scroll' and 'flip' callers we jump cleanly to the
        // new chapter's first page — a flip animation across an entire
        // chapter's worth of pages would look wrong.
        goToGroupIndex(targetGroup);
      } else {
        if (currentGroup > 0) {
          if (animation === 'flip') await pageFlip('back');
          else goToGroupIndex(currentGroup - 1);
          return;
        }
        // At the start of the loaded scroll. Navigate sideways to the
        // previous chapter rather than prepending pages — fresh mount
        // keeps the IntersectionObserver and scroll state sane.
        const firstChId = loadedPages[0]?.chapterId;
        if (firstChId == null) return;
        const prevId = chapterIdBefore(allChapters, firstChId);
        if (prevId == null) return;
        const qs = new URLSearchParams();
        if (clang !== DEFAULT_CLANG) qs.set('clang', clang);
        if (country !== DEFAULT_COUNTRY) qs.set('country', country);
        const suffix = qs.toString();
        goto(`/reader/${prevId}${suffix ? '?' + suffix : ''}`);
      }
    } finally {
      advancing = false;
    }
  }

  /** Jump to the first or last page of the chapter the user is reading.
   *  Uses the already-computed chapter bounds, then finds the group
   *  that contains that edge page — works in every layout mode. */
  function jumpToChapterEdge(edge: 'start' | 'end') {
    const targetPage = edge === 'start'
      ? currentChapterFirstIndex
      : currentChapterFirstIndex + chapterPageCount - 1;
    const targetGroup = findGroupContainingPage(pageGroups, targetPage);
    if (targetGroup >= 0) goToGroupIndex(targetGroup);
  }

  function onKey(e: KeyboardEvent) {
    // When the help modal is open it owns the keyboard. Its own
    // svelte:window handler closes on Escape / "?"; everything else
    // should be a no-op so the reader doesn't navigate underneath.
    if (helpOpen) return;
    const action = keyToReaderAction(e.key);
    if (action == null) return; // unbound, let the browser handle it
    e.preventDefault();
    switch (action) {
      case 'advance-forward-scroll': void advance('forward', 'scroll'); break;
      case 'advance-back-scroll':    void advance('back',    'scroll'); break;
      case 'advance-forward-flip':   void advance('forward', 'flip');   break;
      case 'advance-back-flip':      void advance('back',    'flip');   break;
      case 'jump-chapter-start':     jumpToChapterEdge('start'); break;
      case 'jump-chapter-end':       jumpToChapterEdge('end');   break;
      case 'toggle-page-mode':       togglePageMode(); break;
      case 'toggle-eye-filter':      toggleEyeFilter(); break;
      case 'reload-images':          reloadAllImages(); break;
      case 'open-help':              openHelp(); break;
      case 'go-back':                goBack(); break;
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

    <!-- Eye-protection sepia filter: crescent moon icon, button tints
         to the active accent at higher filter levels so the current
         setting reads at a glance. -->
    <button
      class="filter-toggle"
      class:on={eyeFilter !== 'off'}
      data-level={eyeFilter}
      onclick={toggleEyeFilter}
      title="Cycle eye-protection filter (press F) — current: {eyeFilter}"
      aria-label="Cycle eye-protection filter, current: {eyeFilter}"
    >
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
        <path
          d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"
          fill={eyeFilter === 'off' ? 'none' : 'currentColor'}
          stroke="currentColor"
          stroke-width="2"
          stroke-linejoin="round"
        />
      </svg>
      {#if eyeFilter !== 'off'}
        <span class="filter-level-dot" aria-hidden="true">
          {eyeFilter === 'low' ? '·' : eyeFilter === 'med' ? '··' : '···'}
        </span>
      {/if}
    </button>

    <!-- Help button: opens the keymap modal. Also auto-opens on the
         user's very first chapter (see openHelp in onMount path). -->
    <button
      class="help-toggle"
      onclick={openHelp}
      title="Show keyboard shortcuts (press ?)"
      aria-label="Show keyboard shortcuts"
    >
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
        <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2"/>
        <path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 1.5-2.5 2-2.5 3.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        <circle cx="12" cy="16.5" r="1" fill="currentColor"/>
      </svg>
    </button>

    <span class="page-indicator">
      {#if chapterPageCount > 0}
        {#if currentGroupSize === 2}
          {pageInChapter}-{pageInChapter + 1} / {chapterPageCount}{#if fetchingNext}…{/if}
        {:else}
          {pageInChapter} / {chapterPageCount}{#if fetchingNext}…{/if}
        {/if}
      {/if}
    </span>
  </header>

  <main class="reader-main" bind:this={scrollRoot}>
    {#if loading}
      <div class="spinner"></div>
    {:else if error}
      <div class="empty-state">
        <p>{error}</p>
        <p><button class="retry-btn" onclick={() => void loadInitial()}>↻ Retry</button></p>
      </div>
    {:else if loadedPages.length === 0}
      <div class="empty-state"><p>No pages found for this chapter.</p></div>
    {:else}
<div
        class="page-stack"
        class:eye-low={eyeFilter === 'low'}
        class:eye-med={eyeFilter === 'med'}
        class:eye-high={eyeFilter === 'high'}
        bind:this={pageStackEl}
      >
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
                width + height attrs reserve the correct aspect-ratio'd
                space before bytes arrive (prevents Cumulative Layout
                Shift). onerror/onload track per-image fetch outcome so
                we can surface a retry overlay; the imageSrc helper
                appends a fragment when retrying so the browser refetches.
              -->
              <div class="page-image-wrapper">
                <img
                  src={imageSrc(lp.mp.imageUrl)}
                  alt="Page {group.firstPageIndex + pi + 1}"
                  width={lp.mp.width}
                  height={lp.mp.height}
                  loading={group.firstPageIndex + pi < 3 ? 'eager' : 'lazy'}
                  decoding="async"
                  class="manga-page"
                  class:failed={failedImageUrls.has(lp.mp.imageUrl)}
                  onerror={() => onImageError(lp.mp.imageUrl)}
                  onload={() => onImageLoad(lp.mp.imageUrl)}
                />
                {#if failedImageUrls.has(lp.mp.imageUrl)}
                  <button
                    class="image-retry-btn"
                    type="button"
                    aria-label="Retry loading page {group.firstPageIndex + pi + 1}"
                    onclick={(e) => { e.stopPropagation(); retryImage(lp.mp.imageUrl); }}
                  >
                    ↻ Reload page {group.firstPageIndex + pi + 1}
                  </button>
                {/if}
              </div>
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
          <div class="loading-next">
            <div class="spinner"></div>
            <span>loading next chapter{nextChapterInfo?.name ? ` (${nextChapterInfo.name})` : ''}…</span>
          </div>
        {:else if loadedPages.length > 0 && nextChapterInfo == null}
          <!-- True end of the title — no further chapter exists in the
               catalog at this language/country. -->
          <div class="end-of-title">
            <div class="end-of-title-glyph">🏁</div>
            <h2>You've reached the end</h2>
            <p>No further chapters are available right now. New releases drop on the schedule MANGA Plus publishes — check back later.</p>
            <button class="back-to-title-btn" onclick={goBack}>Back to title page</button>
          </div>
        {:else if nextChapterInfo?.failed}
          <!-- Previous prefetch failed (timeout, network, server error).
               Auto-retry is suppressed via failedChapterIds so the user
               isn't trapped in a spinner storm; manual retry clears the
               failure flag and re-runs maybePrefetchNext with force=true. -->
          <div class="prefetch-error">
            <p>Couldn't load <strong>{nextChapterInfo.name || 'the next chapter'}</strong>.</p>
            <p class="hint">May be rate-limited or temporarily unavailable.</p>
            <button class="retry-btn" onclick={() => void maybePrefetchNext(true)}>
              ↻ Retry {nextChapterInfo.name || 'next chapter'}
            </button>
          </div>
        {:else if loadedPages.length > 0}
          <!-- Next chapter exists, no inflight, no failure: explicit
               "Load it now" hatch for users who scrolled past the
               PREFETCH_TRIGGER_DISTANCE without triggering auto-load. -->
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
        style:width={chapterPageCount > 0 ? (pageInChapter / chapterPageCount) * 100 + '%' : '0%'}
      ></div>
      <span class="progress-label">Page {pageInChapter} of {chapterPageCount}</span>
    </footer>
  {/if}
</div>

<HelpModal open={helpOpen} onclose={closeHelp} />

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

  .filter-toggle {
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 6px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    gap: 2px;
    transition: color 0.15s, background 0.15s;
    flex-shrink: 0;
  }

  .filter-toggle:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.06);
  }

  .filter-toggle.on {
    /* Warm amber tint so the active state reads at a glance. */
    color: #f6c177;
  }

  .filter-level-dot {
    font-size: 0.85rem;
    line-height: 1;
    letter-spacing: -0.05em;
    margin-bottom: 2px;
  }

  .help-toggle {
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
  .help-toggle:hover {
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

  /* Eye-protection sepia filter levels. CSS `filter: sepia()` shifts the
     hue toward amber while preserving the luminance range (contrast stays
     intact). brightness/saturate dial in the night-reading warmth. The
     filter goes on the whole page-stack so background art, gutters, and
     speech-bubble whites all warm together. */
  .page-stack.eye-low  { filter: sepia(0.25) brightness(0.97); }
  .page-stack.eye-med  { filter: sepia(0.50) brightness(0.90) saturate(0.85); }
  .page-stack.eye-high { filter: sepia(0.75) brightness(0.82) saturate(0.70); }

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

  /* Per-image wrapper holds the <img> and the retry-overlay button
     together so the overlay is positioned relative to its own image,
     not the whole frame (important in double-pair mode where one of
     two images may have failed). */
  .page-image-wrapper {
    position: relative;
    display: flex;
    align-items: stretch;
    justify-content: center;
    /* In a flex pair, each wrapper shrinks to its image's intrinsic
       width up to the 50% cap from the page-frame.is-pair rule. */
  }
  .page-frame.is-pair .page-image-wrapper {
    max-width: 50%;
    flex: 0 1 auto;
  }

  .manga-page.failed {
    /* Tint the empty reserved space so the user can see what's broken
       at a glance, even before they read the retry-button text. */
    background: rgba(239, 83, 80, 0.06);
    border: 1px dashed rgba(239, 83, 80, 0.4);
  }

  .image-retry-btn {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: var(--bg-card);
    color: var(--text);
    border: 1px solid var(--accent);
    padding: 10px 20px;
    border-radius: 8px;
    font-size: 0.9rem;
    cursor: pointer;
    z-index: 3; /* above the click zones */
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5);
    transition: background 0.15s, color 0.15s;
  }
  .image-retry-btn:hover {
    background: var(--accent);
    color: #fff;
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
    /* No scroll-snap-align here on purpose. If the divider is a snap
       target, the browser pulls the viewport to it at the chapter
       boundary — leaving the divider pinned at the top of the screen
       and the first page of the new chapter pushed partly below the
       viewport. Space-bar advance then jumps to the page AFTER that
       first page, and the user has to mouse-scroll through the half-
       hidden one. Letting the divider scroll through as a transition
       element makes both programmatic and manual scrolls land cleanly
       on the page frame, where the snap actually belongs. */
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

  .prefetch-error {
    width: 100%;
    max-width: 460px;
    margin: 56px auto;
    padding: 24px 28px;
    text-align: center;
    background: rgba(239, 83, 80, 0.08);
    border: 1px solid rgba(239, 83, 80, 0.4);
    border-radius: 10px;
    color: var(--text);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .prefetch-error .hint {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .retry-btn {
    margin-top: 6px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text);
    padding: 8px 18px;
    border-radius: 6px;
    font-size: 0.9rem;
    transition: color 0.15s, border-color 0.15s, background 0.15s;
  }

  .retry-btn:hover {
    color: #fff;
    border-color: var(--accent);
    background: var(--accent);
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
