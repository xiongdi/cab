<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DashboardStats, Column } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
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
  <div class="metrics-grid">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 96px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
  <div class="skeleton" style="height: 300px; border-radius: var(--radius-lg); margin-top: 24px;"></div>
{:else if stats}
  <!-- Metrics Row -->
  <div class="metrics-grid">
    <div class="metric-card">
      <div class="metric-icon metric-icon--blue">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value">{formatNumber(stats.total_requests)}</span>
        <span class="metric-label">{i18n.t('dashboard.total_requests')}</span>
      </div>
    </div>
    <div class="metric-card">
      <div class="metric-icon metric-icon--purple">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value">{formatNumber(stats.total_tokens)}</span>
        <span class="metric-label">{i18n.t('dashboard.total_tokens')}</span>
      </div>
    </div>
    <div class="metric-card">
      <div class="metric-icon metric-icon--green">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value">{stats.active_providers}</span>
        <span class="metric-label">{i18n.t('dashboard.active_providers')}</span>
      </div>
    </div>
    <div class="metric-card">
      <div class="metric-icon metric-icon--amber">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value">{stats.active_models}</span>
        <span class="metric-label">{i18n.t('dashboard.active_models')}</span>
      </div>
    </div>
  </div>

  <!-- Breakdowns Row -->
  <div class="breakdown-grid">
    <Card padding="0">
      <div class="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: var(--accent-text); flex-shrink: 0;">
          <path d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
        </svg>
        <h3>{i18n.t('dashboard.req_by_provider')}</h3>
      </div>
      <div class="section-card-body">
        {#if Object.keys(stats.requests_by_provider).length === 0}
          <div class="breakdown-empty">{i18n.t('dashboard.no_data_yet')}</div>
        {:else}
          {#each Object.entries(stats.requests_by_provider).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-row">
              <span class="breakdown-label">{name}</span>
              <div class="breakdown-track">
                <div
                  class="breakdown-fill breakdown-fill--blue"
                  style:width="{Math.max(2, (count / Math.max(...Object.values(stats.requests_by_provider))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-value">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>

    <Card padding="0">
      <div class="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: #a78bfa; flex-shrink: 0;">
          <path d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
        <h3>{i18n.t('dashboard.req_by_model')}</h3>
      </div>
      <div class="section-card-body">
        {#if Object.keys(stats.requests_by_model).length === 0}
          <div class="breakdown-empty">{i18n.t('dashboard.no_data_yet')}</div>
        {:else}
          {#each Object.entries(stats.requests_by_model).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-row">
              <span class="breakdown-label">{name}</span>
              <div class="breakdown-track">
                <div
                  class="breakdown-fill breakdown-fill--purple"
                  style:width="{Math.max(2, (count / Math.max(...Object.values(stats.requests_by_model))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-value">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>
  </div>

  <!-- Recent Requests -->
  <section class="recent-section">
    <div class="recent-header">
      <div class="recent-header-left">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: var(--text-muted); flex-shrink: 0;">
          <path d="M9 5H7a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
        </svg>
        <h3>{i18n.t('dashboard.recent_requests')}</h3>
      </div>
    </div>
    <DataTable
      columns={recentColumns}
      data={stats.recent_requests}
      emptyMessage={i18n.t('dashboard.empty_recent')}
      searchPlaceholder={i18n.t('common.search')}
      showSearch={false}
    />
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
  /* ── Metrics Grid ─────────────────────────────────────── */
  .metrics-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    margin-bottom: 24px;
  }

  .metric-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 20px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    transition: all var(--transition-normal);
    position: relative;
    overflow: hidden;
  }

  .metric-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.06), transparent);
  }

  .metric-card:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-md);
  }

  .metric-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: 10px;
    flex-shrink: 0;
  }

  .metric-icon--blue {
    background: rgba(59, 130, 246, 0.12);
    color: #60a5fa;
  }

  .metric-icon--purple {
    background: rgba(139, 92, 246, 0.12);
    color: #a78bfa;
  }

  .metric-icon--green {
    background: rgba(34, 197, 94, 0.12);
    color: #4ade80;
  }

  .metric-icon--amber {
    background: rgba(245, 158, 11, 0.12);
    color: #fbbf24;
  }

  .metric-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .metric-value {
    font-size: 20px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .metric-label {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  /* ── Breakdown Grid ───────────────────────────────────── */
  .breakdown-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 28px;
  }

  .section-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }

  .section-card-header h3 {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .section-card-body {
    padding: 14px 18px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .breakdown-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .breakdown-label {
    font-size: 12px;
    color: var(--text-secondary);
    width: 110px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 450;
  }

  .breakdown-track {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
    overflow: hidden;
  }

  .breakdown-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.4s ease;
    min-width: 2px;
  }

  .breakdown-fill--blue {
    background: linear-gradient(90deg, #3b82f6, #60a5fa);
  }

  .breakdown-fill--purple {
    background: linear-gradient(90deg, #8b5cf6, #c084fc);
  }

  .breakdown-value {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    min-width: 36px;
    text-align: right;
    flex-shrink: 0;
  }

  .breakdown-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  /* ── Recent Requests ──────────────────────────────────── */
  .recent-section {
    margin-top: 4px;
  }

  .recent-header {
    margin-bottom: 12px;
  }

  .recent-header-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .recent-header-left h3 {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  /* ── Error ────────────────────────────────────────────── */
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

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 1100px) {
    .metrics-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 640px) {
    .metrics-grid,
    .breakdown-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
