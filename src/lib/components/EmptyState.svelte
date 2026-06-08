<script lang="ts">
  let {
    message = 'No items yet',
    icon = 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4',
    actionLabel,
    onaction
  }: {
    message?: string;
    icon?: string;
    actionLabel?: string;
    onaction?: () => void;
  } = $props();
</script>

<div class="empty-state">
  <div class="empty-icon">
    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round">
      <path d={icon} />
    </svg>
  </div>
  <p class="empty-message">{message}</p>
  {#if actionLabel && onaction}
    <button class="btn btn-secondary btn-sm" onclick={onaction}>
      {actionLabel}
    </button>
  {/if}
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px 24px;
    text-align: center;
  }

  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: var(--radius-lg);
    background: rgba(255, 255, 255, 0.03);
    color: var(--text-muted);
    margin-bottom: 4px;
  }

  .empty-message {
    font-size: 13px;
    color: var(--text-muted);
    max-width: 240px;
    line-height: 1.5;
  }
</style>
