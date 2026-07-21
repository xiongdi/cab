<script lang="ts">
  import { i18n } from '$lib/i18n.svelte';

  let {
    status,
    label,
  }: {
    status: 'active' | 'inactive' | 'error' | 'warning';
    label?: string;
  } = $props();

  const config = $derived.by(() => {
    void i18n.currentLang;
    return {
      active: { class: 'badge-success', text: i18n.t('common.active') },
      inactive: { class: 'badge-neutral', text: i18n.t('common.inactive') },
      error: { class: 'badge-error', text: i18n.t('common.error') },
      warning: { class: 'badge-warning', text: i18n.t('common.warning') },
    } as Record<string, { class: string; text: string }>;
  });

  const dotClass: Record<string, string> = {
    active: 'dot-active',
    inactive: 'dot-inactive',
    error: 'dot-error',
    warning: 'dot-active',
  };
</script>

<span class="badge {config[status].class}">
  <span class="dot {dotClass[status]}"></span>
  {label ?? config[status].text}
</span>
