<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  let configured = $state<boolean | null>(null);
  let value = $state('');
  let saving = $state(false);
  let error = $state('');

  async function check() {
    try {
      configured = await invoke<boolean>('is_configured');
    } catch (e) {
      configured = false;
      console.warn('is_configured failed', e);
    }
  }

  async function save() {
    if (!value.trim()) return;
    saving = true;
    error = '';
    try {
      await invoke<void>('set_secret', { value: value.trim() });
      configured = true;
      value = '';
      // Reload so any in-flight components re-fetch with the new client.
      location.reload();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    void check();
  });
</script>

{#if configured === false}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="setup-title">
    <div class="modal">
      <h2 id="setup-title">Couldn't auto-register</h2>
      <p>
        FRANK MANGA+ normally registers itself as a free-tier device on first launch, but the
        call to the official <code>/register</code> endpoint didn't succeed. Most likely a
        transient network issue — close this dialog and relaunch to retry.
      </p>
      <p>
        Or paste a <strong>deviceSecret</strong> below to use a subscriber session you've
        already extracted from a paid phone install. The full walkthrough is in
        <a href="https://github.com/akitaonrails/frank_mangaplus/blob/main/docs/android-secret.md" target="_blank" rel="noreferrer">
          docs/android-secret.md
        </a>.
      </p>

      <label class="field">
        <span>Paste your deviceSecret here</span>
        <input
          type="password"
          placeholder="32-char hex value"
          bind:value
          disabled={saving}
          autocomplete="off"
        />
      </label>

      {#if error}
        <p class="error">Save failed: {error}</p>
      {/if}

      <div class="actions">
        <button class="primary" onclick={save} disabled={saving || !value.trim()}>
          {saving ? 'Saving…' : 'Save & reload'}
        </button>
      </div>

      <p class="muted">
        Stored on disk at <code>~/.config/mangaplus-reader/secret</code>
        (or <code>%APPDATA%\mangaplus-reader\secret</code> on Windows). Treat it like a password —
        anyone with this value can read everything the session unlocks.
      </p>
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
  }

  .modal {
    width: 100%;
    max-width: 520px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 24px 28px;
    color: var(--text);
    font-size: 0.92rem;
    line-height: 1.55;
  }

  .modal h2 {
    font-size: 1.15rem;
    margin-bottom: 12px;
    color: var(--accent);
  }

  .modal p {
    margin-bottom: 12px;
  }

  .modal a {
    color: var(--accent);
    text-decoration: underline;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 16px 0 8px;
  }

  .field span {
    font-size: 0.8rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .field input {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 9px 12px;
    font-size: 0.95rem;
    font-family: ui-monospace, monospace;
    color: var(--text);
    outline: none;
  }

  .field input:focus {
    border-color: var(--accent);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin: 12px 0;
  }

  .primary {
    background: var(--accent);
    border: none;
    color: #fff;
    padding: 8px 18px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 0.9rem;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error {
    color: #ef5350;
    font-size: 0.85rem;
  }

  .muted {
    font-size: 0.78rem;
    color: var(--text-muted);
    margin-top: 12px;
    line-height: 1.5;
  }

  .muted code {
    background: var(--bg-elevated);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 0.85em;
  }
</style>
