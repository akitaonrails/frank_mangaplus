<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { SubscribedTitlesView, Title } from '$lib/types';
  import TitleCard from '$lib/TitleCard.svelte';
  import { langCode } from '$lib/lang';
  import { withIpcTimeout } from '$lib/ipcTimeout';

  let loading = $state(true);
  let error = $state('');
  let titles: Title[] = $state([]);

  // Fetch races against a generous timeout so a hung IPC call surfaces
  // as a retry-able error instead of an infinite spinner — that was the
  // failure mode the user hit when an in-flight throttled call stalled
  // the page indefinitely.
  async function load() {
    loading = true;
    error = '';
    try {
      const view = await withIpcTimeout(invoke<SubscribedTitlesView>('get_favorites'));
      titles = view.titles ?? [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<svelte:head>
  <title>Library — MANGA+</title>
</svelte:head>

<div class="library">
  {#if loading}
    <div class="spinner"></div>
  {:else if error}
    <div class="empty-state">
      <p>Failed to load favorites: {error}</p>
      <p><button class="retry-btn" onclick={load}>↻ Retry</button></p>
    </div>
  {:else if titles.length === 0}
    <div class="empty-state">
      <p>No favorites yet. Use search to add some.</p>
      <p><a href="/search">Browse the catalog →</a></p>
    </div>
  {:else}
    <div class="title-grid">
      {#each titles as title (title.titleId)}
        <TitleCard
          {title}
          href="/title/{title.titleId}?lang={langCode(title.language)}"
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .library {
    padding: 8px 0;
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
</style>
