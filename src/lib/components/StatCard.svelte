<script lang="ts">
  let {
    icon,
    value,
    label,
    trend,
    color = 'default',
  }: {
    icon: string;
    value: string | number;
    label: string;
    trend?: string;
    color?: 'default' | 'blue' | 'green' | 'purple' | 'amber';
  } = $props();

  const colorMap: Record<string, string> = {
    default: 'var(--badge-neutral-bg)',
    blue: 'var(--icon-blue-bg)',
    green: 'var(--icon-green-bg)',
    purple: 'var(--icon-purple-bg)',
    amber: 'var(--icon-amber-bg)',
  };

  const iconColorMap: Record<string, string> = {
    default: 'var(--text-secondary)',
    blue: 'var(--chart-blue)',
    green: 'var(--chart-green)',
    purple: 'var(--chart-purple)',
    amber: 'var(--chart-amber)',
  };
</script>

<div class="stat-card">
  <div class="stat-icon" style:background={colorMap[color]} style:color={iconColorMap[color]}>
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d={icon} />
    </svg>
  </div>
  <div class="stat-content">
    <span class="stat-value">{value}</span>
    <span class="stat-label">{label}</span>
  </div>
  {#if trend}
    <span class="stat-trend">{trend}</span>
  {/if}
</div>

<style>
  .stat-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    transition: all var(--transition-normal);
    position: relative;
    overflow: hidden;
  }

  .stat-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--gradient-shine);
  }

  .stat-card:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-md);
  }

  .stat-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    flex-shrink: 0;
  }

  .stat-content {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .stat-value {
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .stat-label {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .stat-trend {
    margin-left: auto;
    font-size: 12px;
    font-weight: 500;
    color: var(--success);
    font-family: var(--font-mono);
  }
</style>
