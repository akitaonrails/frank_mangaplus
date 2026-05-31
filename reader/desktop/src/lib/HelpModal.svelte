<script lang="ts">
  /**
   * Reader help modal. Lists every keybinding and the click-zone layout
   * so users can discover navigation without reading docs.
   *
   * Controlled via the `open` prop; the close button + Escape key call
   * the `onclose` callback (parent persists "seen" state and flips
   * `open` to false).
   */
  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    // Escape closes; '?' is a no-op when already open.
    if (e.key === 'Escape' || e.key === '?') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window on:keydown={onKey} />

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="help-title" onclick={onclose}>
    <div class="modal" role="document" onclick={(e) => e.stopPropagation()}>
      <header class="modal-header">
        <h2 id="help-title">Reader controls</h2>
        <button class="close-btn" aria-label="Close" onclick={onclose}>×</button>
      </header>

      <section class="block">
        <h3>Mouse / touch</h3>
        <dl>
          <dt>Click left half of the page</dt><dd>Advance (manga RTL direction)</dd>
          <dt>Click right half of the page</dt><dd>Go back one page</dd>
          <dt>Scroll wheel</dt><dd>Smooth vertical scrolling</dd>
        </dl>
      </section>

      <section class="block">
        <h3>Keyboard — navigate</h3>
        <dl>
          <dt><kbd>Space</kbd> <kbd>↓</kbd> <kbd>j</kbd> <kbd>PgDn</kbd></dt><dd>Forward (smooth scroll)</dd>
          <dt><kbd>↑</kbd> <kbd>k</kbd> <kbd>PgUp</kbd></dt><dd>Back (smooth scroll)</dd>
          <dt><kbd>←</kbd></dt><dd>Forward (manga page-flip animation)</dd>
          <dt><kbd>→</kbd></dt><dd>Back (manga page-flip animation)</dd>
          <dt><kbd>Home</kbd></dt><dd>Jump to first page of current chapter</dd>
          <dt><kbd>End</kbd></dt><dd>Jump to last page of current chapter</dd>
        </dl>
      </section>

      <section class="block">
        <h3>Keyboard — layout &amp; viewing</h3>
        <dl>
          <dt><kbd>D</kbd></dt><dd>Cycle layout: single → double → cover-offset double</dd>
          <dt><kbd>F</kbd></dt><dd>Cycle eye-protection filter: off → low → med → high</dd>
        </dl>
      </section>

      <section class="block">
        <h3>Keyboard — other</h3>
        <dl>
          <dt><kbd>?</kbd></dt><dd>Open this help</dd>
          <dt><kbd>Esc</kbd></dt><dd>Back to the title page</dd>
        </dl>
      </section>

      <footer class="modal-footer">
        <button class="primary" onclick={onclose}>Got it</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.78);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 24px;
    overflow-y: auto;
  }

  .modal {
    width: 100%;
    max-width: 540px;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 20px 24px;
    color: var(--text);
    font-size: 0.92rem;
    line-height: 1.55;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .modal-header h2 {
    font-size: 1.15rem;
    color: var(--accent);
    margin: 0;
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 1.4rem;
    line-height: 1;
    padding: 4px 8px;
    border-radius: 4px;
    transition: color 0.15s, background 0.15s;
  }
  .close-btn:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.08);
  }

  .block {
    margin: 12px 0 4px;
  }

  .block h3 {
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin: 0 0 6px;
    font-weight: 700;
  }

  dl {
    display: grid;
    grid-template-columns: minmax(150px, max-content) 1fr;
    gap: 4px 16px;
    margin: 0;
  }

  dt {
    color: var(--text);
    font-size: 0.88rem;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  dd {
    color: var(--text-muted);
    font-size: 0.88rem;
    margin: 0;
  }

  kbd {
    display: inline-block;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    padding: 1px 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
    color: var(--text);
    white-space: nowrap;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }

  .primary {
    background: var(--accent);
    border: none;
    color: #fff;
    padding: 8px 18px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 0.9rem;
    transition: background 0.15s;
  }
  .primary:hover {
    background: var(--accent-hover);
  }
</style>
