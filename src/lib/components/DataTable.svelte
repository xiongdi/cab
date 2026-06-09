<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Column } from '$lib/types';
  import EmptyState from './EmptyState.svelte';

  import { i18n } from '$lib/i18n.svelte';

  let {
    columns,
    data,
    emptyMessage,
    emptyIcon = 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4',
    rowActions,
    showSearch = true,
    pageSize = 50,
    searchPlaceholder,
    ontoggleStatus,
    expandedRow,
    isRowExpanded,
    onRowClick,
  }: {
    columns: Column[];
    data: any[];
    emptyMessage?: string;
    emptyIcon?: string;
    rowActions?: Snippet<[any]>;
    showSearch?: boolean;
    pageSize?: number;
    searchPlaceholder?: string;
    ontoggleStatus?: (row: any) => void;
    expandedRow?: Snippet<[any]>;
    isRowExpanded?: (row: any) => boolean;
    onRowClick?: (row: any) => void;
  } = $props();

  let sortKey = $state('');
  let sortDir = $state<'asc' | 'desc'>('asc');
  let searchQuery = $state('');
  let currentPage = $state(1);

  // Advanced Category Filter States
  let selectedProvider = $state('all');
  let selectedProtocol = $state('all');
  let selectedStatus = $state('all');
  let selectedContext = $state('all');
  let selectedPrice = $state('all');

  // Reset currentPage reactively when any filter changes
  $effect(() => {
    void searchQuery;
    void selectedProvider;
    void selectedProtocol;
    void selectedStatus;
    void selectedContext;
    void selectedPrice;
    currentPage = 1;
  });

  function handleSort(key: string) {
    if (sortKey === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortKey = key;
      sortDir = 'asc';
    }
  }

  function clearAllFilters() {
    searchQuery = '';
    selectedProvider = 'all';
    selectedProtocol = 'all';
    selectedStatus = 'all';
    selectedContext = 'all';
    selectedPrice = 'all';
  }

  // Dynamically extract unique provider options from the actual data rows
  let uniqueProviders = $derived.by(() => {
    const set = new Set<string>();
    for (const item of data) {
      const p = item.provider_name || item.provider || item.provider_id;
      if (p) set.add(p);
    }
    return Array.from(set).sort();
  });

  // Tokenized fuzzy matching function
  function tokenMatchRow(row: any, query: string): boolean {
    if (!query) return true;
    const tokens = query
      .toLowerCase()
      .split(/\s+/)
      .filter((t) => t.length > 0);
    if (tokens.length === 0) return true;

    // EVERY token must match AT LEAST ONE field in the row
    return tokens.every((token) => {
      return (
        columns.some((col) => {
          const val = row[col.key];
          if (val == null) return false;
          return String(val).toLowerCase().includes(token);
        }) ||
        (row.name && String(row.name).toLowerCase().includes(token)) ||
        (row.id && String(row.id).toLowerCase().includes(token)) ||
        (row.display_name && String(row.display_name).toLowerCase().includes(token)) ||
        (row.provider_name && String(row.provider_name).toLowerCase().includes(token))
      );
    });
  }

  // Advanced reactive filtering logic
  let filteredData = $derived.by(() => {
    return data.filter((row) => {
      // 1. Text Search matching (tokenized fuzzy)
      if (!tokenMatchRow(row, searchQuery)) return false;

      // 2. Provider Filter
      if (selectedProvider !== 'all') {
        const p = row.provider_name || row.provider || row.provider_id;
        if (p !== selectedProvider) return false;
      }

      // 3. Protocol Filter
      if (selectedProtocol !== 'all') {
        if (row.protocol !== selectedProtocol) return false;
      }

      // 4. Status Filter
      if (selectedStatus !== 'all') {
        const isActive = row.enabled === 1 || row.enabled === true;
        const wantActive = selectedStatus === 'active';
        if (isActive !== wantActive) return false;
      }

      // 5. Context Limit Filter
      if (selectedContext !== 'all') {
        const len = row.context_length || 0;
        if (selectedContext === '8k' && len < 8192) return false;
        if (selectedContext === '32k' && len < 32768) return false;
        if (selectedContext === '128k' && len < 131072) return false;
      }

      // 6. Price Range Filter (input_cost per 1M tokens)
      if (selectedPrice !== 'all') {
        const cost = row.input_cost ?? 0;
        if (selectedPrice === 'free' && cost > 0) return false;
        if (selectedPrice === 'budget' && (cost <= 0 || cost > 1)) return false;
        if (selectedPrice === 'mid' && (cost <= 1 || cost > 10)) return false;
        if (selectedPrice === 'premium' && cost <= 10) return false;
      }

      return true;
    });
  });

  // Sort filtered data
  let sortedData = $derived.by(() => {
    if (!sortKey) return filteredData;
    return [...filteredData].sort((a, b) => {
      const aVal = a[sortKey];
      const bVal = b[sortKey];
      if (aVal == null) return 1;
      if (bVal == null) return -1;

      if (typeof aVal === 'string') {
        return sortDir === 'asc' ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal);
      } else {
        return sortDir === 'asc' ? aVal - bVal : bVal - aVal;
      }
    });
  });

  // Pagination states and variables
  let totalPages = $derived(Math.max(1, Math.ceil(sortedData.length / pageSize)));
  let paginatedData = $derived.by(() => {
    if (pageSize <= 0) return sortedData;
    const start = (currentPage - 1) * pageSize;
    return sortedData.slice(start, start + pageSize);
  });

  // Calculate visible page range dynamically with ellipses
  function getVisiblePages(current: number, total: number) {
    const pages: number[] = [];
    if (total <= 5) {
      for (let i = 1; i <= total; i++) pages.push(i);
    } else {
      pages.push(1);
      if (current > 3) {
        pages.push(-1); // -1 signifies ellipsis
      }
      const start = Math.max(2, current - 1);
      const end = Math.min(total - 1, current + 1);
      for (let i = start; i <= end; i++) {
        if (!pages.includes(i)) pages.push(i);
      }
      if (current < total - 2) {
        pages.push(-1); // -1 signifies ellipsis
      }
      if (!pages.includes(total)) {
        pages.push(total);
      }
    }
    return pages;
  }
