<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { SubscribedTitlesView, Title } from '$lib/types';
  import TitleCard from '$lib/TitleCard.svelte';
  import { langCode } from '$lib/lang';

  let loading = $state(true);
  let error = $state('');
  let titles: Title[] = $state([]);

  onMount(async () => {
    try {
      const view = await invoke<SubscribedTitlesView>('get_favorites');
      titles = view.titles ?? [];
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
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
</style>
