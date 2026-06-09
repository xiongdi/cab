<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    padding = '20px',
    hover = false,
    glow = false,
    children,
  }: {
    padding?: string;
    hover?: boolean;
    glow?: boolean;
    children: Snippet;
  } = $props();
</script>

<div class="card" class:card-hover={hover} class:card-glow={glow} style:padding>
  {@render children()}
</div>

<style>
  .card {
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    transition: all var(--transition-normal);
  }

  .card-hover:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-hover);
    box-shadow: var(--shadow-md);
  }

  .card-glow {
    box-shadow: var(--shadow-glow);
  }

  .card-glow:hover {
    box-shadow: var(--shadow-glow), var(--shadow-md);
  }
</style>
