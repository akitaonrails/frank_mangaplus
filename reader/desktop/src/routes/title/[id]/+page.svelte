<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { TitleDetailView } from '$lib/types';
  import {
    getReadChapters,
    getLastReadChapter,
    getSortDescending,
    setSortDescending,
    getReadVisibility,
    setReadVisibility,
    nextReadVisibility,
    type ReadVisibility,
  } from '$lib/readState';
  import {
    flattenChapters,
    buildChapterList,
    visibleRange,
    type ChapterRow,
  } from '$lib/chapterListLogic';
  import { proxied } from '$lib/img';
  import { DEFAULT_LANG, DEFAULT_CLANG, DEFAULT_COUNTRY } from '$lib/lang';
  import { withIpcTimeout } from '$lib/ipcTimeout';
  import { addFavorite, getFavorites, getTitleDetail, removeFavorite } from '$lib/ipcCommands';

  // URL-controlled. Library cards encode the title's own language as a
  // `?lang=` query param so e.g. a Portuguese title's detail view comes
  // back in Portuguese. Falls back to the global default when absent.
  let lang = $derived($page.url.searchParams.get('lang') ?? DEFAULT_LANG);
  let clang = $derived($page.url.searchParams.get('clang') ?? DEFAULT_CLANG);
  let country = $derived($page.url.searchParams.get('country') ?? DEFAULT_COUNTRY);

  let titleId = $derived(Number.parseInt($page.params.id ?? '', 10));

  let loading = $state(true);
  let error = $state('');
  let detail: TitleDetailView | null = $state(null);
  let isFavorited = $state(false);
  let favPending = $state(false);
  let favError = $state('');
  let sortDesc = $state(true);
  let readVis: ReadVisibility = $state('hidden');
  let readSet: Set<number> = $state(new Set());
  let lastReadId: number | null = $state(null);

  // Flattened chapter list for rendering
  let rows: ChapterRow[] = $state([]);
  // Prefix sums of row heights — rows vary in height (a read chapter in
  // "compact" mode is a slim line), so the virtualizer indexes this
  // table instead of multiplying by a constant.
  let rowOffsets: number[] = $state([0]);
  let totalChapters = $state(0);
  let unreadCount = $state(0);
  let hiddenCount = $state(0);

  // Virtualization state
  let listContainer: HTMLElement | undefined = $state(undefined);
  let visibleStart = $state(0);
  let visibleEnd = $state(50);
  const OVERSCAN = 10;

  let bannerCss = $derived.by(() => {
    const bg = detail?.backgroundImageUrl;
    return bg ? 'url(' + proxied(bg) + ')' : 'none';
  });

  let totalHeight = $derived(rowOffsets[rowOffsets.length - 1] ?? 0);
  let offsetTop = $derived(rowOffsets[visibleStart] ?? 0);
  let visibleRows = $derived(rows.slice(visibleStart, visibleEnd));
  type StyleMap = Record<string, string>;
  let bannerStyles = $derived({ 'background-image': bannerCss });
  let spacerStyles = $derived({ height: totalHeight + 'px', position: 'relative' });
  let virtualRowsStyles = $derived({ position: 'absolute', top: offsetTop + 'px', left: '0', right: '0' });

  function applyStyles(node: HTMLElement, styles: StyleMap) {
    const update = (next: StyleMap) => {
      for (const [name, value] of Object.entries(next)) {
        node.style.setProperty(name, value);
      }
    };
    update(styles);
    return { update };
  }

  // Suffix appended to /reader/<id> links so the reader inherits this
  // page's locale. Derived from clang/country so reactivity stays clean.
  let readerSuffix = $derived.by(() => {
    const qs = new URLSearchParams();
    if (clang !== DEFAULT_CLANG) qs.set('clang', clang);
    if (country !== DEFAULT_COUNTRY) qs.set('country', country);
    const s = qs.toString();
    return s ? '?' + s : '';
  });

  let loadSeq = 0;

  onMount(() => {
    sortDesc = getSortDescending();
    readVis = getReadVisibility();
  });

  $effect(() => {
    const id = titleId;
    const activeLang = lang;
    const activeClang = clang;
    const activeCountry = country;
    const seq = ++loadSeq;

    if (!Number.isFinite(id)) {
      detail = null;
      rows = [];
      loading = false;
      error = 'Invalid title id.';
      return;
    }

    void loadDetail(id, activeLang, activeClang, activeCountry, seq);
  });

  // Reload read-state and rows whenever titleId / sortDesc / read
  // visibility change. `reads` is passed into buildRows rather than read
  // back off `readSet`: this effect *writes* readSet, and an effect that
  // also reads what it writes re-triggers itself forever
  // ("effect_update_depth_exceeded").
  $effect(() => {
    if (detail) {
      const reads = getReadChapters(titleId);
      readSet = reads;
      lastReadId = getLastReadChapter(titleId);
      buildRows(detail, reads);
    }
  });

  async function loadDetail(
    id = titleId,
    activeLang = lang,
    activeClang = clang,
    activeCountry = country,
    seq = ++loadSeq,
  ) {
    loading = true;
    error = '';
    favError = '';
    detail = null;
    rows = [];
    try {
      const d = await withIpcTimeout(getTitleDetail({
        titleId: id,
        lang: activeLang,
        clang: activeClang,
        countryCode: activeCountry,
      }));
      if (seq !== loadSeq) return;
      detail = d;
      const reads = getReadChapters(id);
      readSet = reads;
      lastReadId = getLastReadChapter(id);
      buildRows(d, reads);
      loading = false;

      void loadFavoriteState(id, seq);
    } catch (e) {
      if (seq !== loadSeq) return;
      console.error('[title] loadDetail error:', e);
      error = e instanceof Error ? e.message : String(e);
    } finally {
      if (seq === loadSeq) loading = false;
    }
  }

  async function loadFavoriteState(id: number, seq = loadSeq) {
    try {
      const favs = await withIpcTimeout(getFavorites());
      if (seq !== loadSeq) return;
      isFavorited = (favs.titles ?? []).some(t => t.titleId === id);
    } catch (e) {
      if (seq !== loadSeq) return;
      console.warn('[title] fetching favorites failed:', e);
      favError = 'Library state unavailable. Favorite changes may fail until you retry.';
    }
  }

  function buildRows(d: TitleDetailView, reads: ReadonlySet<number>, resetScroll = true) {
    const { chapters, leadingDivider } = flattenChapters(d);
    const built = buildChapterList(chapters, {
      sortDesc,
      readSet: reads,
      visibility: readVis,
      leadingDivider,
    });

    // Important: read from `built` (not the rows/rowOffsets state) here.
    // Reading state right after writing it inside the $effect →
    // infinite reactive loop ("effect_update_depth_exceeded"). Same
    // values, different dep graph.
    rows = built.rows;
    rowOffsets = built.offsets;
    totalChapters = built.totalChapters;
    unreadCount = built.unreadCount;
    hiddenCount = built.hiddenCount;
    visibleStart = 0;
    visibleEnd = Math.min(50, built.rows.length);
    if (resetScroll && listContainer) listContainer.scrollTop = 0;
  }

  function toggleSort() {
    sortDesc = !sortDesc;
    setSortDescending(sortDesc);
    if (detail) buildRows(detail, readSet);
  }

  // Cycle: hidden → compact → all. Read chapters are hidden by default
  // so opening a title shows what's left to read; the other two modes
  // bring them back (slim, then full) for re-reading.
  function cycleReadVisibility() {
    readVis = nextReadVisibility(readVis);
    setReadVisibility(readVis);
    if (detail) buildRows(detail, readSet);
  }

  function showAllChapters() {
    if (readVis === 'all') return;
    readVis = 'all';
    setReadVisibility(readVis);
    if (detail) buildRows(detail, readSet);
  }

  let readVisLabel = $derived(
    readVis === 'hidden' ? '\u25CF Unread only'
      : readVis === 'compact' ? '\u25D0 Read collapsed'
      : '\u25CB All chapters',
  );
  let readVisHint = $derived(
    readVis === 'hidden'
      ? `${hiddenCount} read chapter${hiddenCount === 1 ? '' : 's'} hidden \u2014 click to collapse them instead`
      : readVis === 'compact' ? 'Read chapters are collapsed \u2014 click to show them in full'
      : 'All chapters shown \u2014 click to hide read ones',
  );

  function openChapter(chapterId: number, e?: MouseEvent) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    // Pass the title page's locale through so the reader requests pages
    // in the same language/country tuple. Without this the reader
    // falls back to defaults and a Portuguese title would render in
    // English.
    const qs = new URLSearchParams();
    if (clang !== DEFAULT_CLANG) qs.set('clang', clang);
    if (country !== DEFAULT_COUNTRY) qs.set('country', country);
    const suffix = qs.toString();
    goto('/reader/' + chapterId + (suffix ? '?' + suffix : ''));
  }

  function onScroll(e: Event) {
    const el = e.target as HTMLElement;
    const { start, end } = visibleRange(rowOffsets, el.scrollTop, el.clientHeight, OVERSCAN);
    visibleStart = start;
    visibleEnd = end;
  }

  async function toggleFavorite() {
    if (favPending || !detail?.title) return;
    const id = detail.title.titleId;
    const nextFavorited = !isFavorited;
    favPending = true;
    favError = '';
    try {
      if (nextFavorited) {
        await withIpcTimeout(addFavorite(id));
      } else {
        await withIpcTimeout(removeFavorite(id));
      }
      isFavorited = nextFavorited;
    } catch (e) {
      console.warn(`[title] favorite toggle ${id} failed:`, e);
      favError = e instanceof Error ? e.message : String(e);
    } finally {
      favPending = false;
    }
  }
