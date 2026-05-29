<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';

  let { children } = $props();

  // Derive whether we're in the reader (hide main header there)
  let inReader = $derived($page.url.pathname.startsWith('/reader/'));
</script>

{#if !inReader}
  <header class="app-header">
    <a href="/" class="brand">MANGA+</a>
    <nav>
      <a href="/" class:active={$page.url.pathname === '/'}>Library</a>
      <a href="/search" class:active={$page.url.pathname === '/search'}>Search</a>
    </nav>
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
    align-items: center;
    gap: 24px;
    padding: 0 20px;
  }

  .brand {
    font-size: 1.1rem;
    font-weight: 800;
    letter-spacing: 0.05em;
    color: var(--accent);
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

  .page-shell {
    min-height: calc(100vh - var(--header-h));
  }

  .full-height {
    min-height: 100vh;
  }
</style>
