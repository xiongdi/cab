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

<PageHeader title="Usage" description="Token consumption and cost tracking" />

<div class="usage-page">
  <div class="range-selector">
    {#each ranges as r}
      <button class="range-btn" class:active={range === r.value} onclick={() => (range = r.value)}>
        {r.label}
      </button>
    {/each}
  </div>

  {#if loading}
    <div class="loading">Loading usage data...</div>
  {:else if summary}
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-label">Total Requests</div>
        <div class="stat-value">{fmt(summary.total_requests)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Input Tokens</div>
        <div class="stat-value">{fmt(summary.total_input_tokens)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Output Tokens</div>
        <div class="stat-value">{fmt(summary.total_output_tokens)}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Total Cost</div>
        <div class="stat-value">{fmtCost(summary.total_cost_usd)}</div>
      </div>
    </div>

    <div class="breakdown-grid">
      <div class="breakdown-section">
        <h3>By Provider</h3>
        <table class="breakdown-table">
          <thead>
            <tr><th>Provider</th><th>Requests</th><th>Tokens</th><th>Cost</th></tr>
          </thead>
          <tbody>
            {#each providerRows as row}
              <tr>
                <td>{row.id}</td>
                <td>{fmt(row.requests)}</td>
                <td>{fmt(row.input_tokens + row.output_tokens)}</td>
                <td>{fmtCost(row.cost_usd)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="breakdown-section">
        <h3>By Model</h3>
        <table class="breakdown-table">
          <thead>
            <tr><th>Model</th><th>Requests</th><th>Tokens</th><th>Cost</th></tr>
          </thead>
          <tbody>
            {#each modelRows as row}
              <tr>
                <td>{row.id}</td>
                <td>{fmt(row.requests)}</td>
                <td>{fmt(row.input_tokens + row.output_tokens)}</td>
                <td>{fmtCost(row.cost_usd)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="breakdown-section">
        <h3>By Agent</h3>
        <table class="breakdown-table">
          <thead>
            <tr><th>Agent</th><th>Requests</th><th>Tokens</th><th>Cost</th></tr>
          </thead>
          <tbody>
            {#each agentRows as row}
              <tr>
                <td>{row.id}</td>
                <td>{fmt(row.requests)}</td>
                <td>{fmt(row.input_tokens + row.output_tokens)}</td>
                <td>{fmtCost(row.cost_usd)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
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
  }

  .range-selector {
    display: flex;
    gap: 8px;
  }

  .range-btn {
    padding: 6px 16px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 13px;
    transition: all 0.15s;
  }

  .range-btn:hover {
    border-color: var(--accent);
  }

  .range-btn.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .stat-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 20px;
  }

  .stat-label {
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .stat-value {
    font-size: 24px;
    font-weight: 600;
    color: var(--text);
  }

  .breakdown-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
  }

  .breakdown-section h3 {
    font-size: 14px;
    margin-bottom: 12px;
    color: var(--text);
  }

  .breakdown-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .breakdown-table th {
    text-align: left;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    color: var(--text-secondary);
    font-weight: 500;
  }

  .breakdown-table td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .breakdown-table td:last-child {
    text-align: right;
  }

  .loading,
  .empty {
    text-align: center;
    padding: 48px;
    color: var(--text-secondary);
  }

  .records-section h3 {
    font-size: 14px;
    margin-bottom: 12px;
  }
</style>
