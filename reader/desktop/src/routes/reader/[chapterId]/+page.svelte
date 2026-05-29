<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import type { MangaViewer, MangaPage } from '$lib/types';
  import { markChapterRead } from '$lib/readState';

  let loading = $state(true);
  let error = $state('');
  let viewer = $state<MangaViewer | null>(null);
  let mangaPages: MangaPage[] = $state([]);

  let currentPage = $state(1);
  let imgEls: HTMLImageElement[] = $state([]);

  // Intersection observer to track current visible page
  let observer: IntersectionObserver | null = null;

  onMount(async () => {
    const chapterId = parseInt($page.params.chapterId, 10);
    try {
      const v = await invoke<MangaViewer>('get_chapter_pages', {
        chapterId,
        imgQuality: 'super_high',
        viewerMode: 'vertical',
        clang: 'eng',
        countryCode: 'US',
      });
      viewer = v;
      mangaPages = (v.pages ?? [])
        .map(p => p.data?.mangaPage)
        .filter((mp): mp is MangaPage => !!mp);
      // Persist read state — chapter is considered "opened" the moment the
      // server actually returned its pages (so we don't mark broken chapters
      // as read on transient errors).
      if (v.titleId && v.chapterId) {
        markChapterRead(v.titleId, v.chapterId);
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }

    // keyboard handler
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      observer?.disconnect();
    };
  });

  onDestroy(() => {
    observer?.disconnect();
  });

  // Called once images are rendered; set up intersection observer
  function setupObserver() {
    observer?.disconnect();
    if (imgEls.length === 0) return;
    const ratio = 0.4;
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
            if (!isNaN(idx)) {
              currentPage = idx + 1;
            }
          }
        }
      },
      { threshold: ratio }
    );
    for (const img of imgEls) {
      if (img) observer.observe(img);
    }
  }

  // Reactive: whenever imgEls is populated, wire up observer
  $effect(() => {
    if (imgEls.length > 0 && !loading) {
      setupObserver();
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'j') {
      scrollByPage(1);
    } else if (e.key === 'ArrowUp' || e.key === 'k') {
      scrollByPage(-1);
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      // navigate back to title detail (prev/next chapter is a follow-up feature)
      if (viewer) goto(`/title/${viewer.titleId}`);
    }
  }

  function scrollByPage(direction: number) {
    const nextIdx = currentPage - 1 + direction;
    const el = imgEls[nextIdx];
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  }

  function goBack() {
    if (viewer) goto(`/title/${viewer.titleId}`);
    else history.back();
  }
</script>

<svelte:head>
  <title>
    {viewer ? `${viewer.titleName} — ${viewer.chapterName}` : 'Reader'} — MANGA+
  </title>
</svelte:head>

<div class="reader">
  <!-- Reader header -->
  <header class="reader-header">
    <button class="back-btn" onclick={goBack}>← Back</button>
    {#if viewer}
      <span class="reader-title">{viewer.titleName}</span>
      <span class="reader-chapter">{viewer.chapterName}</span>
    {/if}
    <span class="page-indicator">
      {#if mangaPages.length > 0}
        {currentPage} / {mangaPages.length}
      {/if}
    </span>
  </header>

  <!-- Content -->
  <main class="reader-main">
    {#if loading}
      <div class="spinner"></div>
    {:else if error}
      <div class="empty-state"><p>Error: {error}</p></div>
    {:else if mangaPages.length === 0}
      <div class="empty-state"><p>No pages found for this chapter.</p></div>
    {:else}
      <div class="page-stack">
        {#each mangaPages as mp, i (i)}
          <img
            src={mp.imageUrl}
            alt="Page {i + 1}"
            width={mp.width || undefined}
            height={mp.height || undefined}
            loading={i < 3 ? 'eager' : 'lazy'}
            decoding="async"
            data-page-index={i}
            bind:this={imgEls[i]}
            class="manga-page"
          />
        {/each}
      </div>
    {/if}
  </main>

  <!-- Footer progress -->
  {#if !loading && mangaPages.length > 0}
    <footer class="reader-footer">
      <div
        class="progress-bar"
        style="width: {(currentPage / mangaPages.length) * 100}%"
      ></div>
      <span class="progress-label">Page {currentPage} of {mangaPages.length}</span>
    </footer>
  {/if}
</div>

<style>
  .reader {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
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
    flex-shrink: 0;
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
  }

  .page-stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 0 48px;
  }

  .manga-page {
    max-width: 900px;
    width: 100%;
    height: auto;
    display: block;
    background: #1a1a1a;
  }

  .reader-footer {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 32px;
    background: rgba(10, 10, 10, 0.85);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    border-top: 1px solid var(--border);
    font-size: 0.75rem;
    color: var(--text-muted);
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
