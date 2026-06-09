<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DashboardStats, Column } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import StatCard from '$lib/components/StatCard.svelte';
  import Card from '$lib/components/Card.svelte';
  import DataTable from '$lib/components/DataTable.svelte';
  import { i18n } from '$lib/i18n.svelte';

  let stats = $state<DashboardStats | null>(null);
  let loading = $state(true);
  let error = $state('');

  const recentColumns = $derived.by((): Column[] => {
    void i18n.currentLang;
    return [
      {
        key: 'timestamp',
        label: i18n.t('logs.time'),
        sortable: true,
        render: (v: string) => {
          try {
            return new Date(v).toLocaleTimeString();
          } catch {
            return v;
          }
        },
      },
      { key: 'agent', label: i18n.t('logs.agent'), sortable: true },
      { key: 'provider', label: i18n.t('logs.provider'), sortable: true },
      { key: 'model', label: i18n.t('logs.model'), sortable: true },
      {
        key: 'total_tokens',
        label: i18n.t('dashboard.tokens'),
        sortable: true,
        align: 'right' as const,
        render: (v: number) => v?.toLocaleString() ?? '0',
      },
      {
        key: 'latency_ms',
        label: i18n.t('logs.latency'),
        sortable: true,
        align: 'right' as const,
        render: (v: number) => `${v}ms`,
      },
      {
        key: 'status_code',
        label: i18n.t('logs.status_code'),
        align: 'center' as const,
        render: (v: number) => {
          const cls = v < 300 ? 'badge-success' : v < 500 ? 'badge-warning' : 'badge-error';
          return `<span class="badge ${cls}">${v}</span>`;
        },
      },
    ];
  });

  onMount(async () => {
    try {
      stats = await api.dashboard.getStats();
    } catch (e) {
      error = e instanceof Error ? e.message : i18n.t('dashboard.load_failed');
      stats = {
        total_requests: 0,
        total_tokens: 0,
        active_providers: 0,
        active_models: 0,
        requests_by_provider: {},
        requests_by_model: {},
        recent_requests: [],
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

<PageHeader title={i18n.t('dashboard.title')} description={i18n.t('dashboard.subtitle')} />

{#if loading}
  <div class="stats-grid">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 88px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
{:else if stats}
  <div class="stats-grid">
    <StatCard
      icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      value={formatNumber(stats.total_requests)}
      label={i18n.t('dashboard.total_requests')}
      color="blue"
    />
    <StatCard
      icon="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z"
      value={formatNumber(stats.total_tokens)}
      label={i18n.t('dashboard.total_tokens')}
      color="purple"
    />
    <StatCard
      icon="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2"
      value={stats.active_providers}
      label={i18n.t('dashboard.active_providers')}
      color="green"
    />
    <StatCard
      icon="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
      value={stats.active_models}
      label={i18n.t('dashboard.active_models')}
      color="amber"
    />
  </div>

  <div class="breakdown-grid">
    <Card padding="0">
      <div class="card-title-bar">
        <h3 class="card-title">{i18n.t('dashboard.req_by_provider')}</h3>
      </div>
      <div class="breakdown-list">
        {#if Object.keys(stats.requests_by_provider).length === 0}
          <div class="breakdown-empty">{i18n.t('dashboard.no_data_yet')}</div>
        {:else}
          {#each Object.entries(stats.requests_by_provider).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-item">
              <span class="breakdown-name">{name}</span>
              <div class="breakdown-bar-wrapper">
                <div
                  class="breakdown-bar"
                  style:width="{Math.max(
                    4,
                    (count / Math.max(...Object.values(stats.requests_by_provider))) * 100
                  )}%"
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
        <h3 class="card-title">{i18n.t('dashboard.req_by_model')}</h3>
      </div>
      <div class="breakdown-list">
        {#if Object.keys(stats.requests_by_model).length === 0}
          <div class="breakdown-empty">{i18n.t('dashboard.no_data_yet')}</div>
        {:else}
          {#each Object.entries(stats.requests_by_model).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-item">
              <span class="breakdown-name">{name}</span>
              <div class="breakdown-bar-wrapper">
                <div
                  class="breakdown-bar bar-purple"
                  style:width="{Math.max(
                    4,
                    (count / Math.max(...Object.values(stats.requests_by_model))) * 100
                  )}%"
                ></div>
              </div>
              <span class="breakdown-count">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>
  </div>

  <section class="recent-section">
    <h3 class="section-title">{i18n.t('dashboard.recent_requests')}</h3>
    <DataTable
      columns={recentColumns}
      data={stats.recent_requests}
      emptyMessage={i18n.t('dashboard.empty_recent')}
      searchPlaceholder={i18n.t('common.search')}
    />
  </section>
{/if}

{#if error}
  <div class="error-banner">
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
      <path
        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z"
      />
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
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }

  .card-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .breakdown-list {
    padding: 12px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .breakdown-item {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .breakdown-name {
    font-size: 12px;
    color: var(--text-secondary);
    width: 100px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    transition: width 0.4s ease;
  }

  .bar-purple {
    background: linear-gradient(90deg, #8b5cf6, #c084fc);
  }

  .breakdown-count {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    width: 40px;
    text-align: right;
    flex-shrink: 0;
  }

  .breakdown-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .recent-section {
    margin-top: 8px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 12px;
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md);
    color: #fca5a5;
    font-size: 13px;
    margin-top: 16px;
  }

  @media (max-width: 1024px) {
    .stats-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 640px) {
    .stats-grid,
    .breakdown-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
