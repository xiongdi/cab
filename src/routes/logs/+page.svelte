<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api';
  import type { RequestLog, PaginatedLogs, Column, LogFilter } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import DataTable from '$lib/components/DataTable.svelte';

  let logs = $state<RequestLog[]>([]);
  let total = $state(0);
  let page = $state(1);
  let perPage = $state(25);
  let totalPages = $state(0);
  let loading = $state(true);
  let autoRefresh = $state(false);
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // Filters
  let filterAgent = $state('');
  let filterProvider = $state('');
  let filterModel = $state('');
  let filterStatus = $state('');

  const columns: Column[] = [
    {
      key: 'timestamp',
      label: 'Timestamp',
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
    { key: 'agent', label: 'Agent', sortable: true },
    { key: 'provider', label: 'Provider', sortable: true },
    {
      key: 'model',
      label: 'Model',
      sortable: true,
      render: (v: string) => `<span class="mono" style="font-size:12px">${v}</span>`,
    },
    {
      key: 'input_tokens',
      label: 'In',
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v?.toLocaleString() ?? '0'}</span>`,
    },
    {
      key: 'output_tokens',
      label: 'Out',
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v?.toLocaleString() ?? '0'}</span>`,
    },
    {
      key: 'latency_ms',
      label: 'Latency',
      sortable: true,
      align: 'right' as const,
      render: (v: number) => `<span class="mono">${v}ms</span>`,
    },
    {
      key: 'status_code',
      label: 'Status',
      align: 'center' as const,
      render: (v: number) => {
        const cls = v < 300 ? 'badge-success' : v < 500 ? 'badge-warning' : 'badge-error';
        return `<span class="badge ${cls}">${v}</span>`;
      },
    },
  ];

  onMount(loadLogs);

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  function toggleAutoRefresh() {
    autoRefresh = !autoRefresh;
    if (autoRefresh) {
      refreshInterval = setInterval(loadLogs, 5000);
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

<PageHeader title="Logs" description="Request history and audit trail">
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
      {autoRefresh ? 'Auto-refresh ON' : 'Auto-refresh'}
    </button>
  {/snippet}
</PageHeader>

<!-- Filter Bar -->
<div class="filter-bar">
  <input
    class="input filter-input"
    type="text"
    placeholder="Filter agent…"
    bind:value={filterAgent}
    onchange={applyFilters}
  />
  <input
    class="input filter-input"
    type="text"
    placeholder="Filter provider…"
    bind:value={filterProvider}
    onchange={applyFilters}
  />
  <input
    class="input filter-input"
    type="text"
    placeholder="Filter model…"
    bind:value={filterModel}
    onchange={applyFilters}
  />
  <select class="select filter-input" bind:value={filterStatus} onchange={applyFilters}>
    <option value="">All status</option>
    <option value="2xx">2xx Success</option>
    <option value="4xx">4xx Client Error</option>
    <option value="5xx">5xx Server Error</option>
  </select>
  <button class="btn btn-ghost btn-sm" onclick={clearFilters}>Clear</button>
</div>

{#if loading && logs.length === 0}
  <div class="skeleton" style="height: 300px; border-radius: var(--radius-lg);"></div>
{:else}
  <DataTable {columns} data={logs} emptyMessage="No logs match your filters" />

  <!-- Pagination -->
  {#if totalPages > 1}
    <div class="pagination">
      <span class="page-info">
        Showing {(page - 1) * perPage + 1}–{Math.min(page * perPage, total)} of {total}
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
          Previous
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
          Next
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

<style>
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
    background: rgba(255, 255, 255, 0.04);
  }

  .page-btn.active {
    background: var(--accent-muted);
    color: var(--accent-text);
    border-color: rgba(59, 130, 246, 0.2);
  }
</style>
