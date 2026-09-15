<script lang="ts">
  import { CONTENT_LANGUAGES } from '$lib/lang';
  import {
    contentLanguages,
    toggleContentLanguage,
  } from '$lib/contentLanguagePreference';
  import { contentLanguageSummary } from '$lib/contentLanguagePicker';

  // Multi-select dropdown: a trigger showing the current selection, and
  // a popover of checkbox rows. Replaces the horizontal pill row, which
  // overflowed the header and read as clutter. The store still owns the
  // "at least one language stays active" invariant, so the last-active
  // row renders disabled.
  let open = $state(false);
  let rootEl: HTMLElement | undefined = $state();

  let summary = $derived(contentLanguageSummary($contentLanguages));

  function toggleOpen() {
    open = !open;
  }

  function close() {
    open = false;
  }

  // Close on outside click / focus leaving the widget, and on Escape.
  function onWindowPointerDown(e: MouseEvent) {
    if (open && rootEl && !rootEl.contains(e.target as Node)) close();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      close();
      // Return focus to the trigger so keyboard users aren't stranded.
      (rootEl?.querySelector('.cl-trigger') as HTMLElement | undefined)?.focus();
    }
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} onkeydown={onKeydown} />

<div class="cl-picker" bind:this={rootEl}>
  <button
    type="button"
    class="cl-trigger"
    class:open
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label="Content languages: {summary}"
    onclick={toggleOpen}
  >
    <span class="cl-globe" aria-hidden="true">
      <svg viewBox="0 0 24 24" width="15" height="15">
        <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.8" />
        <path
          d="M3 12h18M12 3c2.5 2.5 2.5 15 0 18M12 3c-2.5 2.5-2.5 15 0 18"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
        />
      </svg>
    </span>
    <span class="cl-summary">{summary}</span>
    <span class="cl-caret" class:open aria-hidden="true">▾</span>
  </button>

  {#if open}
    <div class="cl-menu" role="listbox" aria-multiselectable="true">
      {#each CONTENT_LANGUAGES as language}
        {@const selected = $contentLanguages.includes(language.code)}
        {@const required = selected && $contentLanguages.length === 1}
        <button
          type="button"
          role="option"
          class="cl-option"
          class:selected
          aria-selected={selected}
          disabled={required}
          title={required ? `${language.label} is the only active content language` : undefined}
          onclick={() => toggleContentLanguage(language.code)}
        >
          <span class="cl-check" class:on={selected} aria-hidden="true">
            {selected ? '✓' : ''}
          </span>
          <span class="cl-badge">{language.badge}</span>
          <span class="cl-label">{language.label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .cl-picker {
    position: relative;
    flex-shrink: 0;
  }

  .cl-trigger {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 600;
    transition: border-color 0.15s, background 0.15s;
  }

  .cl-trigger:hover,
  .cl-trigger.open {
    border-color: var(--accent);
    background: rgba(255, 255, 255, 0.06);
  }

  .cl-globe {
    display: flex;
    color: var(--text-muted);
  }

  .cl-summary {
    white-space: nowrap;
  }

  .cl-caret {
    color: var(--text-muted);
    font-size: 0.7rem;
    transition: transform 0.15s;
  }
  .cl-caret.open {
    transform: rotate(180deg);
  }

  .cl-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 200;
    min-width: 220px;
    padding: 5px;
    background: var(--bg-card, #1a1a1a);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 34px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: min(70vh, 420px);
    overflow-y: auto;
  }

  .cl-option {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 0.82rem;
    text-align: left;
    width: 100%;
    transition: background 0.12s;
  }

  .cl-option:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.07);
  }

  .cl-option:disabled {
    cursor: default;
    opacity: 0.85;
  }

  .cl-check {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: #fff;
    font-size: 0.7rem;
    line-height: 1;
  }
  .cl-check.on {
    background: var(--accent);
    border-color: var(--accent);
  }

  .cl-badge {
    flex-shrink: 0;
    min-width: 42px;
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.03em;
    color: var(--text-muted);
  }
  .cl-option.selected .cl-badge {
    color: var(--text);
  }

  .cl-label {
    flex: 1;
    color: var(--text-muted);
  }
  .cl-option.selected .cl-label {
    color: var(--text);
  }
</style>
