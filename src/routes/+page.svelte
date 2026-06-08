<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DashboardStats, Column } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import StatCard from '$lib/components/StatCard.svelte';
  import Card from '$lib/components/Card.svelte';
  import DataTable from '$lib/components/DataTable.svelte';

  let stats = $state<DashboardStats | null>(null);
  let loading = $state(true);
  let error = $state('');

  const recentColumns: Column[] = [
    { key: 'timestamp', label: 'Time', sortable: true, render: (v: string) => {
      try { return new Date(v).toLocaleTimeString(); } catch { return v; }
    }},
    { key: 'agent', label: 'Agent', sortable: true },
    { key: 'provider', label: 'Provider', sortable: true },
    { key: 'model', label: 'Model', sortable: true },
    { key: 'total_tokens', label: 'Tokens', sortable: true, align: 'right' as const, render: (v: number) => v?.toLocaleString() ?? '0' },
    { key: 'latency_ms', label: 'Latency', sortable: true, align: 'right' as const, render: (v: number) => `${v}ms` },
    { key: 'status_code', label: 'Status', align: 'center' as const, render: (v: number) => {
      const cls = v < 300 ? 'badge-success' : v < 500 ? 'badge-warning' : 'badge-error';
      return `<span class="badge ${cls}">${v}</span>`;
    }}
  ];

  onMount(async () => {
    try {
      stats = await api.dashboard.getStats();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load dashboard';
      // Provide fallback data for development
      stats = {
        total_requests: 0,
        total_tokens: 0,
        active_providers: 0,
        active_models: 0,
        requests_by_provider: {},
        requests_by_model: {},
        recent_requests: []
      };
    } finally {
      loading = false;
    }
  });

  function formatNumber(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
    return n.toString();
  }
</script>

<PageHeader title="Dashboard" description="Overview of your gateway activity and performance" />

{#if loading}
  <div class="stats-grid">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 88px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
{:else if stats}
  <!-- Stats -->
  <div class="stats-grid">
    <StatCard
      icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      value={formatNumber(stats.total_requests)}
      label="Total Requests"
      color="blue"
    />
    <StatCard
      icon="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z"
      value={formatNumber(stats.total_tokens)}
      label="Total Tokens"
      color="purple"
    />
    <StatCard
      icon="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2"
      value={stats.active_providers}
      label="Active Providers"
      color="green"
    />
    <StatCard
      icon="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
      value={stats.active_models}
      label="Active Models"
      color="amber"
    />
  </div>

  <!-- Breakdown -->
  <div class="breakdown-grid">
    <Card padding="0">
      <div class="card-title-bar">
        <h3 class="card-title">Requests by Provider</h3>
      </div>
      <div class="breakdown-list">
        {#if Object.keys(stats.requests_by_provider).length === 0}
          <div class="breakdown-empty">No data yet</div>
        {:else}
          {#each Object.entries(stats.requests_by_provider).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-item">
              <span class="breakdown-name">{name}</span>
              <div class="breakdown-bar-wrapper">
                <div
                  class="breakdown-bar"
                  style:width="{Math.max(4, (count / Math.max(...Object.values(stats.requests_by_provider))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-count">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>

    <Card padding="0">
      <div class="card-title-bar">
        <h3 class="card-title">Requests by Model</h3>
      </div>
      <div class="breakdown-list">
        {#if Object.keys(stats.requests_by_model).length === 0}
          <div class="breakdown-empty">No data yet</div>
        {:else}
          {#each Object.entries(stats.requests_by_model).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-item">
              <span class="breakdown-name">{name}</span>
              <div class="breakdown-bar-wrapper">
                <div
                  class="breakdown-bar bar-purple"
                  style:width="{Math.max(4, (count / Math.max(...Object.values(stats.requests_by_model))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-count">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>
  </div>

  <!-- Recent Requests -->
  <section class="recent-section">
    <h3 class="section-title">Recent Requests</h3>
    <DataTable columns={recentColumns} data={stats.recent_requests} emptyMessage="No requests recorded yet" />
  </section>
{/if}

{#if error}
  <div class="error-banner">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
    </svg>
    {error}
  </div>
{/if}

<style>
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
    margin-bottom: 28px;
  }

  .breakdown-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin-bottom: 28px;
  }

  .card-title-bar {
    padding: 16px 20px 12px;
    border-bottom: 1px solid var(--border);
  }

  .card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
    letter-spacing: 0.01em;
  }

  .breakdown-list {
    padding: 8px 12px;
  }

  .breakdown-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border-radius: var(--radius-sm);
  }

  .breakdown-item:hover {
    background: rgba(255, 255, 255, 0.02);
  }

  .breakdown-name {
    font-size: 13px;
    color: var(--text-secondary);
    min-width: 100px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .breakdown-bar-wrapper {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
    overflow: hidden;
  }

  .breakdown-bar {
    height: 100%;
    background: linear-gradient(90deg, var(--accent), #60a5fa);
    border-radius: 3px;
    transition: width var(--transition-slow);
  }

  .breakdown-bar.bar-purple {
    background: linear-gradient(90deg, #8b5cf6, #a78bfa);
  }

  .breakdown-count {
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-mono);
    color: var(--text-primary);
    min-width: 40px;
    text-align: right;
  }

  .breakdown-empty {
    padding: 24px;
    text-align: center;
    font-size: 13px;
    color: var(--text-muted);
  }

  .recent-section {
    margin-top: 4px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 14px;
    letter-spacing: -0.01em;
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--error-muted);
    border: 1px solid rgba(239, 68, 68, 0.2);
    color: var(--error);
    font-size: 13px;
    margin-top: 16px;
  }

  @media (max-width: 900px) {
    .stats-grid {
      grid-template-columns: 1fr 1fr;
    }
    .breakdown-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
