<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open = $bindable(false),
    title = '',
    width = '480px',
    children,
    onclose,
  }: {
    open: boolean;
    title?: string;
    width?: string;
    children: Snippet;
    onclose?: () => void;
  } = $props();

  function handleBackdrop() {
    open = false;
    onclose?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
      onclose?.();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="modal-overlay" onclick={handleBackdrop} role="presentation">
    <div
      class="modal"
      style:max-width={width}
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div class="modal-header">
        <h2 class="modal-title">{title}</h2>
        <button class="modal-close" onclick={handleBackdrop} aria-label="Close">
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
      <div class="modal-body">
        {@render children()}
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    animation: overlayFadeIn 0.15s ease-out;
    padding: 20px;
  }

  .modal {
    width: 100%;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow:
      var(--shadow-lg),
      0 0 60px rgba(0, 0, 0, 0.4);
    animation: modalSlideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--border);
  }

  .modal-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.01em;
  }

  .modal-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .modal-close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 20px 24px 24px;
  }

  @keyframes overlayFadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes modalSlideIn {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }
</style>
