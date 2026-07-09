<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { UsageSummary, UsageRecord, Column } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import DataTable from '$lib/components/DataTable.svelte';
  import { i18n } from '$lib/i18n.svelte';

  let summary = $state<UsageSummary | null>(null);
  let records = $state<UsageRecord[]>([]);
  let loading = $state(true);
  let range = $state('month');

  const ranges = [
    { value: 'day', label: '24h' },
    { value: 'week', label: '7d' },
    { value: 'month', label: '30d' },
  ];

  async function loadData() {
    loading = true;
    try {
      const [s, r] = await Promise.all([
        api.usage.getSummary(range),
        api.usage.getRecords(range, 100),
      ]);
      summary = s;
      records = r.data;
    } catch (e) {
      console.error('Failed to load usage:', e);
    } finally {
      loading = false;
    }
  }

  onMount(loadData);

  $effect(() => {
    void range;
    loadData();
  });

  const columns: Column[] = [
    {
      key: 'timestamp',
      label: 'Time',
      sortable: true,
      render: (v: string) => {
        try {
          const d = new Date(v);
          return `<span class="mono" style="font-size:11px">${d.toLocaleDateString()} ${d.toLocaleTimeString()}</span>`;
        } catch {
          return v;
        }
      },
    },
    { key: 'agent_id', label: 'Agent', sortable: true },
    { key: 'provider_id', label: 'Provider', sortable: true },
    { key: 'model_id', label: 'Model', sortable: true },
    {
      key: 'input_tokens',
      label: 'Input',
      sortable: true,
      render: (v: number) => v.toLocaleString(),
    },
    {
      key: 'output_tokens',
      label: 'Output',
      sortable: true,
      render: (v: number) => v.toLocaleString(),
    },
    {
      key: 'cost_usd',
      label: 'Cost (USD)',
      sortable: true,
      render: (v: number) => `$${v.toFixed(4)}`,
    },
  ];

  function fmt(n: number): string {
    return n.toLocaleString();
  }

  function fmtCost(n: number): string {
    return `$${n.toFixed(4)}`;
  }

  let providerRows = $derived(
    summary
      ? Object.entries(summary.by_provider)
          .map(([id, s]) => ({ id, ...s }))
          .sort((a, b) => b.cost_usd - a.cost_usd)
      : [],
  );

  let modelRows = $derived(
    summary
      ? Object.entries(summary.by_model)
          .map(([id, s]) => ({ id, ...s }))
          .sort((a, b) => b.cost_usd - a.cost_usd)
      : [],
  );

  let agentRows = $derived(
    summary
      ? Object.entries(summary.by_agent)
          .map(([id, s]) => ({ id, ...s }))
          .sort((a, b) => b.requests - a.requests)
      : [],
  );
</script>

<PageHeader title={i18n.t('usage.title') || 'Usage'} description={i18n.t('usage.subtitle') || 'Token consumption and cost tracking'} />