</script>

<svelte:head>
  <title>{detail?.title?.name ?? 'Title'} — FRANK MANGA+</title>
</svelte:head>

{#if loading}
  <div class="spinner"></div>
{:else if error}
  <div class="empty-state">
    <p>Error: {error}</p>
    <p><button class="retry-btn" onclick={() => void loadDetail()}>↻ Retry</button></p>
  </div>
{:else if detail}
  {@const title = detail.title}
  <div class="detail-page">
    <!-- Banner -->
    <div
      class="banner"
      class:has-image={!!detail.backgroundImageUrl}
      use:applyStyles={bannerStyles}
    >
      <div class="banner-overlay">
        <h1 class="banner-title">{title?.name ?? ''}</h1>
        <p class="banner-author">{title?.author ?? ''}</p>
      </div>
    </div>

    <!-- Body -->
    <div class="detail-body">
      <!-- Left column -->
      <aside class="detail-aside">
        {#if title?.portraitImageUrl || detail.titleImageUrl}
          <img
            class="portrait"
            src={proxied(title?.portraitImageUrl ?? detail.titleImageUrl)}
            alt={title?.name ?? ''}
          />
        {/if}

        <button
          class="fav-toggle"
          class:favorited={isFavorited}
          onclick={toggleFavorite}
          disabled={favPending}
        >
          {favPending ? 'Saving…' : isFavorited ? '♥ Remove from Library' : '♡ Add to Library'}
        </button>

        {#if favError}
          <p class="fav-error">{favError}</p>
        {/if}

        {#if detail.overview}
          <p class="overview">{detail.overview}</p>
        {/if}
      </aside>

      <!-- Right column: virtual chapter list -->
      <section class="chapter-section">
        <div class="chapter-header">
          <h2 class="section-heading">
            Chapters
            {#if totalChapters > 0}
              ({unreadCount} unread of {totalChapters})
            {/if}
          </h2>
          <div class="chapter-actions">
            {#if lastReadId != null}
              <a class="continue-link" href="/reader/{lastReadId}{readerSuffix}">Continue ▶</a>
            {/if}
            <button
              class="sort-btn"
              onclick={cycleReadVisibility}
              title={readVisHint}
            >
              {readVisLabel}
            </button>
            <button class="sort-btn" onclick={toggleSort} title="Toggle sort order">
              {sortDesc ? '↓ Newest first' : '↑ Oldest first'}
            </button>
          </div>
        </div>
        {#if totalChapters === 0}
          <p class="no-chapters">No chapters available.</p>
        {:else if unreadCount === 0 && readVis === 'hidden'}
          <p class="no-chapters">
            All {totalChapters} chapters read — nothing new here.
            <button class="link-btn" onclick={showAllChapters}>Show all chapters</button>
          </p>
        {:else}
          <div
            class="chapter-scroll"
            onscroll={onScroll}
            bind:this={listContainer}
          >
            <!-- spacer to maintain correct scroll height -->
            <div use:applyStyles={spacerStyles}>
              <div use:applyStyles={virtualRowsStyles}>
                {#each visibleRows as row, i (visibleStart + i)}
                  {#if row.type === 'divider'}
                    <div class="chapter-divider">{row.label}</div>
                  {:else}
                    {@const ch = row.chapter}
                    <a
                      class="chapter-row"
                      class:is-read={row.read}
                      class:is-compact={row.compact}
                      class:is-last-read={ch.chapterId === lastReadId}
                      href="/reader/{ch.chapterId}{readerSuffix}"
                      onclick={(e) => openChapter(ch.chapterId, e)}
                    >
                      <div class="chapter-meta">
                        <span class="chapter-name">{ch.name}</span>
                        {#if row.compact && ch.subTitle}
                          <span class="chapter-subtitle inline">{ch.subTitle}</span>
                        {/if}
                        {#if ch.isUpdated && !row.read}
                          <span class="badge badge-new">New</span>
                        {/if}
                        {#if row.read && !row.compact}
                          <span class="badge badge-read">Read</span>
                        {/if}
                        {#if ch.chapterId === lastReadId}
                          <span class="badge badge-last">Last opened</span>
                        {/if}
                      </div>
                      {#if ch.subTitle && !row.compact}
                        <span class="chapter-subtitle">{ch.subTitle}</span>
                      {/if}
                    </a>
                  {/if}
                {/each}
              </div>
            </div>
          </div>
        {/if}
      </section>
    </div>
  </div>
{/if}

<style>
  .detail-page {
    display: flex;
    flex-direction: column;
    min-height: calc(100vh - var(--header-h));
  }

  .banner {
    /* Tall + image-backed when we have a backgroundImageUrl; short and
       just-the-title when we don't. min-height must be enough for the
       overlay's h1 + author line + its top/bottom padding (the overlay
       uses justify-content: flex-end, so anything taller than the
       banner spills UPWARD into the header — caught by a screenshot). */
    min-height: 110px;
    background-size: cover;
    background-position: center 30%;
    background-repeat: no-repeat;
    position: relative;
  }

  .banner.has-image {
    height: 220px;
  }

  .banner-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(0,0,0,0.2), rgba(20,20,20,0.92));
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 20px 24px;
  }

  .banner-title {
    font-size: 1.6rem;
    font-weight: 800;
    line-height: 1.2;
    text-shadow: 0 2px 6px rgba(0,0,0,0.8);
  }

  .banner-author {
    font-size: 0.9rem;
    color: #ccc;
    margin-top: 4px;
    text-shadow: 0 1px 4px rgba(0,0,0,0.8);
  }

  .detail-body {
    display: flex;
    gap: 24px;
    padding: 20px;
    flex: 1;
    align-items: flex-start;
  }

  .detail-aside {
    width: 200px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .portrait {
    width: 100%;
    border-radius: 6px;
    aspect-ratio: 2/3;
    object-fit: cover;
    border: 1px solid var(--border);
  }

  .fav-toggle {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
    padding: 8px;
    font-size: 0.85rem;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
    width: 100%;
  }

  .fav-toggle:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .fav-toggle.favorited {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .fav-error {
    color: #ff8f8f;
    font-size: 0.78rem;
    line-height: 1.4;
    margin: 0;
  }

  .retry-btn {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 0.9rem;
    transition: color 0.15s, border-color 0.15s;
  }

  .retry-btn:hover {
    color: var(--text);
    border-color: var(--text-muted);
  }

  .overview {
    font-size: 0.82rem;
    color: var(--text-muted);
    line-height: 1.6;
  }

  .chapter-section {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .section-heading {
    font-size: 1rem;
    font-weight: 700;
    margin-bottom: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .no-chapters {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .chapter-scroll {
    height: calc(100vh - var(--header-h) - 220px - 80px);
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-card);
  }

  .chapter-divider {
    padding: 6px 14px;
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    /* Matches ROW_H_DIVIDER in lib/chapterListLogic.ts. */
    height: 72px;
    display: flex;
    align-items: center;
  }

  .chapter-row {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    /* Fixed, not min-height: the virtualizer positions the visible
       window from a height table (lib/chapterListLogic.ts, ROW_H_FULL),
       so a row that grows would drift out of sync with the spacer. */
    height: 72px;
    overflow: hidden;
    transition: background 0.12s;
    cursor: pointer;
  }

  .chapter-row:hover {
    background: var(--bg-elevated);
  }

  .chapter-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chapter-name {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .chapter-subtitle {
    font-size: 0.8rem;
    color: var(--text-muted);
    margin-top: 3px;
  }

  .chapter-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
    gap: 10px;
  }

  .chapter-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sort-btn {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
    padding: 4px 10px;
    font-size: 0.75rem;
    cursor: pointer;
    transition: color 0.12s, border-color 0.12s;
  }

  .sort-btn:hover {
    color: var(--text);
    border-color: var(--text-muted);
  }

  .continue-link {
    background: var(--accent);
    color: #fff;
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 0.75rem;
    text-decoration: none;
    font-weight: 600;
  }

  .continue-link:hover {
    opacity: 0.9;
  }

  .chapter-row.is-read .chapter-name,
  .chapter-row.is-read .chapter-subtitle {
    color: var(--text-muted);
  }

  /* "Read collapsed" mode: a read chapter shrinks to a single slim line
     so finished chapters stay reachable without pushing unread ones off
     screen. The height here must match ROW_H_COMPACT in
     lib/chapterListLogic.ts — the virtualizer positions rows absolutely
     from those numbers. */
  .chapter-row.is-compact {
    flex-direction: row;
    align-items: center;
    height: 34px;
    padding: 0 14px;
    opacity: 0.62;
  }

  .chapter-row.is-compact .chapter-meta {
    flex-wrap: nowrap;
    overflow: hidden;
    width: 100%;
  }

  .chapter-row.is-compact .chapter-name {
    font-size: 0.8rem;
    font-weight: 500;
    white-space: nowrap;
  }

  .chapter-row.is-compact:hover {
    opacity: 1;
  }

  .chapter-subtitle.inline {
    margin-top: 0;
    font-size: 0.75rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .link-btn {
    background: none;
    border: none;
    color: var(--accent);
    font-size: inherit;
    padding: 0 0 0 4px;
    cursor: pointer;
    text-decoration: underline;
  }

  .chapter-row.is-last-read {
    background: rgba(59, 130, 246, 0.08);
    border-left: 3px solid var(--accent);
  }

  .badge-last {
    background: var(--accent);
    color: #fff;
  }

  @media (max-width: 640px) {
    .detail-body {
      flex-direction: column;
    }

    .detail-aside {
      width: 100%;
      flex-direction: row;
      flex-wrap: wrap;
    }

    .portrait {
      width: 120px;
    }

    .chapter-scroll {
      height: 60vh;
    }
  }
</style>
