<script lang="ts" module>
  import type { Toast as ToastData } from '$lib/types';

  let toasts = $state<ToastData[]>([]);

  export function addToast(type: ToastData['type'], message: string, duration = 4000) {
    const id = crypto.randomUUID();
    toasts.push({ id, type, message, duration });

    if (duration > 0) {
      setTimeout(() => removeToast(id), duration);
    }

    return id;
  }

  export function removeToast(id: string) {
    toasts = toasts.filter((t) => t.id !== id);
  }

  export function toast(message: string) {
    return addToast('info', message);
  }

  toast.success = (msg: string) => addToast('success', msg);
  toast.error = (msg: string) => addToast('error', msg, 6000);
  toast.warning = (msg: string) => addToast('warning', msg, 5000);
</script>

<script lang="ts">
  const iconPaths: Record<string, string> = {
    success: 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z',
    error: 'M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z',
    warning: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z',
    info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'
  };
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as t (t.id)}
      <div class="toast toast-{t.type}" role="alert">
        <svg class="toast-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d={iconPaths[t.type]} />
        </svg>
        <span class="toast-message">{t.message}</span>
        <button class="toast-close" onclick={() => removeToast(t.id)} aria-label="Dismiss">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    bottom: 20px;
    right: 20px;
    display: flex;
    flex-direction: column-reverse;
    gap: 8px;
    z-index: 300;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    color: var(--text-primary);
    pointer-events: all;
    animation: toastSlideIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    max-width: 380px;
  }

  .toast-success { border-left: 3px solid var(--success); }
  .toast-error { border-left: 3px solid var(--error); }
  .toast-warning { border-left: 3px solid var(--warning); }
  .toast-info { border-left: 3px solid var(--accent); }

  .toast-success .toast-icon { color: var(--success); }
  .toast-error .toast-icon { color: var(--error); }
  .toast-warning .toast-icon { color: var(--warning); }
  .toast-info .toast-icon { color: var(--accent); }

  .toast-icon {
    flex-shrink: 0;
  }

  .toast-message {
    flex: 1;
    line-height: 1.4;
  }

  .toast-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: all var(--transition-fast);
  }

  .toast-close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-primary);
  }

  @keyframes toastSlideIn {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
