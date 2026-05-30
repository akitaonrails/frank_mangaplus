<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import SecretSetup from '$lib/SecretSetup.svelte';

  let { children } = $props();

  // Derive whether we're in the reader (hide main header there)
  let inReader = $derived($page.url.pathname.startsWith('/reader/'));

  const REPO_URL = 'https://github.com/akitaonrails/frank_mangaplus';

  function openRepo(e: MouseEvent) {
    e.preventDefault();
    // Route through Tauri so the WebView doesn't try to navigate.
    void openUrl(REPO_URL);
  }
</script>

<SecretSetup />

{#if !inReader}
  <header class="app-header">
    <a href="/" class="brand" aria-label="FRANK MANGA+">
      <img src="/logo.png" alt="FRANK MANGA+" />
    </a>
    <nav>
      <a href="/" class:active={$page.url.pathname === '/'}>Library</a>
      <a href="/search" class:active={$page.url.pathname === '/search'}>Search</a>
    </nav>
    <a
      href={REPO_URL}
      class="github-link"
      title="Source on GitHub"
      aria-label="Source on GitHub"
      onclick={openRepo}
    >
      <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
        <path
          fill="currentColor"
          d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.28-.01-1.02-.02-2-3.2.69-3.87-1.54-3.87-1.54-.53-1.34-1.29-1.7-1.29-1.7-1.05-.72.08-.71.08-.71 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.71 1.26 3.37.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.7 0-1.26.45-2.29 1.19-3.1-.12-.29-.51-1.47.11-3.07 0 0 .97-.31 3.18 1.18a11.06 11.06 0 0 1 5.78 0c2.21-1.49 3.18-1.18 3.18-1.18.62 1.6.23 2.78.11 3.07.74.81 1.19 1.84 1.19 3.1 0 4.43-2.69 5.41-5.25 5.69.41.36.78 1.06.78 2.14 0 1.55-.01 2.8-.01 3.18 0 .31.21.68.8.56C20.21 21.38 23.5 17.07 23.5 12 23.5 5.65 18.35.5 12 .5z"
        />
      </svg>
    </a>
  </header>
{/if}

<div class="page-shell" class:full-height={inReader}>
  {@render children()}
</div>

<style>
  .app-header {
    position: sticky;
    top: 0;
    z-index: 100;
    height: var(--header-h);
    background: #111;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 24px;
    padding: 0 20px;
    overflow: hidden;
  }

  .brand {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    height: 100%;
    padding: 4px 0;
  }

  .brand img {
    height: 100%;
    width: auto;
    display: block;
    border-radius: 4px;
  }

  nav {
    flex-shrink: 0;
  }

  nav {
    display: flex;
    gap: 16px;
  }

  nav a {
    font-size: 0.9rem;
    color: var(--text-muted);
    transition: color 0.15s;
    padding: 4px 0;
    border-bottom: 2px solid transparent;
  }

  nav a:hover,
  nav a.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  .github-link {
    margin-left: auto;
    display: flex;
    align-items: center;
    color: var(--text-muted);
    transition: color 0.15s;
    flex-shrink: 0;
  }

  .github-link:hover {
    color: var(--text);
  }

  .page-shell {
    min-height: calc(100vh - var(--header-h));
  }

  .full-height {
    min-height: 100vh;
  }
</style>
