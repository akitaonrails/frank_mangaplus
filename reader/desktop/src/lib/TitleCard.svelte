<script lang="ts">
  import type { Title } from '$lib/types';
  import { proxied } from '$lib/img';
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
</script>

<a class="title-card" href={linkHref}>
  <img src={proxied(title.portraitImageUrl)} alt={title.name} loading="lazy" />
  <div class="card-info">
    <div class="card-name">{title.name}</div>
    <div class="card-author">{title.author}</div>
    {#if action}{@render action()}{/if}
  </div>
</a>
