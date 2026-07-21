<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DashboardStats, Column } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import DataTable from '$lib/components/DataTable.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { gatewayHealth } from '$lib/gateway-health.svelte';

  let stats = $state<DashboardStats | null>(null);
  let loading = $state(true);
  let error = $state('');
  
  // Row Expansion State
  let expandedRowId = $state<string | null>(null);

  // Performance computations from recent logs
  let averageLatency = $derived.by(() => {
    if (!stats || !stats.recent_requests || stats.recent_requests.length === 0) return 0;
    const sum = stats.recent_requests.reduce((acc, r) => acc + r.latency_ms, 0);
    return Math.round(sum / stats.recent_requests.length);
  });

  let successRate = $derived.by(() => {
    if (!stats || !stats.recent_requests || stats.recent_requests.length === 0) return 100;
    const successCount = stats.recent_requests.filter(
      (r) => r.status_code >= 200 && r.status_code < 400
    ).length;
    return Math.round((successCount / stats.recent_requests.length) * 100);
  });

  let cacheHitRate = $derived.by(() => {
    if (!stats || !stats.recent_requests || stats.recent_requests.length === 0) return 0;
    const hitCount = stats.recent_requests.filter(
      (r) => (r.cache_read_tokens ?? 0) > 0
    ).length;
    return Math.round((hitCount / stats.recent_requests.length) * 100);
  });

  // Dynamic Agent requests distribution from recent requests
  let requestsByAgent = $derived.by(() => {
    if (!stats || !stats.recent_requests) return {};
    const dist: Record<string, number> = {};
    for (const r of stats.recent_requests) {
      const agent = r.agent || 'unknown';
      dist[agent] = (dist[agent] || 0) + 1;
    }
    return dist;
  });

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
      {
        key: 'agent',
        label: i18n.t('logs.agent'),
        sortable: true,
        render: (v: string) => {
          const lower = (v || '').toLowerCase();
          let cls = 'badge-agent-generic';
          if (lower.includes('claude')) cls = 'badge-agent-claude';
          else if (lower.includes('code')) cls = 'badge-agent-code';
          else if (lower.includes('pi')) cls = 'badge-agent-pi';
          return `<span class="badge-agent ${cls}">${v || i18n.t('common.unknown')}</span>`;
        },
      },
      {
        key: 'model',
        label: i18n.t('logs.model'),
        sortable: true,
        render: (v: string, row: any) => {
          return `<div class="model-flow-cell"><span class="model-provider-badge">${row.provider}</span><span class="flow-arrow">➔</span><span class="model-name-badge">${v}</span></div>`;
        },
      },
      {
        key: 'total_tokens',
        label: i18n.t('dashboard.tokens'),
        sortable: true,
        align: 'right' as const,
        render: (v: number) => `<span class="mono-text">${v?.toLocaleString() ?? '0'}</span>`,
      },
      {
        key: 'latency_ms',
        label: i18n.t('logs.latency'),
        sortable: true,
        align: 'right' as const,
        render: (v: number) => `<span class="mono-text">${v}ms</span>`,
      },
      {
        key: 'status_code',
        label: i18n.t('logs.status_code'),
        align: 'center' as const,
        render: (v: number) => {
          const cls = v < 300 ? 'status-glow-success' : v < 500 ? 'status-glow-warning' : 'status-glow-error';
          return `<span class="status-dot-badge ${cls}"><span class="status-dot-core"></span>${v}</span>`;
        },
      },
    ];
  });

  onMount(async () => {
    // Start polling gateway health
    gatewayHealth.start();
    
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

  function handleRowClick(row: any) {
    if (expandedRowId === row.id) {
      expandedRowId = null;
    } else {
      expandedRowId = row.id;
    }
  }

  function isRowExpanded(row: any) {
    return expandedRowId === row.id;
  }
</script>

<PageHeader title={i18n.t('dashboard.title')} description={i18n.t('dashboard.subtitle')} />

{#if loading}
  <div class="gateway-center-card skeleton-loading" style="height: 160px; margin-bottom: 24px;"></div>
  <div class="metrics-grid">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 96px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
  <div class="skeleton" style="height: 300px; border-radius: var(--radius-lg); margin-top: 24px;"></div>
{:else if stats}
  <!-- Gateway Control Center -->
  <div class="gateway-center-card">
    <div class="control-header">
      <div class="gateway-title-wrapper">
        <div class="pulse-ring">
          <span class="pulse-dot {gatewayHealth.status}"></span>
        </div>
        <div class="gateway-meta">
          <h3>{i18n.t('dashboard.gateway_center_title')}</h3>
          <span class="gateway-subtitle">
            {#if gatewayHealth.status === 'running'}
              {i18n.tParams('dashboard.gateway_running_subtitle', { port: '3125' })}
            {:else}
              {i18n.t('dashboard.gateway_stopped_subtitle')}
            {/if}
          </span>
        </div>
      </div>
      <div class="port-badge-wrapper">
        <span class="port-badge">{i18n.tParams('dashboard.port_badge', { port: '3125' })}</span>
      </div>
    </div>
    
    <div class="gateway-performance-grid">
      <div class="perf-card">
        <div class="perf-icon-bg perf-icon-bg--green">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 12l2 2 4-4M7.835 4.697a3.42 3.42 0 001.946-.806 3.42 3.42 0 014.438 0 3.42 3.42 0 001.946.806 3.42 3.42 0 013.138 3.138 3.42 3.42 0 00.806 1.946 3.42 3.42 0 010 4.438 3.42 3.42 0 00-.806 1.946 3.42 3.42 0 01-3.138 3.138 3.42 3.42 0 00-1.946.806 3.42 3.42 0 01-4.438 0 3.42 3.42 0 00-1.946-.806 3.42 3.42 0 01-3.138-3.138 3.42 3.42 0 00-.806-1.946 3.42 3.42 0 010-4.438 3.42 3.42 0 00.806-1.946 3.42 3.42 0 013.138-3.138z" />
          </svg>
        </div>
        <div class="perf-card-body">
          <span class="perf-card-value font-mono">{successRate}%</span>
          <span class="perf-card-label">{i18n.t('dashboard.recent_success_rate')}</span>
        </div>
      </div>
      
      <div class="perf-card">
        <div class="perf-icon-bg perf-icon-bg--blue">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
        </div>
        <div class="perf-card-body">
          <span class="perf-card-value font-mono">{averageLatency}ms</span>
          <span class="perf-card-label">{i18n.t('dashboard.recent_avg_latency')}</span>
        </div>
      </div>
      
      <div class="perf-card">
        <div class="perf-icon-bg perf-icon-bg--purple">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />
          </svg>
        </div>
        <div class="perf-card-body">
          <span class="perf-card-value font-mono">{cacheHitRate}%</span>
          <span class="perf-card-label">{i18n.t('dashboard.cache_hit_rate')}</span>
        </div>
      </div>
    </div>
  </div>

  <!-- Metrics Row -->
  <div class="metrics-grid">
    <div class="metric-card metric-card--requests">
      <div class="metric-icon metric-icon--blue">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value font-mono">{formatNumber(stats.total_requests)}</span>
        <span class="metric-label">{i18n.t('dashboard.total_requests')}</span>
      </div>
    </div>
    <div class="metric-card metric-card--tokens">
      <div class="metric-icon metric-icon--purple">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value font-mono">{formatNumber(stats.total_tokens)}</span>
        <span class="metric-label">{i18n.t('dashboard.total_tokens')}</span>
      </div>
    </div>
    <div class="metric-card metric-card--providers">
      <div class="metric-icon metric-icon--green">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value font-mono">{stats.active_providers}</span>
        <span class="metric-label">{i18n.t('dashboard.active_providers')}</span>
      </div>
    </div>
    <div class="metric-card metric-card--models">
      <div class="metric-icon metric-icon--amber">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
      </div>
      <div class="metric-body">
        <span class="metric-value font-mono">{stats.active_models}</span>
        <span class="metric-label">{i18n.t('dashboard.active_models')}</span>
      </div>
    </div>
  </div>

  <!-- Breakdowns Row -->
  <div class="breakdown-grid">
    <Card padding="0">
      <div class="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: var(--chart-blue); flex-shrink: 0;">
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
              <span class="breakdown-label" title={name}>{name}</span>
              <div class="breakdown-track">
                <div
                  class="breakdown-fill breakdown-fill--blue"
                  style:width="{Math.max(2, (count / Math.max(...Object.values(stats.requests_by_provider))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-value font-mono">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>

    <Card padding="0">
      <div class="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: var(--chart-purple); flex-shrink: 0;">
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
              <span class="breakdown-label" title={name}>{name}</span>
              <div class="breakdown-track">
                <div
                  class="breakdown-fill breakdown-fill--purple"
                  style:width="{Math.max(2, (count / Math.max(...Object.values(stats.requests_by_model))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-value font-mono">{formatNumber(count)}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Card>

    <Card padding="0">
      <div class="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" style="color: var(--success-text); flex-shrink: 0;">
          <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2M9 11a4 4 0 100-8 4 4 0 000 8z" />
        </svg>
        <h3>{i18n.t('dashboard.req_by_agent')}</h3>
      </div>
      <div class="section-card-body">
        {#if Object.keys(requestsByAgent).length === 0}
          <div class="breakdown-empty">{i18n.t('dashboard.no_data_yet')}</div>
        {:else}
          {#each Object.entries(requestsByAgent).sort((a, b) => b[1] - a[1]) as [name, count]}
            <div class="breakdown-row">
              <span class="breakdown-label" title={name}>{name}</span>
              <div class="breakdown-track">
                <div
                  class="breakdown-fill breakdown-fill--green"
                  style:width="{Math.max(2, (count / Math.max(...Object.values(requestsByAgent))) * 100)}%"
                ></div>
              </div>
              <span class="breakdown-value font-mono">{formatNumber(count)}</span>
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
      onRowClick={handleRowClick}
      isRowExpanded={isRowExpanded}
    >
      {#snippet expandedRow(row)}
        <div class="expanded-detail-panel">
          <div class="detail-grid">
            <!-- Token Breakdown -->
            <div class="detail-section">
              <h4 class="detail-section-title">{i18n.t('dashboard.detail_token_distribution')}</h4>
              <div class="token-breakdown-container">
                <div class="token-progress-bar">
                  {#if (row.cache_read_tokens ?? 0) > 0}
                    <div 
                      class="token-bar token-bar--cache" 
                      style:width="{(row.cache_read_tokens / row.total_tokens) * 100}%"
                      title={i18n.tParams('dashboard.legend_cache_hit', { tokens: String(row.cache_read_tokens), pct: String(Math.round((row.cache_read_tokens / row.total_tokens) * 100)) })}
                    ></div>
                  {/if}
                  <div 
                    class="token-bar token-bar--input" 
                    style:width="{((row.input_tokens - (row.cache_read_tokens ?? 0)) / row.total_tokens) * 100}%"
                    title={i18n.tParams('dashboard.legend_input', { tokens: String(row.input_tokens - (row.cache_read_tokens ?? 0)), pct: String(Math.round(((row.input_tokens - (row.cache_read_tokens ?? 0)) / row.total_tokens) * 100)) })}
                  ></div>
                  <div 
                    class="token-bar token-bar--output" 
                    style:width="{(row.output_tokens / row.total_tokens) * 100}%"
                    title={i18n.tParams('dashboard.legend_output', { tokens: String(row.output_tokens), pct: String(Math.round((row.output_tokens / row.total_tokens) * 100)) })}
                  ></div>
                </div>
                
                <div class="token-legend">
                  {#if (row.cache_read_tokens ?? 0) > 0}
                    <span class="legend-item"><span class="legend-dot legend-dot--cache"></span>{i18n.tParams('dashboard.legend_cache_hit', { tokens: String(row.cache_read_tokens), pct: String(Math.round((row.cache_read_tokens / row.total_tokens) * 100)) })}</span>
                  {/if}
                  <span class="legend-item"><span class="legend-dot legend-dot--input"></span>{i18n.tParams('dashboard.legend_input', { tokens: String(row.input_tokens - (row.cache_read_tokens ?? 0)), pct: String(Math.round(((row.input_tokens - (row.cache_read_tokens ?? 0)) / row.total_tokens) * 100)) })}</span>
                  <span class="legend-item"><span class="legend-dot legend-dot--output"></span>{i18n.tParams('dashboard.legend_output', { tokens: String(row.output_tokens), pct: String(Math.round((row.output_tokens / row.total_tokens) * 100)) })}</span>
                </div>
              </div>
            </div>
            
            <!-- Diagnostics -->
            {#if row.status_code >= 400 || row.error_message}
              <div class="detail-section detail-section--full">
                <h4 class="detail-section-title error-text">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 4px; vertical-align: middle;">
                    <path d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                  </svg>
                  {i18n.t('dashboard.gateway_diag_error')}
                </h4>
                <div class="error-terminal">
                  <div class="terminal-header">
                    <span class="terminal-dot red"></span>
                    <span class="terminal-dot yellow"></span>
                    <span class="terminal-dot green"></span>
                    <span class="terminal-title">status_{row.status_code}.log</span>
                  </div>
                  <pre class="terminal-body">{row.error_message || i18n.tParams('dashboard.request_failed_fallback', { code: row.status_code })}</pre>
                </div>
              </div>
            {:else}
              <div class="detail-section">
                <h4 class="detail-section-title">{i18n.t('logs.detail_performance')}</h4>
                <div class="performance-metrics-list">
                  <div class="perf-metric">
                    <span class="perf-label">{i18n.t('dashboard.perf_cache_status')}</span>
                    <span class="perf-value">
                      {#if (row.cache_read_tokens ?? 0) > 0}
                        <span class="badge-cache badge-cache--hit">{i18n.tParams('dashboard.perf_cache_hit', { pct: String(Math.round((row.cache_read_tokens / row.input_tokens) * 100)) })}</span>
                      {:else}
                        <span class="badge-cache badge-cache--miss">{i18n.t('dashboard.perf_cache_miss')}</span>
                      {/if}
                    </span>
                  </div>
                  <div class="perf-metric">
                    <span class="perf-label">{i18n.t('dashboard.perf_efficiency')}</span>
                    <span class="perf-value">
                      {#if row.output_tokens > 0 && row.latency_ms > 0}
                        <span class="mono-text">{i18n.tParams('common.tokens_per_sec', { value: String(Math.round((row.output_tokens / (row.latency_ms / 1000)) * 10) / 10) })}</span>
                      {:else}
                        -
                      {/if}
                    </span>
                  </div>
                  <div class="perf-metric">
                    <span class="perf-label">{i18n.t('dashboard.request_id')}</span>
                    <span class="perf-value mono-text text-small">{row.id}</span>
                  </div>
                </div>
              </div>
            {/if}
          </div>
        </div>
      {/snippet}
    </DataTable>
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
  /* ── Fonts ────────────────────────────────────────────── */
  .font-mono {
    font-family: var(--font-mono);
  }

  /* ── Gateway Control Center ───────────────────────────── */
  .gateway-center-card {
    background: var(--gradient-card);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-xl);
    padding: 24px;
    margin-bottom: 24px;
    position: relative;
    overflow: hidden;
  }

  .gateway-center-card::after {
    content: '';
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    background: var(--gateway-accent-glow);
    pointer-events: none;
  }

  .control-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border);
    padding-bottom: 18px;
    margin-bottom: 20px;
  }

  .gateway-title-wrapper {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .pulse-ring {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
  }

  .pulse-dot {
    width: 10px;
    height: 10px;
    border-radius: var(--radius-full);
    display: inline-block;
  }

  .pulse-dot.running {
    background-color: var(--success);
    box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.4);
    animation: pulse-success 2s infinite;
  }

  .pulse-dot.checking {
    background-color: var(--warning);
    box-shadow: 0 0 0 0 rgba(245, 158, 11, 0.4);
    animation: pulse-warning 2s infinite;
  }

  .pulse-dot.stopped, .pulse-dot.error {
    background-color: var(--error);
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.4);
    animation: pulse-error 2s infinite;
  }

  @keyframes pulse-success {
    0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.5); }
    70% { transform: scale(1); box-shadow: 0 0 0 8px rgba(34, 197, 94, 0); }
    100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(34, 197, 94, 0); }
  }

  @keyframes pulse-warning {
    0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(245, 158, 11, 0.5); }
    70% { transform: scale(1); box-shadow: 0 0 0 8px rgba(245, 158, 11, 0); }
    100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(245, 158, 11, 0); }
  }

  @keyframes pulse-error {
    0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.5); }
    70% { transform: scale(1); box-shadow: 0 0 0 8px rgba(239, 68, 68, 0); }
    100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(239, 68, 68, 0); }
  }

  .gateway-meta h3 {
    font-size: 15px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  .gateway-subtitle {
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .port-badge-wrapper {
    flex-shrink: 0;
  }

  .port-badge {
    background: var(--glass-bg-hover);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
  }

  .gateway-performance-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
  }

  .perf-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: var(--glass-bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
  }

  .perf-icon-bg {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
  }

  .perf-icon-bg--green { background: var(--icon-green-bg); color: var(--chart-green); }
  .perf-icon-bg--blue { background: var(--icon-blue-bg); color: var(--chart-blue); }
  .perf-icon-bg--purple { background: var(--icon-purple-bg); color: var(--chart-purple-light); }

  .perf-card-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .perf-card-value {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .perf-card-label {
    font-size: 11px;
    color: var(--text-muted);
  }

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
    background: var(--gradient-card-subtle);
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
    top: 0; left: 0; right: 0;
    height: 1px;
    background: var(--gradient-shine);
  }

  .metric-card:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-hover);
    transform: translateY(-2px);
  }

  .metric-card--requests:hover { box-shadow: 0 6px 20px rgba(59, 130, 246, 0.08); border-color: rgba(59, 130, 246, 0.25); }
  .metric-card--tokens:hover { box-shadow: 0 6px 20px rgba(139, 92, 246, 0.08); border-color: rgba(139, 92, 246, 0.25); }
  .metric-card--providers:hover { box-shadow: 0 6px 20px rgba(34, 197, 94, 0.08); border-color: rgba(34, 197, 94, 0.25); }
  .metric-card--models:hover { box-shadow: 0 6px 20px rgba(245, 158, 11, 0.08); border-color: rgba(245, 158, 11, 0.25); }

  .metric-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: var(--radius-md);
    flex-shrink: 0;
  }

  .metric-icon--blue { background: var(--icon-blue-bg); color: var(--chart-blue); }
  .metric-icon--purple { background: var(--icon-purple-bg); color: var(--chart-purple); }
  .metric-icon--green { background: var(--icon-green-bg); color: var(--chart-green); }
  .metric-icon--amber { background: var(--icon-amber-bg); color: var(--chart-amber); }

  .metric-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .metric-value {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .metric-label {
    font-size: 11.5px;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  /* ── Breakdown Grid ───────────────────────────────────── */
  .breakdown-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    margin-bottom: 28px;
  }

  .breakdown-grid :global(.card) {
    background: var(--gradient-card-faint) !important;
    border: 1px solid var(--border) !important;
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
    padding: 16px 18px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .breakdown-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .breakdown-label {
    font-size: 11.5px;
    color: var(--text-secondary);
    min-width: 60px;
    max-width: 120px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 400;
  }

  .breakdown-track {
    flex: 1;
    height: 6px;
    background: var(--border-dashed-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    position: relative;
    overflow: hidden;
  }

  .breakdown-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.6s cubic-bezier(0.16, 1, 0.3, 1);
    min-width: 2px;
    position: relative;
  }

  .breakdown-fill::after {
    content: '';
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    background: var(--bar-shimmer);
    animation: shimmer-bar 3s infinite linear;
  }

  @keyframes shimmer-bar {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  .breakdown-fill--blue { background: linear-gradient(90deg, var(--chart-blue-deep), var(--chart-blue)); box-shadow: 0 0 10px rgba(59, 130, 246, 0.15); }
  .breakdown-fill--purple { background: linear-gradient(90deg, var(--chart-purple-deep), var(--chart-purple-light)); box-shadow: 0 0 10px rgba(139, 92, 246, 0.15); }
  .breakdown-fill--green { background: linear-gradient(90deg, var(--chart-green-strong), var(--success-text)); box-shadow: 0 0 10px rgba(52, 211, 153, 0.15); }

  .breakdown-value {
    font-size: 11.5px;
    color: var(--text-muted);
    min-width: 36px;
    text-align: right;
    flex-shrink: 0;
  }

  .breakdown-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
  }

  /* ── Recent Requests ──────────────────────────────────── */
  .recent-section {
    margin-top: 4px;
  }

  .recent-header {
    margin-bottom: 16px;
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

  /* ── Expanded Detail Panel (Row Expansion) ────────────── */
  .expanded-detail-panel {
    background: var(--bg-card-expanded);
    border-bottom: 1px solid var(--border);
    padding: 20px 24px;
    animation: slide-down 0.2s ease-out;
  }

  @keyframes slide-down {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 24px;
  }

  .detail-section--full {
    grid-column: 1 / -1;
  }

  .detail-section-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .detail-section-title.error-text {
    color: var(--error-text);
  }

  /* Token progress bar stack */
  .token-breakdown-container {
    background: var(--glass-bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 16px;
  }

  .token-progress-bar {
    height: 10px;
    background: var(--glass-bg-hover);
    border-radius: var(--radius-full);
    display: flex;
    overflow: hidden;
    margin-bottom: 14px;
    border: 1px solid var(--border-subtle);
  }

  .token-bar {
    height: 100%;
    transition: width 0.4s ease;
  }

  .token-bar--cache { background: linear-gradient(90deg, var(--success), var(--success-text)); }
  .token-bar--input { background: linear-gradient(90deg, var(--chart-blue-deep), var(--chart-blue-strong)); }
  .token-bar--output { background: linear-gradient(90deg, var(--chart-purple-deep), var(--chart-purple)); }

  .token-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .legend-item {
    font-size: 11px;
    color: var(--text-secondary);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .legend-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    display: inline-block;
  }

  .legend-dot--cache { background-color: var(--success-text); }
  .legend-dot--input { background-color: var(--chart-blue-strong); }
  .legend-dot--output { background-color: var(--chart-purple); }

  /* Performance list */
  .performance-metrics-list {
    background: var(--glass-bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: calc(100% - 28px);
    justify-content: center;
  }

  .perf-metric {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    border-bottom: 1px dashed var(--border);
    padding-bottom: 8px;
  }

  .perf-metric:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .perf-label {
    color: var(--text-secondary);
  }

  .perf-value {
    color: var(--text-primary);
  }

  /* Diagnostic console */
  .error-terminal {
    background: var(--bg-terminal);
    border: 1px solid rgba(239, 68, 68, 0.15);
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: var(--terminal-shadow);
  }

  .terminal-header {
    background: var(--glass-bg-subtle);
    border-bottom: 1px solid var(--border);
    padding: 8px 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .terminal-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
  }

  .terminal-dot.red { background-color: var(--error); }
  .terminal-dot.yellow { background-color: var(--warning); }
  .terminal-dot.green { background-color: var(--success); }

  .terminal-title {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    margin-left: 6px;
  }

  .terminal-body {
    margin: 0;
    padding: 12px 16px;
    color: var(--error-text);
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow-y: auto;
  }

  /* ── Badge Globals for DataTable Render ───────────────── */
  :global(.badge-agent) {
    font-size: 10.5px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-weight: 500;
    display: inline-block;
  }

  :global(.badge-agent-claude) {
    background: rgba(217, 119, 6, 0.1);
    color: var(--warning);
    border: 1px solid rgba(217, 119, 6, 0.15);
  }

  :global(.badge-agent-code) {
    background: var(--icon-purple-bg);
    color: var(--chart-purple);
    border: 1px solid rgba(139, 92, 246, 0.15);
  }

  :global(.badge-agent-pi) {
    background: rgba(16, 185, 129, 0.1);
    color: var(--success-text);
    border: 1px solid rgba(16, 185, 129, 0.15);
  }

  :global(.badge-agent-generic) {
    background: var(--bg-badge);
    color: var(--text-secondary);
    border: 1px solid var(--border);
  }

  :global(.model-flow-cell) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.model-provider-badge) {
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-badge);
    padding: 1px 6px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border-subtle);
  }

  :global(.flow-arrow) {
    color: var(--text-muted);
    font-size: 10px;
  }

  :global(.model-name-badge) {
    font-size: 11.5px;
    color: var(--text-primary);
    font-weight: 500;
  }

  :global(.mono-text) {
    font-family: var(--font-mono);
    font-size: 12px;
  }

  :global(.text-small) {
    font-size: 11px;
  }

  :global(.status-dot-badge) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-family: var(--font-mono);
    padding: 2px 8px;
    border-radius: var(--radius-full);
    font-weight: 500;
  }

  :global(.status-dot-core) {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
  }

  :global(.status-glow-success) {
    background: rgba(16, 185, 129, 0.08);
    color: var(--success-text);
    border: 1px solid rgba(16, 185, 129, 0.12);
  }

  :global(.status-glow-success .status-dot-core) {
    background-color: var(--success);
    box-shadow: 0 0 6px var(--success);
  }

  :global(.status-glow-warning) {
    background: rgba(245, 158, 11, 0.08);
    color: var(--chart-amber);
    border: 1px solid rgba(245, 158, 11, 0.12);
  }

  :global(.status-glow-warning .status-dot-core) {
    background-color: var(--warning);
    box-shadow: 0 0 6px var(--warning);
  }

  :global(.status-glow-error) {
    background: rgba(239, 68, 68, 0.08);
    color: var(--error-text);
    border: 1px solid rgba(239, 68, 68, 0.12);
  }

  :global(.status-glow-error .status-dot-core) {
    background-color: var(--error);
    box-shadow: 0 0 6px var(--error);
  }

  :global(.badge-cache) {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: var(--radius-xs);
    font-weight: 500;
  }

  :global(.badge-cache--hit) {
    background: var(--success-muted);
    color: var(--success-text);
    border: 1px solid rgba(16, 185, 129, 0.15);
  }

  :global(.badge-cache--miss) {
    background: var(--bg-badge);
    color: var(--text-muted);
    border: 1px solid var(--border);
  }

  /* ── Error Banner ─────────────────────────────────────── */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: var(--error-muted);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md);
    color: var(--error-text);
    font-size: 13px;
    margin-top: 16px;
  }

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 1200px) {
    .breakdown-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 900px) {
    .gateway-performance-grid {
      grid-template-columns: 1fr;
      gap: 10px;
    }
    .metrics-grid {
      grid-template-columns: repeat(2, 1fr);
    }
    .breakdown-grid {
      grid-template-columns: 1fr;
    }
    .detail-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 640px) {
    .metrics-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
