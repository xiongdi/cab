<script lang="ts">
  let {
    status,
    label
  }: {
    status: 'active' | 'inactive' | 'error' | 'warning';
    label?: string;
  } = $props();

  const config: Record<string, { class: string; text: string }> = {
    active: { class: 'badge-success', text: 'Active' },
    inactive: { class: 'badge-neutral', text: 'Inactive' },
    error: { class: 'badge-error', text: 'Error' },
    warning: { class: 'badge-warning', text: 'Warning' }
  };

  const dotClass: Record<string, string> = {
    active: 'dot-active',
    inactive: 'dot-inactive',
    error: 'dot-error',
    warning: 'dot-active'
  };
</script>

<span class="badge {config[status].class}">
  <span class="dot {dotClass[status]}"></span>
  {label ?? config[status].text}
</span>
