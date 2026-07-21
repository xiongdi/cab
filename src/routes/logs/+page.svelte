<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api';
  import type {
    RequestLog,
    PaginatedLogs,
    Column,
    LogFilter,
    ToolWeightSnapshot,
  } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import DataTable from '$lib/components/DataTable.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { JsonView, darkStyles, allExpanded } from '@humanspeak/svelte-json-view-lite';
  import { themeManager } from '$lib/theme.svelte';

  let isDarkTheme = $derived(
    themeManager.current === 'system'
      ? (typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)').matches : true)
      : themeManager.current === 'dark'
  );
  let jsonViewStyle = $derived(isDarkTheme ? darkStyles : undefined);

  let logs = $state<RequestLog[]>([]);
  let total = $state(0);
  let page = $state(1);
  let perPage = $state(25);
  let totalPages = $state(0);
  let loading = $state(true);
  let autoRefresh = $state(false);
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let expandedLogId = $state<string | null>(null);
  let modalContent = $state<string | null>(null);
  let modalLabel = $state('');

  let modalData = $derived.by(() => {
    if (!modalContent) return null;
    try {
      return JSON.parse(modalContent);
    } catch {
      return null;
    }
  });

  function isRowExpanded(row: RequestLog): boolean {
    return expandedLogId === row.id;
  }

  function toggleRow(row: RequestLog) {
    expandedLogId = expandedLogId === row.id ? null : row.id;
  }

  function openBodyModal(body: string | undefined | null, label: string) {
    if (!body) return;
    modalContent = formatJson(body);
    modalLabel = label;
  }

  function closeBodyModal() {
    modalContent = null;
    modalLabel = '';
  }

  function formatTimestamp(ts: string): string {
    try {
      const d = new Date(ts);
      return d.toLocaleString();
    } catch {
      return ts;
    }
  }

  function timeAgo(ts: string): string {
    try {
      const diff = Date.now() - new Date(ts).getTime();
      const secs = Math.floor(diff / 1000);
      if (secs < 60) return `${secs}s ago`;
      const mins = Math.floor(secs / 60);
      if (mins < 60) return `${mins}m ago`;
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return `${hrs}h ago`;
      const days = Math.floor(hrs / 24);
      return `${days}d ago`;
    } catch {
      return '';
    }
  }

  function formatJson(raw: string | undefined | null): string {
    if (!raw) return '';
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }

  // Tool-schema weight diagnostics (cache prefix).
  let toolWeights = $state<ToolWeightSnapshot[]>([]);

  // Filters
  let filterAgent = $state('');
  let filterProvider = $state('');
  let filterModel = $state('');
  let filterStatus = $state('');

  const columns = $derived.by((): Column[] => {
    void i18n.currentLang;
    return [
    {
      key: 'timestamp',
      label: i18n.t('logs.time'),
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
    { key: 'agent', label: i18n.t('logs.agent'), sortable: true },
    { key: 'provider', label: i18n.t('logs.provider'), sortable: true },
    {
      key: 'model',
      label: i18n.t('logs.model'),
      sortable: true,
      render: (v: string) => `<span class="mono" style="font-size:12px">${v}</span>`,
    },
    {
      key: 'input_tokens',
      label: i18n.t('dashboard.inputs'),
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v?.toLocaleString() ?? '0'}</span>`,
    },
    {
      key: 'output_tokens',
      label: i18n.t('dashboard.outputs'),
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v?.toLocaleString() ?? '0'}</span>`,
    },
    {
      key: 'cache_read_tokens',
      label: i18n.t('logs.cache_hit'),
      sortable: true,
      align: 'right' as const,
      render: (v: number, row: RequestLog) => {
        const cacheRead = v ?? 0;
        const total = (row.input_tokens ?? 0);
        if (total <= 0 || cacheRead <= 0) {
          return '<span class="mono" style="color:var(--text-muted)">—</span>';
        }
        const pct = (cacheRead / total) * 100;
        const color = pct >= 50 ? 'var(--success)' : pct > 0 ? 'var(--text-secondary)' : 'var(--text-muted)';
        const tooltip = i18n
          .tParams('logs.cache_hit_tooltip', {
            cached: cacheRead.toLocaleString(),
            total: total.toLocaleString(),
          })
          .replace(/"/g, '&quot;');
        return `<span class="mono" style="color:${color}" title="${tooltip}">${pct.toFixed(2)}%</span>`;
      },
    },
    {
      key: 'latency_ms',
      label: i18n.t('logs.latency'),
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v}ms</span>`,
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

  onMount(() => {
    loadLogs();
    loadToolWeights();
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  function toggleAutoRefresh() {
    autoRefresh = !autoRefresh;
    if (autoRefresh) {
      refreshInterval = setInterval(() => {
        loadLogs();
        loadToolWeights();
      }, 5000);
    } else {
      if (refreshInterval) clearInterval(refreshInterval);
      refreshInterval = null;
    }
  }

  async function loadLogs() {
    loading = true;
    try {
      const filter: LogFilter = {
        page,
        per_page: perPage,
      };
      if (filterAgent) filter.agent = filterAgent;
      if (filterProvider) filter.provider = filterProvider;
      if (filterModel) filter.model = filterModel;
      if (filterStatus) filter.status = filterStatus;

      const result: PaginatedLogs = await api.logs.list(filter);
      logs = result.data;
      total = result.total;
      totalPages = result.total_pages;
    } catch {
      logs = [];
      total = 0;
      totalPages = 0;
    } finally {
      loading = false;
    }
  }

  async function loadToolWeights() {
    try {
      toolWeights = await api.diagnostics.toolWeights();
    } catch {
      toolWeights = [];
    }
  }

  function capturedAgo(ms: number): string {
    const secs = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (secs < 60) return i18n.tParams('logs.tool_weights_ago_s', { n: secs });
    const mins = Math.round(secs / 60);
    if (mins < 60) return i18n.tParams('logs.tool_weights_ago_m', { n: mins });
    const hrs = Math.round(mins / 60);
    return i18n.tParams('logs.tool_weights_ago_h', { n: hrs });
  }

  function goPage(p: number) {
    page = p;
    loadLogs();
  }

  function applyFilters() {
    page = 1;
    loadLogs();
  }

  function clearFilters() {
    filterAgent = '';
    filterProvider = '';
    filterModel = '';
    filterStatus = '';
    page = 1;
    loadLogs();
  }
</script>

<PageHeader title={i18n.t('logs.title')} description={i18n.t('logs.subtitle')}>
  {#snippet children()}
    <button class="btn {autoRefresh ? 'btn-primary' : 'btn-secondary'}" onclick={toggleAutoRefresh}>
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path
          d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
        />
      </svg>
      {autoRefresh ? i18n.t('logs.auto_refresh_on') : i18n.t('logs.auto_refresh')}
    </button>
  {/snippet}
</PageHeader>

<!-- Filter Bar -->
<div class="filter-bar">
  <input
    class="input filter-input"
    type="text"
    placeholder={i18n.t('logs.filter_agent_ph')}
    bind:value={filterAgent}
    onchange={applyFilters}
  />
  <input
    class="input filter-input"
    type="text"
    placeholder={i18n.t('logs.filter_provider_ph')}
    bind:value={filterProvider}
    onchange={applyFilters}
  />
  <input
    class="input filter-input"
    type="text"
    placeholder={i18n.t('logs.filter_model_ph')}
    bind:value={filterModel}
    onchange={applyFilters}
  />
  <select class="select filter-input" bind:value={filterStatus} onchange={applyFilters}>
    <option value="">{i18n.t('logs.all_status')}</option>
    <option value="2xx">{i18n.t('logs.status_2xx')}</option>
    <option value="4xx">{i18n.t('logs.status_4xx')}</option>
    <option value="5xx">{i18n.t('logs.status_5xx')}</option>
  </select>
  <button class="btn btn-ghost btn-sm" onclick={clearFilters}>{i18n.t('common.clear')}</button>
</div>

{#if toolWeights.length > 0}
  <details class="weights-panel">
    <summary>
      <span class="weights-title">{i18n.t('logs.tool_weights_title')}</span>
      <span class="weights-hint">{i18n.t('logs.tool_weights_hint')}</span>
    </summary>
    <div class="weights-body">
      {#each toolWeights as snap (snap.agent)}
        {@const max = snap.tools[0]?.tokens ?? 0}
        <div class="weights-agent">
          <div class="weights-agent-head">
            <span class="weights-agent-name mono">{snap.agent}</span>
            <span class="weights-agent-meta">
              {i18n.tParams('logs.tool_weights_summary', {
                count: snap.tool_count,
                tokens: snap.total_tokens.toLocaleString(),
              })}
              · {capturedAgo(snap.captured_at_ms)}
            </span>
          </div>
          <ul class="weights-list">
            {#each snap.tools.slice(0, 12) as tool (tool.name)}
              <li class="weights-row">
                <span class="weights-name mono" title={tool.name}>{tool.name}</span>
                <span class="weights-bar-track">
                  <span
                    class="weights-bar-fill"
                    style="width: {max > 0 ? Math.max(2, Math.round((tool.tokens / max) * 100)) : 0}%"
                  ></span>
                </span>
                <span class="weights-tokens mono">~{tool.tokens.toLocaleString()}</span>
              </li>
            {/each}
          </ul>
          {#if snap.tools.length > 12}
            <div class="weights-more">
              {i18n.tParams('logs.tool_weights_more', { n: snap.tools.length - 12 })}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </details>
{/if}

{#if loading && logs.length === 0}
  <div class="skeleton" style="height: 300px; border-radius: var(--radius-lg);"></div>
{:else}
  {#snippet expandedRow(row: RequestLog)}
    <div class="log-detail-panel">
      <div class="detail-grid">
        <div class="detail-section">
          <div class="detail-section-title">{i18n.t('logs.time')}</div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.detail_timestamp')}</span>
            <span class="detail-value mono">{formatTimestamp(row.timestamp)}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('common.relative')}</span>
            <span class="detail-value mono">{timeAgo(row.timestamp)}</span>
          </div>
        </div>

        <div class="detail-section">
          <div class="detail-section-title">{i18n.t('logs.detail_routing')}</div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.agent')}</span>
            <span class="detail-value">{row.agent}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.provider')}</span>
            <span class="detail-value">{row.provider}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.model')}</span>
            <span class="detail-value mono">{row.model}</span>
          </div>
        </div>

        <div class="detail-section">
          <div class="detail-section-title">{i18n.t('logs.detail_tokens')}</div>
          <!-- Token breakdown capsule bar -->
          <div class="token-ratio-bar">
            {#if (row.input_tokens || 0) > 0}
              <div class="bar-segment input" style="flex: {row.input_tokens};" title={i18n.t('logs.detail_input_tokens_title')}></div>
            {/if}
            {#if (row.cache_read_tokens || 0) > 0}
              <div class="bar-segment cache" style="flex: {row.cache_read_tokens};" title={i18n.t('logs.detail_cache_read_title')}></div>
            {/if}
            {#if (row.output_tokens || 0) > 0}
              <div class="bar-segment output" style="flex: {row.output_tokens};" title={i18n.t('logs.detail_output_tokens_title')}></div>
            {/if}
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('dashboard.inputs')}</span>
            <span class="detail-value mono">{row.input_tokens?.toLocaleString() ?? '0'}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('dashboard.outputs')}</span>
            <span class="detail-value mono">{row.output_tokens?.toLocaleString() ?? '0'}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.cache_hit')}</span>
            <span class="detail-value mono">{row.cache_read_tokens?.toLocaleString() ?? '0'}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.detail_cache_creation')}</span>
            <span class="detail-value mono">{row.cache_creation_tokens?.toLocaleString() ?? '0'}</span>
          </div>
          <div class="detail-row detail-row--total">
            <span class="detail-label">{i18n.t('common.total')}</span>
            <span class="detail-value mono">{row.total_tokens?.toLocaleString() ?? '0'}</span>
          </div>
        </div>

        <div class="detail-section">
          <div class="detail-section-title">{i18n.t('logs.detail_performance')}</div>
          <div class="detail-row">
            <span class="detail-label">{i18n.t('logs.latency')}</span>
            <span class="detail-value mono">{row.latency_ms}ms</span>
          </div>
          {#if (row.output_tokens || 0) > 0 && (row.latency_ms || 0) > 0}
            <div class="detail-row">
              <span class="detail-label">{i18n.t('common.speed')}</span>
              <span class="detail-value mono detail-speed">
                {i18n.tParams('common.tokens_per_sec', {
                  value: ((row.output_tokens ?? 0) / ((row.latency_ms ?? 1) / 1000)).toFixed(1),
                })}
              </span>
            </div>
          {/if}
          <div class="detail-row">
            <span class="detail-label">{i18n.t('common.status')}</span>
            <span class="detail-value">
              <span class="badge {row.status_code < 300 ? 'badge-success' : row.status_code < 500 ? 'badge-warning' : 'badge-error'}">{row.status_code}</span>
            </span>
          </div>
        </div>
      </div>

      {#if row.error_message}
        <div class="terminal-error-diagnoser">
          <div class="diag-header">
            <span class="diag-dot red"></span>
            <span class="diag-dot yellow"></span>
            <span class="diag-dot green"></span>
            <span class="diag-title mono">{i18n.t('logs.diag_terminal_title')}</span>
          </div>
          <pre class="diag-content mono"><code>{i18n.tParams('logs.diag_error_prefix', { code: row.status_code })}
{row.error_message}</code></pre>
        </div>
      {/if}

      <div class="detail-bodies">
        {#if row.request_body}
          <button class="body-btn" onclick={() => openBodyModal(row.request_body, i18n.t('logs.detail_request_body'))}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4M6 12h12" />
            </svg>
            <span>{i18n.t('logs.detail_request_body')}</span>
            <svg width="12" height="12" class="body-btn-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M9 18l6-6-6-6" />
            </svg>
          </button>
        {/if}
        {#if row.response_body}
          <button class="body-btn" onclick={() => openBodyModal(row.response_body, i18n.t('logs.detail_response_body'))}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
              <path d="M14 4l-4 16m-4-4l-4-4 4-4M18 16l4-4-4-4" />
            </svg>
            <span>{i18n.t('logs.detail_response_body')}</span>
            <svg width="12" height="12" class="body-btn-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M9 18l6-6-6-6" />
            </svg>
          </button>
        {/if}
      </div>
    </div>
  {/snippet}

  <DataTable
    {columns}
    data={logs}
    emptyMessage={i18n.t('logs.empty_filtered')}
    searchPlaceholder={i18n.t('common.search')}
    {expandedRow}
    {isRowExpanded}
    onRowClick={toggleRow}
  />

  <!-- Pagination -->
  {#if totalPages > 1}
    <div class="pagination">
      <span class="page-info">
        {i18n.tParams('logs.page_range', {
          start: (page - 1) * perPage + 1,
          end: Math.min(page * perPage, total),
          total,
        })}
      </span>
      <div class="page-controls">
        <button class="btn btn-ghost btn-sm" disabled={page <= 1} onclick={() => goPage(page - 1)}>
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M15 19l-7-7 7-7" />
          </svg>
          {i18n.t('common.previous')}
        </button>
        {#each Array(Math.min(totalPages, 7)) as _, i}
          {@const p =
            totalPages <= 7
              ? i + 1
              : page <= 4
                ? i + 1
                : page >= totalPages - 3
                  ? totalPages - 6 + i
                  : page - 3 + i}
          <button class="page-btn" class:active={p === page} onclick={() => goPage(p)}>
            {p}
          </button>
        {/each}
        <button
          class="btn btn-ghost btn-sm"
          disabled={page >= totalPages}
          onclick={() => goPage(page + 1)}
        >
          {i18n.t('common.next')}
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
    </div>
  {/if}
{/if}

<!-- Body JSON Modal -->
{#if modalContent}
  <div class="modal-overlay" onclick={closeBodyModal} role="presentation">
    <div class="modal-container" onclick={(e) => e.stopPropagation()} role="dialog" aria-label={modalLabel}>
      <div class="modal-header">
        <span class="modal-label">{modalLabel}</span>
        <button class="modal-close-btn" onclick={closeBodyModal}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
      <div class="modal-scroll">
        {#if modalData}
          <JsonView data={modalData} style={jsonViewStyle} shouldExpandNode={allExpanded} />
        {:else}
          <pre class="modal-code"><code>{modalContent}</code></pre>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Token Ratio Bar ────────────────────────────────── */
  .token-ratio-bar {
    display: flex;
    height: 6px;
    border-radius: var(--radius-full);
    overflow: hidden;
    background: var(--bg-badge);
    border: 1px solid var(--border);
    margin-bottom: 12px;
    margin-top: 4px;
    width: 100%;
  }

  .bar-segment {
    height: 100%;
    transition: width var(--transition-normal);
  }

  .bar-segment.input {
    background: var(--chart-blue-strong); /* Blue input */
  }

  .bar-segment.cache {
    background: var(--success); /* Green cache read */
  }

  .bar-segment.output {
    background: var(--chart-purple); /* Purple output */
  }

  /* ── Terminal Diagnostic box ───────────────────────── */
  .terminal-error-diagnoser {
    background: var(--bg-terminal);
    border: 1px solid rgba(239, 68, 68, 0.15);
    border-radius: var(--radius-md);
    overflow: hidden;
    margin: 16px 0;
    box-shadow: 0 4px 20px rgba(239, 68, 68, 0.05);
  }

  .diag-header {
    background: var(--glass-bg-subtle);
    border-bottom: 1px solid var(--border);
    padding: 8px 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .diag-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
  }

  .diag-dot.red { background: var(--error); }
  .diag-dot.yellow { background: var(--warning); }
  .diag-dot.green { background: var(--success); }

  .diag-title {
    font-size: 10.5px;
    color: var(--text-muted);
    margin-left: 6px;
  }

  .diag-content {
    margin: 0;
    padding: 14px;
    overflow-x: auto;
  }

  .diag-content code {
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--error-text-soft);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .filter-bar {
    display: flex;
    gap: 8px;
    margin-bottom: 20px;
    flex-wrap: wrap;
  }

  .filter-input {
    max-width: 180px;
    font-size: 12px;
    padding: 7px 10px;
  }

  .pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 16px;
    padding: 12px 0;
  }

  .page-info {
    font-size: 12px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .page-controls {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .page-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: var(--font-mono);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .page-btn:hover {
    background: var(--glass-bg-hover);
  }

  .page-btn.active {
    background: var(--accent-muted);
    color: var(--accent-text);
    border-color: rgba(59, 130, 246, 0.2);
  }

  .weights-panel {
    margin-bottom: 16px;
    border: 1px solid var(--border-color, var(--border-hover));
    border-radius: var(--radius-lg);
    background: var(--surface-1, var(--bg-badge));
    overflow: hidden;
  }

  .weights-panel summary {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 12px 16px;
    cursor: pointer;
    user-select: none;
    list-style: none;
  }

  .weights-panel summary::-webkit-details-marker {
    display: none;
  }

  .weights-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .weights-hint {
    font-size: 11px;
    color: var(--text-muted);
  }

  .weights-body {
    padding: 4px 16px 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .weights-agent-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
  }

  .weights-agent-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .weights-agent-meta {
    font-size: 11px;
    color: var(--text-muted);
  }

  .weights-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .weights-row {
    display: grid;
    grid-template-columns: minmax(120px, 220px) 1fr auto;
    align-items: center;
    gap: 10px;
  }

  .weights-name {
    font-size: 12px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .weights-bar-track {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-raised);
    overflow: hidden;
  }

  .weights-bar-fill {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent, var(--chart-blue-strong));
  }

  .weights-tokens {
    font-size: 11px;
    color: var(--text-muted);
    text-align: right;
    min-width: 64px;
  }

  .weights-more {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }

  /* ── Expanded Log Detail Panel ─────────────────────────── */
  .log-detail-panel {
    background: var(--bg-badge);
    border-top: 1px solid var(--border);
    padding: 16px 20px;
    animation: fadeIn 0.15s ease-out;
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
  }

  .detail-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .detail-section-title {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin-bottom: 4px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--surface-raised);
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .detail-row--total {
    padding-top: 4px;
    border-top: 1px solid var(--surface-raised);
  }

  .detail-label {
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .detail-value {
    font-size: 12px;
    color: var(--text-primary);
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail-speed {
    color: var(--accent);
  }

  .detail-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 12px;
    padding: 10px 12px;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.15);
    border-radius: var(--radius-md);
    color: var(--error-text-soft);
    font-size: 12px;
    line-height: 1.5;
  }

  .detail-error svg {
    margin-top: 1px;
    flex-shrink: 0;
    color: var(--error);
  }

  /* ── Request / Response Body Buttons ─────────────────────── */
  .detail-bodies {
    margin-top: 10px;
    display: flex;
    gap: 8px;
  }

  .body-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--border-dashed-subtle);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all var(--transition-fast);
    font-family: var(--font-sans);
  }

  .body-btn:hover {
    background: var(--badge-neutral-bg);
    border-color: var(--border-hover);
    color: var(--text-primary);
  }

  .body-btn-arrow {
    opacity: 0.4;
    transition: transform var(--transition-fast);
  }

  .body-btn:hover .body-btn-arrow {
    transform: translateX(2px);
    opacity: 0.8;
  }

  /* ── Body JSON Modal ─────────────────────────────────────── */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    animation: fadeIn 0.12s ease-out;
    padding: 24px;
  }

  .modal-container {
    width: 100%;
    max-width: 800px;
    max-height: 80vh;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: modalIn 0.15s ease-out;
  }

  @keyframes modalIn {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .modal-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .modal-close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .modal-close-btn:hover {
    background: var(--badge-neutral-bg);
    color: var(--text-primary);
  }

  .modal-scroll {
    overflow: auto;
    padding: 4px 0 4px 4px;
    flex: 1;
  }

  .modal-scroll :global(div[role='tree']) {
    --sjv-background: transparent;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.65;
    padding: 12px 16px;
  }

  .modal-code {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.65;
    color: var(--text-primary);
    white-space: pre;
    word-wrap: normal;
    padding: 12px 16px;
  }

  @media (max-width: 900px) {
    .detail-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 500px) {
    .detail-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