</script>

<div class="data-table-container">
  {#if showSearch && data.length > 0}
    <div class="search-bar-container fade-in">
      <div class="search-input-wrapper">
        <svg
          class="search-icon"
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          type="text"
          class="input search-input"
          placeholder={searchPlaceholder ?? i18n.t('datatable.search_placeholder')}
          bind:value={searchQuery}
        />
        {#if searchQuery}
          <button
            class="clear-btn"
            onclick={() => (searchQuery = '')}
            aria-label={i18n.t('datatable.clear_search')}
            type="button"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        {/if}
      </div>

      <!-- Linear-style Category Selectors -->
      <div class="filters-wrapper">
        <!-- Provider Filter Dropdown -->
        {#if uniqueProviders.length > 1}
          <select class="select filter-select" bind:value={selectedProvider}>
            <option value="all">{i18n.t('datatable.all_providers')}</option>
            {#each uniqueProviders as provider}
              <option value={provider}>{provider}</option>
            {/each}
          </select>
        {/if}

        <!-- Protocol Filter Dropdown -->
        {#if columns.some((c) => c.key === 'protocol')}
          <select class="select filter-select" bind:value={selectedProtocol}>
            <option value="all">{i18n.t('datatable.all_protocols')}</option>
            <option value="openai">{i18n.t('datatable.protocol_openai')}</option>
            <option value="anthropic">{i18n.t('datatable.protocol_anthropic')}</option>
          </select>
        {/if}

        <!-- Context Limit Range Filter Dropdown -->
        {#if columns.some((c) => c.key === 'context_length')}
          <select class="select filter-select" bind:value={selectedContext}>
            <option value="all">{i18n.t('datatable.all_context')}</option>
            <option value="8k">≥ 8K</option>
            <option value="32k">≥ 32K</option>
            <option value="128k">≥ 128K</option>
          </select>
        {/if}

        <!-- Price Range Filter -->
        {#if columns.some((c) => c.key === 'input_cost')}
          <select class="select filter-select" bind:value={selectedPrice}>
            <option value="all">{i18n.t('datatable.all_price')}</option>
            <option value="free">{i18n.t('datatable.price_free')}</option>
            <option value="budget">{i18n.t('datatable.price_budget')}</option>
            <option value="mid">{i18n.t('datatable.price_mid')}</option>
            <option value="premium">{i18n.t('datatable.price_premium')}</option>
          </select>
        {/if}

        <!-- Status Filter Dropdown -->
        <select class="select filter-select" bind:value={selectedStatus}>
          <option value="all">{i18n.t('datatable.all_status')}</option>
          <option value="active">{i18n.t('datatable.status_active')}</option>
          <option value="inactive">{i18n.t('datatable.status_inactive')}</option>
        </select>
      </div>
    </div>

    <!-- Active Filter Chips Bar -->
    {#if searchQuery || selectedProvider !== 'all' || selectedProtocol !== 'all' || selectedStatus !== 'all' || selectedContext !== 'all'}
      <div class="active-filters-chips fade-in">
        <span class="chips-label text-muted">{i18n.t('datatable.filters')}</span>

        {#if searchQuery}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_query')}:</span>
            <span class="chip-val">{searchQuery}</span>
            <button class="chip-remove" onclick={() => (searchQuery = '')}>✕</button>
          </div>
        {/if}

        {#if selectedProvider !== 'all'}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_provider')}:</span>
            <span class="chip-val">{selectedProvider}</span>
            <button class="chip-remove" onclick={() => (selectedProvider = 'all')}>✕</button>
          </div>
        {/if}

        {#if selectedProtocol !== 'all'}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_protocol')}:</span>
            <span class="chip-val">{selectedProtocol}</span>
            <button class="chip-remove" onclick={() => (selectedProtocol = 'all')}>✕</button>
          </div>
        {/if}

        {#if selectedContext !== 'all'}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_context')}:</span>
            <span class="chip-val">≥ {selectedContext.toUpperCase()}</span>
            <button class="chip-remove" onclick={() => (selectedContext = 'all')}>✕</button>
          </div>
        {/if}

        {#if selectedPrice !== 'all'}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_price')}:</span>
            <span class="chip-val"
              >{selectedPrice === 'free'
                ? i18n.t('datatable.price_free')
                : selectedPrice === 'budget'
                  ? i18n.t('datatable.price_budget')
                  : selectedPrice === 'mid'
                    ? i18n.t('datatable.price_mid')
                    : i18n.t('datatable.price_premium')}</span
            >
            <button class="chip-remove" onclick={() => (selectedPrice = 'all')}>✕</button>
          </div>
        {/if}

        {#if selectedStatus !== 'all'}
          <div class="filter-chip">
            <span class="chip-key">{i18n.t('datatable.filter_status')}:</span>
            <span class="chip-val"
              >{selectedStatus === 'active'
                ? i18n.t('datatable.status_active')
                : i18n.t('datatable.status_inactive')}</span
            >
            <button class="chip-remove" onclick={() => (selectedStatus = 'all')}>✕</button>
          </div>
        {/if}

        <button class="btn btn-ghost btn-sm clear-all-btn" onclick={clearAllFilters} type="button">
          {i18n.t('datatable.clear_all')}
        </button>

        <div style="flex-grow:1"></div>

        <div class="search-results-count mono">
          {i18n.tParams('datatable.found_count', {
            found: filteredData.length,
            total: data.length,
          })}
        </div>
      </div>
    {/if}
  {/if}

  {#if filteredData.length === 0}
    <EmptyState message={emptyMessage ?? i18n.t('datatable.empty')} icon={emptyIcon} />
  {:else}
    <div class="table-wrapper">
      <table>
        <thead>
          <tr>
            {#each columns as col}
              <th
                style:width={col.width}
                style:text-align={col.align ?? 'left'}
                class:sortable={col.sortable}
                onclick={() => col.sortable && handleSort(col.key)}
              >
                <span class="th-content">
                  {col.label}
                  {#if col.sortable && sortKey === col.key}
                    <svg
                      class="sort-icon"
                      width="10"
                      height="10"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.5"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    >
                      {#if sortDir === 'asc'}
                        <path d="M18 15l-6-6-6 6" />
                      {:else}
                        <path d="M6 9l6 6 6-6" />
                      {/if}
                    </svg>
                  {/if}
                </span>
              </th>
            {/each}
            {#if rowActions}
              <th style:width="100px" style:text-align="right">{i18n.t('datatable.actions')}</th>
            {/if}
          </tr>
        </thead>
        <tbody>
          {#each paginatedData as row, i}
            <tr
              class="fade-in"
              class:clickable={onRowClick}
              style="animation-delay: {Math.min(i * 15, 150)}ms"
              onclick={(e) => {
                const target = e.target as HTMLElement;
                if (target.closest('a, button, input, select, label')) {
                  return;
                }
                if (onRowClick) onRowClick(row);
              }}
            >
              {#each columns as col}
                <td style:text-align={col.align ?? 'left'}>
                  {#if (col.key === 'enabled' || col.key === 'effective_enabled') && ontoggleStatus}
                    <label
                      class="toggle"
                      style="transform: scale(0.85); display: inline-block;"
                      onclick={(e) => e.stopPropagation()}
                      role="presentation"
                    >
                      <input
                        type="checkbox"
                        checked={row[col.key]}
                        onchange={() => ontoggleStatus(row)}
                      />
                      <span class="toggle-slider"></span>
                    </label>
                  {:else if col.render}
                    {@html col.render(row[col.key], row)}
                  {:else}
                    {row[col.key] ?? '—'}
                  {/if}
                </td>
              {/each}
              {#if rowActions}
                <td style:text-align="right" onclick={(e) => e.stopPropagation()}>
                  {@render rowActions(row)}
                </td>
              {/if}
            </tr>
            {#if expandedRow && isRowExpanded && isRowExpanded(row)}
              <tr class="expanded-row-tr">
                <td
                  colspan={columns.length + (rowActions ? 1 : 0)}
                  style="padding: 0; border-top: none;"
                >
                  {@render expandedRow(row)}
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>

    <!-- Premium Pagination Controls -->
    {#if pageSize > 0 && totalPages > 1}
      <div class="pagination-container fade-in">
        <div class="pagination-info">
          {i18n.tParams('datatable.showing_range', {
            start: Math.min(filteredData.length, (currentPage - 1) * pageSize + 1),
            end: Math.min(filteredData.length, currentPage * pageSize),
            total: filteredData.length,
          })}
        </div>
        <div class="pagination-buttons">
          <button
            class="btn btn-secondary btn-sm"
            disabled={currentPage === 1}
            onclick={() => (currentPage = Math.max(1, currentPage - 1))}
            type="button"
          >
            {i18n.t('common.previous')}
          </button>

          <div class="pagination-pages">
            {#each getVisiblePages(currentPage, totalPages) as page}
              {#if page === -1}
                <span class="pagination-ellipsis">...</span>
              {:else}
                <button
                  class="btn btn-sm page-num-btn"
                  class:active-page={currentPage === page}
                  onclick={() => (currentPage = page)}
                  type="button"
                >
                  {page}
                </button>
              {/if}
            {/each}
          </div>

          <button
            class="btn btn-secondary btn-sm"
            disabled={currentPage === totalPages}
            onclick={() => (currentPage = Math.min(totalPages, currentPage + 1))}
            type="button"
          >
            {i18n.t('common.next')}
          </button>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .data-table-container {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }

  .search-bar-container {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .search-input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    min-width: 320px;
    flex-grow: 1;
    max-width: 480px;
  }

  .search-icon {
    position: absolute;
    left: 12px;
    color: var(--text-muted);
    pointer-events: none;
  }

  .search-input {
    padding-left: 36px;
    padding-right: 32px;
    height: 38px;
    border-radius: var(--radius-md);
  }

  .clear-btn {
    position: absolute;
    right: 10px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    border-radius: var(--radius-xs);
    transition: all var(--transition-fast);
  }

  .clear-btn:hover {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.05);
  }

  .filters-wrapper {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .filter-select {
    width: auto !important;
    min-width: 130px;
    height: 38px;
    padding: 8px 30px 8px 12px;
    border-radius: var(--radius-md);
    background-color: var(--bg-input);
    border-color: var(--border);
    font-size: 12px;
    color: var(--text-secondary);
  }

  .filter-select:hover {
    color: var(--text-primary);
  }

  .active-filters-chips {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 4px 0;
  }

  .chips-label {
    font-size: 12px;
    margin-right: 4px;
  }

  .filter-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: var(--accent-muted);
    border: 1px solid rgba(59, 130, 246, 0.2);
    color: var(--accent-text);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .chip-key {
    opacity: 0.7;
    font-weight: 500;
  }

  .chip-val {
    font-weight: 600;
  }

  .chip-remove {
    background: transparent;
    border: none;
    color: var(--accent-text);
    font-size: 10px;
    cursor: pointer;
    padding: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    margin-left: 2px;
    opacity: 0.8;
    transition: all var(--transition-fast);
  }

  .chip-remove:hover {
    opacity: 1;
    background: rgba(59, 130, 246, 0.15);
  }

  .clear-all-btn {
    font-size: 11px !important;
    padding: 4px 8px !important;
    color: var(--text-muted) !important;
    cursor: pointer;
  }

  .clear-all-btn:hover {
    color: var(--text-primary) !important;
  }

  .search-results-count {
    font-size: 12px;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-full);
  }

  .sortable {
    cursor: pointer;
    user-select: none;
  }

  .clickable {
    cursor: pointer;
  }

  .sortable:hover {
    color: var(--text-secondary);
  }

  .th-content {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .sort-icon {
    opacity: 0.6;
  }

  .pagination-container {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 4px 12px 4px;
    gap: 16px;
  }

  .pagination-info {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .pagination-buttons {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .pagination-pages {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .page-num-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-secondary);
    min-width: 32px;
    height: 32px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .page-num-btn:hover {
    background: var(--bg-elevated);
    border-color: var(--border);
    color: var(--text-primary);
  }

  .active-page {
    background: var(--accent-muted) !important;
    border-color: var(--accent) !important;
    color: var(--accent-text) !important;
    font-weight: 600;
  }

  .pagination-ellipsis {
    padding: 0 4px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .status-toggle-btn {
    background: transparent;
    border: none;
    padding: 0;
    margin: 0;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    border-radius: var(--radius-sm);
    transition:
      transform 0.1s ease,
      opacity 0.2s;
  }
  .status-toggle-btn:hover {
    opacity: 0.85;
    transform: scale(1.02);
  }
  .status-toggle-btn:active {
    transform: scale(0.98);
  }
</style>
