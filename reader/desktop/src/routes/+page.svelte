<script lang="ts">
  import { onMount } from 'svelte';
  import type { Title } from '$lib/types';
  import TitleCard from '$lib/TitleCard.svelte';
  import { DEFAULT_COUNTRY, langCode } from '$lib/lang';
  import { withIpcTimeout } from '$lib/ipcTimeout';
  import { getFavorites, getSubscription } from '$lib/ipcCommands';
  import {
    subscriptionWarning,
    rememberPlan,
    getBestSeenPlan,
    shouldCheckNow,
    markCheckedNow,
    isWarningDismissed,
    dismissWarning,
    clearDismissedWarning,
    type PlanWarning,
  } from '$lib/subscription';

  let loading = $state(true);
  let error = $state('');
  let titles: Title[] = $state([]);

  // Subscription plan-drop warning. Populated by the throttled
  // background check below; null = all good (or not checked yet).
  let planWarning: PlanWarning | null = $state(null);

  // Best-effort and silent on failure: a network error here must never
  // affect the library view. Throttled via localStorage so app
  // restarts within the interval don't re-hit the API.
  async function checkSubscription() {
    const nowSecs = Math.floor(Date.now() / 1000);
    if (!shouldCheckNow(nowSecs)) return;
    try {
      const view = await withIpcTimeout(getSubscription(DEFAULT_COUNTRY));
      markCheckedNow(nowSecs);
      const sub = view.userSubscription;
      const plan = sub?.planType ?? '';
      const warning = subscriptionWarning(
        plan,
        getBestSeenPlan(),
        sub?.nextPaymentDate ?? 0,
        nowSecs,
      );
      rememberPlan(plan);
      if (warning == null) {
        clearDismissedWarning();
      } else if (!isWarningDismissed(warning)) {
        planWarning = warning;
      }
    } catch (e) {
      console.warn('[library] subscription check failed:', e);
    }
  }

  function onDismissWarning() {
    if (planWarning) dismissWarning(planWarning);
    planWarning = null;
  }

  // Fetch races against a generous timeout so a hung IPC call surfaces
  // as a retry-able error instead of an infinite spinner — that was the
  // failure mode the user hit when an in-flight throttled call stalled
  // the page indefinitely.
  async function load() {
    loading = true;
    error = '';
    try {
      const view = await withIpcTimeout(getFavorites());
      titles = view.titles ?? [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    void checkSubscription();
  });
</script>

<svelte:head>
  <title>Library — MANGA+</title>
</svelte:head>

<div class="library">
  {#if planWarning}
    <div class="sub-warning" role="alert">
      <span class="sub-warning-icon" aria-hidden="true">⚠️</span>
      <div class="sub-warning-text">
        {#if planWarning.kind === 'dropped'}
          <strong>Your MANGA Plus plan dropped from {planWarning.from} to {planWarning.to}.</strong>
          Subscription-locked chapters will stop loading. Open the official
          app in the rooted emulator and tap <em>Restore subscription</em>
          — see <code>reader/docs/android-secret.md</code> for the steps.
        {:else}
          <strong>Your {planWarning.plan} plan's billing period ended on
          {new Date(planWarning.nextPaymentDate * 1000).toLocaleDateString()}
          and hasn't renewed server-side.</strong>
          If chapters stop loading, restore the subscription in the
          official app via the rooted emulator.
        {/if}
      </div>
      <button class="sub-warning-dismiss" onclick={onDismissWarning} aria-label="Dismiss warning">✕</button>
    </div>
  {/if}
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

  /* Subscription plan-drop banner: amber "caution", consistent with
     the reader's locked-chapter surfaces. */
  .sub-warning {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin: 0 0 14px;
    padding: 12px 14px;
    background: rgba(246, 193, 119, 0.08);
    border: 1px solid rgba(246, 193, 119, 0.45);
    border-radius: 8px;
    font-size: 0.88rem;
    line-height: 1.5;
  }

  .sub-warning-icon {
    font-size: 1.1rem;
    line-height: 1.4;
  }

  .sub-warning-text {
    flex: 1;
    color: var(--text);
  }

  .sub-warning-text code {
    font-size: 0.8rem;
    background: rgba(255, 255, 255, 0.07);
    padding: 1px 5px;
    border-radius: 4px;
  }

  .sub-warning-dismiss {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 0.9rem;
    padding: 2px 6px;
    border-radius: 4px;
    flex-shrink: 0;
    transition: color 0.15s;
  }

  .sub-warning-dismiss:hover {
    color: var(--text);
  }
</style>
