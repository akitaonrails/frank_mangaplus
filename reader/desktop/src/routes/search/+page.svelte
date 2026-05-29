<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { SearchView, Title } from '$lib/types';

  let loading = $state(true);
  let error = $state('');
  let allTitles: Title[] = $state([]);
  let query = $state('');
  // Map of titleId → transient confirmation message
  let confirmMap = $state<Map<number, string>>(new Map());

  let filtered = $derived(
    query.trim() === ''
      ? allTitles
      : allTitles.filter(t =>
          t.name.toLowerCase().includes(query.trim().toLowerCase()) ||
          t.author.toLowerCase().includes(query.trim().toLowerCase())
        )
  );

  onMount(async () => {
    try {
      const view = await invoke<SearchView>('search', { lang: 'eng', clang: 'eng' });
      const seen = new Set<number>();
      const flat: Title[] = [];
      for (const content of view.contents ?? []) {
        for (const t of content.titleList?.featuredTitles ?? []) {
          if (!seen.has(t.titleId)) {
            seen.add(t.titleId);
            flat.push(t);
          }
        }
      }
      allTitles = flat;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function addFavorite(title: Title, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await invoke<void>('add_favorite', { titleId: title.titleId });
      confirmMap = new Map(confirmMap).set(title.titleId, 'Added!');
      setTimeout(() => {
        confirmMap = new Map(confirmMap);
        confirmMap.delete(title.titleId);
        confirmMap = new Map(confirmMap);
      }, 2000);
    } catch (e) {
      confirmMap = new Map(confirmMap).set(title.titleId, 'Error');
      setTimeout(() => {
        confirmMap = new Map(confirmMap);
        confirmMap.delete(title.titleId);
        confirmMap = new Map(confirmMap);
      }, 2000);
    }
  }
</script>

<svelte:head>
  <title>Search — MANGA+</title>
</svelte:head>

<div class="search-page">
  <div class="search-bar-wrap">
    <input
      class="search-input"
      type="search"
      placeholder="Search titles or authors…"
      bind:value={query}
      autofocus
    />
    {#if !loading}
      <span class="result-count">{filtered.length} title{filtered.length !== 1 ? 's' : ''}</span>
    {/if}
  </div>

  {#if loading}
    <div class="spinner"></div>
  {:else if error}
    <div class="empty-state"><p>Error: {error}</p></div>
  {:else if filtered.length === 0}
    <div class="empty-state"><p>No titles match "{query}".</p></div>
  {:else}
    <div class="title-grid">
      {#each filtered as title (title.titleId)}
        <a class="title-card" href="/title/{title.titleId}">
          <img src={title.portraitImageUrl} alt={title.name} loading="lazy" />
          <div class="card-info">
            <div class="card-name">{title.name}</div>
            <div class="card-author">{title.author}</div>
            <button
              class="fav-btn"
              class:confirmed={confirmMap.has(title.titleId)}
              onclick={(e) => addFavorite(title, e)}
            >
              {confirmMap.get(title.titleId) ?? '+ Library'}
            </button>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .search-page {
    display: flex;
    flex-direction: column;
  }

  .search-bar-wrap {
    position: sticky;
    top: var(--header-h);
    z-index: 10;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .search-input {
    flex: 1;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 14px;
    font-size: 1rem;
    color: var(--text);
    outline: none;
    transition: border-color 0.15s;
  }

  .search-input:focus {
    border-color: var(--accent);
  }

  .result-count {
    font-size: 0.8rem;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .fav-btn {
    margin-top: 6px;
    width: 100%;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
    font-size: 0.72rem;
    padding: 4px 6px;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .fav-btn:hover {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .fav-btn.confirmed {
    background: #2e7d32;
    border-color: #2e7d32;
    color: #fff;
  }
</style>
