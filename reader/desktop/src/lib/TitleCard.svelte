<script lang="ts">
  import type { Title } from '$lib/types';
  import { proxied } from '$lib/img';
  import { languageBadge } from '$lib/lang';
  import type { Snippet } from 'svelte';

  let {
    title,
    href,
    action,
  }: {
    title: Title;
    href?: string;
    action?: Snippet;
  } = $props();

  let linkHref = $derived(href ?? `/title/${title.titleId}`);
  let badge = $derived(languageBadge(title.language));
</script>

<!--
  The anchor and the action snippet are siblings, not parent/child. Putting
  a <button> inside an <a> is invalid HTML and WebKit's click handling for
  that combination is unreliable — preventDefault on the inner button
  sometimes still navigates the outer anchor. Separate elements make event
  propagation trivial and the markup valid.
-->
<div class="title-card-wrapper">
  <a class="title-card" href={linkHref}>
    <div class="cover-wrap">
      <img src={proxied(title.portraitImageUrl)} alt={title.name} loading="lazy" />
      {#if badge}
        <span class="content-language-badge">{badge}</span>
      {/if}
    </div>
    <div class="card-info">
      <div class="card-name">{title.name}</div>
      <div class="card-author">{title.author}</div>
    </div>
  </a>
  {#if action}
    <div class="card-action">{@render action()}</div>
  {/if}
</div>

<style>
  .title-card-wrapper {
    display: flex;
    flex-direction: column;
  }

  .cover-wrap {
    position: relative;
  }

  .content-language-badge {
    position: absolute;
    right: 7px;
    bottom: 7px;
    padding: 3px 7px;
    border: 1px solid rgba(255, 255, 255, 0.65);
    border-radius: 4px;
    background: rgba(16, 16, 16, 0.88);
    box-shadow: 0 1px 5px rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 0.65rem;
    font-weight: 800;
    letter-spacing: 0.05em;
    line-height: 1;
    pointer-events: none;
  }

  .card-action {
    /* The action sits flush under the anchor and shares its width so it
       visually reads as part of the same card. */
    padding: 0 8px 8px;
    background: var(--bg-card);
    border-radius: 0 0 6px 6px;
    margin-top: -1px; /* close the seam against the anchor's bottom edge */
  }
</style>
