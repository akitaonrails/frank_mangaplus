<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { SubscribedTitlesView, Title } from '$lib/types';
  import { img } from '$lib/img';

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

  function langCode(lang: number): string {
    const map: Record<number, string> = { 0: 'eng', 1: 'esp', 2: 'fra', 3: 'por', 4: 'rus', 5: 'ind' };
    return map[lang] ?? 'eng';
  }
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
        <a
          class="title-card"
          href="/title/{title.titleId}?lang={langCode(title.language)}"
        >
          <img
            src={img(title.portraitImageUrl)}
            alt={title.name}
            loading="lazy"
          />
          <div class="card-info">
            <div class="card-name">{title.name}</div>
            <div class="card-author">{title.author}</div>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .library {
    padding: 8px 0;
  }
</style>