<div class="usage-page">
  <div class="range-selector">
    {#each ranges as r}
      <button class="range-btn" class:active={range === r.value} onclick={() => (range = r.value)}>
        {r.label}
      </button>
    {/each}
  </div>

  {#if loading}
    <div class="loading">
      <svg class="spin" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /></svg>
      &nbsp;Loading usage data...
    </div>
  {:else if summary}
    <!-- Glow stat cards -->
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-label">Total Requests</div>
        <div class="stat-value mono">{fmt(summary.total_requests)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Input Tokens</div>
        <div class="stat-value mono">{fmt(summary.total_input_tokens)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Output Tokens</div>
        <div class="stat-value mono">{fmt(summary.total_output_tokens)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Total Cost</div>
        <div class="stat-value mono text-white">{fmtCost(summary.total_cost_usd)}</div>
      </div>
    </div>

    <!-- Visual Progress breakdowns -->
    <div class="breakdown-grid">
      <div class="breakdown-section">
        <h3>By Provider</h3>
        <div class="breakdown-list">
          {#each providerRows as row}
            {@const pct = summary.total_cost_usd > 0 ? (row.cost_usd / summary.total_cost_usd) * 100 : 0}
            <div class="progress-item">
              <div class="progress-meta">
                <span class="progress-name">{row.id}</span>
                <span class="progress-val mono">{fmtCost(row.cost_usd)}</span>
              </div>
              <div class="progress-bar-track">
                <div class="progress-bar-fill" style="width: {Math.max(2, Math.round(pct))}%"></div>
              </div>
              <div class="progress-subinfo">
                <span>{fmt(row.requests)} reqs</span>
                <span>{fmt(row.input_tokens + row.output_tokens)} tokens</span>
              </div>
            </div>
          {:else}
            <div class="empty-list">No provider data</div>
          {/each}
        </div>
      </div>

      <div class="breakdown-section">
        <h3>By Model</h3>
        <div class="breakdown-list">
          {#each modelRows as row}
            {@const pct = summary.total_cost_usd > 0 ? (row.cost_usd / summary.total_cost_usd) * 100 : 0}
            <div class="progress-item">
              <div class="progress-meta">
                <span class="progress-name truncate" title={row.id}>{row.id}</span>
                <span class="progress-val mono">{fmtCost(row.cost_usd)}</span>
              </div>
              <div class="progress-bar-track">
                <div class="progress-bar-fill highlight" style="width: {Math.max(2, Math.round(pct))}%"></div>
              </div>
              <div class="progress-subinfo">
                <span>{fmt(row.requests)} reqs</span>
                <span>{fmt(row.input_tokens + row.output_tokens)} tokens</span>
              </div>
            </div>
          {:else}
            <div class="empty-list">No model data</div>
          {/each}
        </div>
      </div>

      <div class="breakdown-section">
        <h3>By Agent</h3>
        <div class="breakdown-list">
          {#each agentRows as row}
            {@const pct = summary.total_requests > 0 ? (row.requests / summary.total_requests) * 100 : 0}
            <div class="progress-item">
              <div class="progress-meta">
                <span class="progress-name">{row.id}</span>
                <span class="progress-val mono">{fmt(row.requests)} reqs</span>
              </div>
              <div class="progress-bar-track">
                <div class="progress-bar-fill" style="width: {Math.max(2, Math.round(pct))}%"></div>
              </div>
              <div class="progress-subinfo">
                <span>{fmt(row.input_tokens + row.output_tokens)} tokens</span>
                <span>{fmtCost(row.cost_usd)} cost</span>
              </div>
            </div>
          {:else}
            <div class="empty-list">No agent data</div>
          {/each}
        </div>
      </div>
    </div>

    <div class="records-section">
      <h3>Recent Records</h3>
      <DataTable {columns} data={records} />
    </div>
  {:else}
    <div class="empty">No usage data yet. Usage is recorded when requests are proxied through the gateway.</div>
  {/if}
</div>

<style>
  .usage-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
    margin-top: 4px;
  }

  .range-selector {
    display: flex;
    gap: 6px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 3px;
    align-self: flex-start;
  }

  .range-btn {
    padding: 6px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 11.5px;
    font-weight: 550;
    transition: all var(--transition-fast);
  }

  .range-btn:hover:not(.active) {
    color: var(--text-secondary);
    background: rgba(255, 255, 255, 0.02);
  }

  .range-btn.active {
    background: #ffffff;
    color: #030303;
    font-weight: 600;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .stat-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
    box-shadow: var(--shadow-sm);
    transition: all var(--transition-normal);
  }

  .stat-card:hover {
    border-color: var(--border-hover);
    box-shadow: var(--shadow-glow);
  }

  .stat-label {
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
  }

  .stat-value {
    font-size: 24px;
    font-weight: 650;
    color: var(--text-primary);
  }

  .text-white {
    color: #ffffff;
  }

  /* ── Breakdown Visual Lists ─────────────────────────── */
  .breakdown-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 20px;
  }

  .breakdown-section {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
    box-shadow: var(--shadow-sm);
  }

  .breakdown-section h3 {
    font-size: 13.5px;
    font-weight: 600;
    margin: 0 0 16px 0;
    color: var(--text-primary);
    border-bottom: 1px dashed var(--border);
    padding-bottom: 12px;
  }

  .breakdown-list {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .progress-item {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .progress-meta {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
  }

  .progress-name {
    font-size: 12.5px;
    font-weight: 550;
    color: var(--text-primary);
  }

  .progress-val {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .progress-bar-track {
    height: 4px;
    background: var(--border-dashed-subtle);
    border-radius: var(--radius-full);
    overflow: hidden;
    width: 100%;
  }

  .progress-bar-fill {
    height: 100%;
    background: var(--text-muted);
    border-radius: var(--radius-full);
    transition: width var(--transition-slow);
  }

  .progress-bar-fill.highlight {
    background: linear-gradient(90deg, #60a5fa, #3b82f6);
  }

  .progress-subinfo {
    display: flex;
    justify-content: space-between;
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .empty-list {
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
    padding: 24px;
  }

  .loading,
  .empty {
    text-align: center;
    padding: 48px;
    color: var(--text-secondary);
    font-size: 13px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .records-section {
    margin-top: 12px;
  }

  .records-section h3 {
    font-size: 13.5px;
    font-weight: 600;
    margin: 0 0 12px 0;
    color: var(--text-primary);
  }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
