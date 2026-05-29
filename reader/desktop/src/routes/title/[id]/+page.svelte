<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, tick } from 'svelte';
  import { page } from '$app/stores';
  import type { TitleDetailView, Chapter } from '$lib/types';

  const LANG = 'eng';
  const CLANG = 'eng';
  const COUNTRY = 'US';

  let titleId = $derived(parseInt($page.params.id, 10));

  let loading = $state(true);
  let error = $state('');
  let detail = $state<TitleDetailView | null>(null);
  let isFavorited = $state(false);
  let favPending = $state(false);

  // Flattened chapter list for rendering
  type ChapterRow = { type: 'chapter'; chapter: Chapter } | { type: 'divider'; label: string };
  let rows: ChapterRow[] = $state([]);

  // Virtualization state
  let listContainer: HTMLElement | undefined = $state(undefined);
  let visibleStart = $state(0);
  let visibleEnd = $state(50);
  const ITEM_HEIGHT = 72; // approximate px per row
  const OVERSCAN = 10;

  let totalHeight = $derived(rows.length * ITEM_HEIGHT);
  let offsetTop = $derived(visibleStart * ITEM_HEIGHT);
  let visibleRows = $derived(rows.slice(visibleStart, visibleEnd));

  onMount(async () => {
    await loadDetail();
  });

  async function loadDetail() {
    loading = true;
    error = '';
    try {
      const id = parseInt($page.params.id, 10);
      const d = await invoke<TitleDetailView>('get_title_detail', {
        titleId: id,
        lang: LANG,
        clang: CLANG,
        countryCode: COUNTRY,
      });
      detail = d;
      buildRows(d);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function buildRows(d: TitleDetailView) {
    const built: ChapterRow[] = [];
    if (d.chapterListV2 && d.chapterListV2.length > 0) {
      for (const ch of d.chapterListV2) {
        built.push({ type: 'chapter', chapter: ch });
      }
    } else if (d.chapterListGroup) {
      const grp = d.chapterListGroup;
      const sections: [string, Chapter[]][] = [
        [grp.chapterNumbers, [
          ...grp.firstChapterList,
          ...grp.midChapterList,
          ...grp.lastChapterList,
        ]],
      ];
      for (const [label, chapters] of sections) {
        if (chapters.length > 0) {
          built.push({ type: 'divider', label });
          for (const ch of chapters) {
            built.push({ type: 'chapter', chapter: ch });
          }
        }
      }
    }
    rows = built;
    visibleEnd = Math.min(50, rows.length);
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
  <title>{detail?.title?.name ?? 'Title'} — MANGA+</title>
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
      style="background-image: url('{detail.backgroundImageUrl}')"
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
        {#if detail.titleImageUrl}
          <img class="portrait" src={detail.titleImageUrl} alt={title?.name ?? ''} />
        {/if}

        <button
          class="fav-toggle"
          class:favorited={isFavorited}
          onclick={toggleFavorite}
          disabled={favPending}
        >
          {isFavorited ? '♥ In Library' : '♡ Add to Library'}
        </button>

        {#if detail.overview}
          <p class="overview">{detail.overview}</p>
        {/if}
      </aside>

      <!-- Right column: virtual chapter list -->
      <section class="chapter-section">
        <h2 class="section-heading">Chapters ({rows.filter(r => r.type === 'chapter').length})</h2>
        {#if rows.length === 0}
          <p class="no-chapters">No chapters available.</p>
        {:else}
          <div
            class="chapter-scroll"
            onscroll={onScroll}
            bind:this={listContainer}
          >
            <!-- spacer to maintain correct scroll height -->
            <div style="height: {totalHeight}px; position: relative;">
              <div style="position: absolute; top: {offsetTop}px; left: 0; right: 0;">
                {#each visibleRows as row, i (visibleStart + i)}
                  {#if row.type === 'divider'}
                    <div class="chapter-divider">{row.label}</div>
                  {:else}
                    {@const ch = row.chapter}
                    <a
                      class="chapter-row"
                      href="/reader/{ch.chapterId}"
                    >
                      <div class="chapter-meta">
                        <span class="chapter-name">{ch.name}</span>
                        {#if ch.isUpdated}
                          <span class="badge badge-new">New</span>
                        {/if}
                        {#if ch.alreadyViewed}
                          <span class="badge badge-read">Read</span>
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
    height: 220px;
    background-size: cover;
    background-position: center 30%;
    position: relative;
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
