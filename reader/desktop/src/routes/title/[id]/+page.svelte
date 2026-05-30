<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { TitleDetailView, Chapter, SubscribedTitlesView } from '$lib/types';
  import {
    getReadChapters,
    getLastReadChapter,
    getSortDescending,
    setSortDescending,
  } from '$lib/readState';
  import { proxied } from '$lib/img';
  import { DEFAULT_LANG, DEFAULT_CLANG, DEFAULT_COUNTRY } from '$lib/lang';

  // URL-controlled. Library cards encode the title's own language as a
  // `?lang=` query param so e.g. a Portuguese title's detail view comes
  // back in Portuguese. Falls back to the global default when absent.
  let lang = $derived($page.url.searchParams.get('lang') ?? DEFAULT_LANG);
  let clang = $derived($page.url.searchParams.get('clang') ?? DEFAULT_CLANG);
  let country = $derived($page.url.searchParams.get('country') ?? DEFAULT_COUNTRY);

  let titleId = $derived(parseInt($page.params.id, 10));

  let loading = $state(true);
  let error = $state('');
  let detail: TitleDetailView | null = $state(null);
  let isFavorited = $state(false);
  let favPending = $state(false);
  let sortDesc = $state(true);
  let readSet: Set<number> = $state(new Set());
  let lastReadId: number | null = $state(null);

  // Flattened chapter list for rendering
  type ChapterRow = { type: 'chapter'; chapter: Chapter } | { type: 'divider'; label: string };
  let rows: ChapterRow[] = $state([]);

  // Virtualization state
  let listContainer: HTMLElement | undefined = $state(undefined);
  let visibleStart = $state(0);
  let visibleEnd = $state(50);
  const ITEM_HEIGHT = 72; // approximate px per row
  const OVERSCAN = 10;

  let bannerCss = $derived(
    detail && detail.backgroundImageUrl
      ? 'url(' + proxied(detail.backgroundImageUrl) + ')'
      : 'none'
  );

  let totalHeight = $derived(rows.length * ITEM_HEIGHT);
  let offsetTop = $derived(visibleStart * ITEM_HEIGHT);
  let visibleRows = $derived(rows.slice(visibleStart, visibleEnd));

  // Suffix appended to /reader/<id> links so the reader inherits this
  // page's locale. Derived from clang/country so reactivity stays clean.
  let readerSuffix = $derived.by(() => {
    const qs = new URLSearchParams();
    if (clang !== DEFAULT_CLANG) qs.set('clang', clang);
    if (country !== DEFAULT_COUNTRY) qs.set('country', country);
    const s = qs.toString();
    return s ? '?' + s : '';
  });

  onMount(async () => {
    sortDesc = getSortDescending();
    await loadDetail();
  });

  // Reload read-state and rows whenever titleId / sortDesc change.
  $effect(() => {
    if (detail) {
      readSet = getReadChapters(titleId);
      lastReadId = getLastReadChapter(titleId);
      buildRows(detail);
    }
  });

  async function loadDetail() {
    loading = true;
    error = '';
    const id = parseInt($page.params.id, 10);
    try {
      const d = await invoke<TitleDetailView>('get_title_detail', {
        titleId: id,
        lang,
        clang,
        countryCode: country,
      });
      detail = d;
      readSet = getReadChapters(id);
      lastReadId = getLastReadChapter(id);
      buildRows(d);

      try {
        const favs = await invoke<SubscribedTitlesView>('get_favorites');
        isFavorited = (favs.titles ?? []).some(t => t.titleId === id);
      } catch (e) {
        console.warn('[title] fetching favorites failed:', e);
      }
    } catch (e) {
      console.error('[title] loadDetail error:', e);
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function buildRows(d: TitleDetailView) {
    let chapters: Chapter[] = [];
    let dividers: { afterIndex: number; label: string }[] = [];

    if (d.chapterListV2 && d.chapterListV2.length > 0) {
      chapters = [...d.chapterListV2];
    } else if (d.chapterListGroup) {
      const grp = d.chapterListGroup;
      chapters = [
        ...grp.firstChapterList,
        ...grp.midChapterList,
        ...grp.lastChapterList,
      ];
      if (grp.chapterNumbers) {
        dividers.push({ afterIndex: -1, label: grp.chapterNumbers });
      }
    }

    // Server returns ascending (oldest → newest). Reverse for descending.
    if (sortDesc) chapters.reverse();

    const built: ChapterRow[] = [];
    for (const div of dividers) if (div.afterIndex === -1) built.push({ type: 'divider', label: div.label });
    for (const ch of chapters) built.push({ type: 'chapter', chapter: ch });

    // Important: use built.length (not rows.length) here. Reading rows
    // right after writing it inside the $effect → infinite reactive loop
    // ("effect_update_depth_exceeded"). Same value, different dep graph.
    rows = built;
    visibleEnd = Math.min(50, built.length);
    if (listContainer) listContainer.scrollTop = 0;
  }

  function toggleSort() {
    sortDesc = !sortDesc;
    setSortDescending(sortDesc);
    if (detail) buildRows(detail);
  }

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
    const scrollTop = el.scrollTop;
    const viewportH = el.clientHeight;
    const start = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN);
    const end = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ITEM_HEIGHT) + OVERSCAN);
    visibleStart = start;
    visibleEnd = end;
  }

  async function toggleFavorite() {
    if (favPending || !detail?.title) return;
    favPending = true;
    try {
      if (isFavorited) {
        await invoke<void>('remove_favorite', { titleId: detail.title.titleId });
        isFavorited = false;
      } else {
        await invoke<void>('add_favorite', { titleId: detail.title.titleId });
        isFavorited = true;
      }
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
  <div class="empty-state"><p>Error: {error}</p></div>
{:else if detail}
  {@const title = detail.title}
  <div class="detail-page">
    <!-- Banner -->
    <div
      class="banner"
      class:has-image={!!detail.backgroundImageUrl}
      style:background-image={bannerCss}
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
          {isFavorited ? '♥ Remove from Library' : '♡ Add to Library'}
        </button>

        {#if detail.overview}
          <p class="overview">{detail.overview}</p>
        {/if}
      </aside>

      <!-- Right column: virtual chapter list -->
      <section class="chapter-section">
        <div class="chapter-header">
          <h2 class="section-heading">Chapters ({rows.filter(r => r.type === 'chapter').length})</h2>
          <div class="chapter-actions">
            {#if lastReadId != null}
              <a class="continue-link" href="/reader/{lastReadId}{readerSuffix}">Continue ▶</a>
            {/if}
            <button class="sort-btn" onclick={toggleSort} title="Toggle sort order">
              {sortDesc ? '↓ Newest first' : '↑ Oldest first'}
            </button>
          </div>
        </div>
        {#if rows.length === 0}
          <p class="no-chapters">No chapters available.</p>
        {:else}
          <div
            class="chapter-scroll"
            onscroll={onScroll}
            bind:this={listContainer}
          >
            <!-- spacer to maintain correct scroll height -->
            <div style:height={totalHeight + 'px'} style:position="relative">
              <div
                style:position="absolute"
                style:top={offsetTop + 'px'}
                style:left="0"
                style:right="0"
              >
                {#each visibleRows as row, i (visibleStart + i)}
                  {#if row.type === 'divider'}
                    <div class="chapter-divider">{row.label}</div>
                  {:else}
                    {@const ch = row.chapter}
                    <a
                      class="chapter-row"
                      class:is-read={readSet.has(ch.chapterId)}
                      class:is-last-read={ch.chapterId === lastReadId}
                      href="/reader/{ch.chapterId}{readerSuffix}"
                      onclick={(e) => openChapter(ch.chapterId, e)}
                    >
                      <div class="chapter-meta">
                        <span class="chapter-name">{ch.name}</span>
                        {#if ch.isUpdated}
                          <span class="badge badge-new">New</span>
                        {/if}
                        {#if readSet.has(ch.chapterId)}
                          <span class="badge badge-read">Read</span>
                        {/if}
                        {#if ch.chapterId === lastReadId}
                          <span class="badge badge-last">Last opened</span>
                        {/if}
                      </div>
                      {#if ch.subTitle}
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
    min-height: 72px;
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
