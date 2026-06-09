<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    description,
    children,
  }: {
    title: string;
    description?: string;
    children?: Snippet;
  } = $props();
</script>

<header class="page-header">
  <div class="header-text">
    <h1 class="header-title">{title}</h1>
    {#if description}
      <p class="header-description">{description}</p>
    {/if}
  </div>
  {#if children}
    <div class="header-actions">
      {@render children()}
    </div>
  {/if}
</header>

<style>
  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 28px;
  }

  .header-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .header-title {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--text-primary);
  }

  .header-description {
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
</style>
