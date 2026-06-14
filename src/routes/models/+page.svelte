<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ModelCatalogEntry, ModelEndpoint } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { dataRevision } from '$lib/data-revision.svelte';
  import CatalogLogo from '$lib/components/CatalogLogo.svelte';
  import { modelLabId } from '$lib/models-dev';

  type SortKey = 'name' | 'id' | 'family' | 'context' | 'knowledge' | 'release_date' | 'enabled';

  let entries = $state<ModelCatalogEntry[]>([]);
  let loading = $state(true);
  let searchQuery = $state('');
  let statusFilter = $state<'all' | 'enabled' | 'disabled'>('all');
  let sortKey = $state<SortKey>('name');
  let sortDir = $state<'asc' | 'desc'>('asc');
  let expandedCatalogId = $state<string | null>(null);
  let endpointCache = $state<Record<string, ModelEndpoint[]>>({});
  let endpointLoading = $state<string | null>(null);

  function md(entry: ModelCatalogEntry): Record<string, unknown> {
    return entry.models_dev ?? {};
  }

  function displayName(entry: ModelCatalogEntry): string {
    const name = md(entry).name;
    return typeof name === 'string' && name.trim() ? name : entry.catalog_id;
  }

  function limitField(entry: ModelCatalogEntry, key: 'context' | 'output'): number | null {
    const limit = md(entry).limit;
    if (!limit || typeof limit !== 'object') return null;
    const value = (limit as Record<string, unknown>)[key];
    return typeof value === 'number' ? value : null;
  }

  function formatTokens(value: number | null): string {
    if (value == null || value <= 0) return '—';
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${Math.round(value / 1_000)}K`;
    return String(value);
  }

  function capabilityFlags(entry: ModelCatalogEntry): string[] {
    const data = md(entry);
    const flags: string[] = [];
    if (data.reasoning) flags.push(i18n.t('models.cap_reasoning'));
    if (data.tool_call) flags.push(i18n.t('models.cap_tools'));
    if (data.temperature) flags.push(i18n.t('models.cap_temperature'));
    if (data.attachment) flags.push(i18n.t('models.cap_files'));
    if (data.open_weights) flags.push(i18n.t('models.cap_open_weights'));
    return flags;
  }

  type BenchmarkRow = {
    name: string;
    score: string;
    metric: string;
    date: string;
    source?: string;
  };

  type WeightRow = {
    label: string;
    url: string;
  };

  function parseBenchmarks(value: unknown): BenchmarkRow[] {
    if (!Array.isArray(value)) return [];
    return value
      .map((item) => {
        if (!item || typeof item !== 'object') return null;
        const row = item as Record<string, unknown>;
        const name = String(row.name ?? '—');
        const scoreRaw = row.score;
        const score = typeof scoreRaw === 'number' ? scoreRaw.toFixed(1) : String(scoreRaw ?? '—');
        return {
          name,
          score,
          metric: String(row.metric ?? '—'),
          date: String(row.date ?? '—'),
          source: typeof row.source === 'string' ? row.source : undefined,
        } as BenchmarkRow;
      })
      .filter((row): row is BenchmarkRow => row !== null);
  }

  function parseWeights(value: unknown): WeightRow[] {
    if (!Array.isArray(value)) return [];
    return value
      .map((item) => {
        if (!item || typeof item !== 'object') return null;
        const row = item as Record<string, unknown>;
        const url = typeof row.url === 'string' ? row.url : '';
        if (!url) return null;
        return {
          label: String(row.label ?? url),
          url,
        };
      })
      .filter((row): row is WeightRow => row !== null);
  }

  function capabilityItems(entry: ModelCatalogEntry): Array<{ label: string; enabled: boolean }> {
    const data = md(entry);
    return [
      { label: i18n.t('models.reasoning'), enabled: !!data.reasoning },
      { label: i18n.t('models.tool_call'), enabled: !!data.tool_call },
      { label: i18n.t('models.temperature'), enabled: !!data.temperature },
      { label: i18n.t('models.attachment'), enabled: !!data.attachment },
      { label: i18n.t('models.open_weights'), enabled: !!data.open_weights },
    ];
  }

  function modalityChips(value: unknown): { input: string[]; output: string[] } {
    if (!value || typeof value !== 'object') return { input: [], output: [] };
    const modalities = value as Record<string, unknown>;
    return {
      input: Array.isArray(modalities.input) ? modalities.input.map(String) : [],
      output: Array.isArray(modalities.output) ? modalities.output.map(String) : [],
    };
  }

  function settingsPath(entry: ModelCatalogEntry): string {
    return `models["${entry.catalog_id}"]`;
  }

  function aaPerformanceRows(entry: ModelCatalogEntry): Array<{ label: string; value: string }> {
    const perf = entry.artificial_analysis?.performance;
    if (!perf) return [];
    const rows: Array<{ label: string; value: string }> = [];
    if (perf.median_output_tokens_per_second != null && perf.median_output_tokens_per_second > 0) {
      rows.push({
        label: i18n.t('models.aa_output_speed'),
        value: `${Number(perf.median_output_tokens_per_second).toFixed(1)} t/s`,
      });
    }
    if (perf.median_time_to_first_token_seconds != null) {
      rows.push({
        label: i18n.t('models.aa_ttft'),
        value: `${Number(perf.median_time_to_first_token_seconds).toFixed(2)} s`,
      });
    }
    return rows;
  }

  function aaScoreRows(entry: ModelCatalogEntry): Array<{ label: string; value: string }> {
    const aa = entry.artificial_analysis;
    if (!aa) return [];
    const evalLabels: Record<string, string> = {
      artificial_analysis_intelligence_index: i18n.t('models.aa_intelligence'),
      artificial_analysis_coding_index: i18n.t('models.aa_coding'),
      artificial_analysis_math_index: i18n.t('models.aa_math'),
      tau2: 'Tau-2',
      terminalbench_hard: 'TerminalBench Hard',
      livecodebench: 'LiveCodeBench',
      scicode: 'SciCode',
      gpqa: 'GPQA',
      hle: 'HLE',
    };
    return Object.entries(evalLabels)
      .map(([key, label]) => {
        const value = aa.evaluations[key as keyof typeof aa.evaluations];
        if (value == null) return null;
        return { label, value: Number(value).toFixed(1) };
      })
      .filter((row): row is { label: string; value: string } => row !== null);
  }

  let rows = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    let result = entries.filter((entry) => {
      if (statusFilter === 'enabled' && !entry.enabled) return false;
      if (statusFilter === 'disabled' && entry.enabled) return false;
      if (!query) return true;
      const name = displayName(entry).toLowerCase();
      return name.includes(query) || entry.catalog_id.toLowerCase().includes(query);
    });

    result = [...result].sort((a, b) => {
      const order = sortDir === 'asc' ? 1 : -1;
      const compare = (left: string | number | boolean, right: string | number | boolean) => {
        if (typeof left === 'number' && typeof right === 'number') return (left - right) * order;
        if (typeof left === 'boolean' && typeof right === 'boolean') {
          return (Number(left) - Number(right)) * order;
        }
        return String(left ?? '').localeCompare(String(right ?? '')) * order;
      };

      switch (sortKey) {
        case 'id':
          return compare(a.catalog_id, b.catalog_id);
        case 'family':
          return compare(String(md(a).family ?? ''), String(md(b).family ?? ''));
        case 'context':
          return compare(limitField(a, 'context') ?? 0, limitField(b, 'context') ?? 0);
        case 'knowledge':
          return compare(String(md(a).knowledge ?? ''), String(md(b).knowledge ?? ''));
        case 'release_date':
          return compare(String(md(a).release_date ?? ''), String(md(b).release_date ?? ''));
        case 'enabled':
          return compare(a.enabled, b.enabled);
        default:
          return compare(displayName(a), displayName(b));
      }
    });

    return result;
  });

  onMount(loadEntries);

  async function loadEntries(options?: { silent?: boolean }) {
    if (!options?.silent) loading = true;
    try {
      entries = await api.models.listCatalog();
      if (expandedCatalogId && !entries.some((e) => e.catalog_id === expandedCatalogId)) {
        expandedCatalogId = null;
      }
    } catch (e) {
      entries = [];
      toast.error(e instanceof Error ? e.message : i18n.t('models.load_failed'));
    } finally {
      if (!options?.silent) loading = false;
    }
  }

  function setSort(key: SortKey) {
    if (sortKey === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortKey = key;
      sortDir = 'asc';
    }
  }

  async function toggleExpanded(catalogId: string) {
    if (expandedCatalogId === catalogId) {
      expandedCatalogId = null;
      return;
    }
    expandedCatalogId = catalogId;
    if (endpointCache[catalogId]) return;
    endpointLoading = catalogId;
    try {
      const eps = await api.models.endpoints(catalogId);
      endpointCache = { ...endpointCache, [catalogId]: eps };
    } catch {
      endpointCache = { ...endpointCache, [catalogId]: [] };
    } finally {
      endpointLoading = null;
    }
  }

  async function toggleEndpoint(catalogId: string, ep: ModelEndpoint) {
    try {
      await api.models.updateEndpoint(ep.id, !ep.enabled);
      const eps = await api.models.endpoints(catalogId);
      endpointCache = { ...endpointCache, [catalogId]: eps };
      toast.success(
        i18n
          .t('models.endpoint_toggle_success')
          .replace('{name}', ep.provider_name)
          .replace('{status}', !ep.enabled ? i18n.t('common.enabled') : i18n.t('common.disabled'))
      );
      dataRevision.touchModels();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    }
  }

  function formatEndpointCost(value: number | null | undefined): string {
    if (value == null || value < 0) return '—';
    return `$${value.toFixed(4)}`;
  }

  async function toggleEnabled(entry: ModelCatalogEntry) {
    const nextEnabled = !entry.enabled;
    try {
      await api.models.update(entry.id, { enabled: nextEnabled });
      entries = entries.map((e) =>
        e.catalog_id === entry.catalog_id
          ? { ...e, enabled: nextEnabled, settings: { ...e.settings, enabled: nextEnabled } }
          : e
      );
      const statusText = nextEnabled ? i18n.t('common.enabled') : i18n.t('common.disabled');
      toast.success(
        i18n
          .t('models.toggle_success')
          .replace('{name}', displayName(entry))
          .replace('{status}', statusText)
      );
      dataRevision.touchModels();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    }
  }
</script>

<PageHeader title={i18n.t('models.title')} description={i18n.t('models.subtitle')} />

{#if loading}
  <div class="skeleton" style="height: 260px; border-radius: var(--radius-lg);"></div>
{:else}
  <section class="model-toolbar">
    <div class="toolbar-col">
      <div class="search-wrap">
        <svg
          class="search-icon"
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.4"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          class="input search-input"
          type="text"
          placeholder={i18n.t('models.search_placeholder')}
          bind:value={searchQuery}
        />
      </div>
    </div>
    <div class="toolbar-col">
      <div class="filter-segment" role="group">
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'all'}
          onclick={() => (statusFilter = 'all')}
        >
          {i18n.t('models.filter_all_status')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'enabled'}
          onclick={() => (statusFilter = 'enabled')}
        >
          {i18n.t('models.filter_enabled')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'disabled'}
          onclick={() => (statusFilter = 'disabled')}
        >
          {i18n.t('models.filter_disabled')}
        </button>
      </div>
    </div>
    <div class="toolbar-col toolbar-count">
      <span class="muted text-xs">{rows.length} / {entries.length}</span>
    </div>
  </section>

  <section class="model-table-wrap">
    <div class="model-table">
      <div class="model-grid header-row">
        <div class="cell cell-logo"></div>
        <div
          class="cell cell-name sortable"
          class:active-sort={sortKey === 'name'}
          onclick={() => setSort('name')}
        >
          {i18n.t('models.col_name')}
        </div>
        <div
          class="cell cell-id sortable"
          class:active-sort={sortKey === 'id'}
          onclick={() => setSort('id')}
        >
          {i18n.t('models.col_id')}
        </div>
        <div
          class="cell cell-family sortable"
          class:active-sort={sortKey === 'family'}
          onclick={() => setSort('family')}
        >
          {i18n.t('models.col_family')}
        </div>
        <div
          class="cell cell-context sortable"
          class:active-sort={sortKey === 'context'}
          onclick={() => setSort('context')}
        >
          {i18n.t('models.col_context')}
        </div>
        <div class="cell cell-output">{i18n.t('models.col_output')}</div>
        <div
          class="cell cell-knowledge sortable"
          class:active-sort={sortKey === 'knowledge'}
          onclick={() => setSort('knowledge')}
        >
          {i18n.t('models.col_knowledge')}
        </div>
        <div
          class="cell cell-release sortable"
          class:active-sort={sortKey === 'release_date'}
          onclick={() => setSort('release_date')}
        >
          {i18n.t('models.col_release')}
        </div>
        <div class="cell cell-capabilities">{i18n.t('models.col_capabilities')}</div>
        <div
          class="cell cell-enabled sortable"
          class:active-sort={sortKey === 'enabled'}
          onclick={() => setSort('enabled')}
        >
          {i18n.t('models.col_enabled')}
        </div>
        <div class="cell cell-chevron" aria-hidden="true"></div>
      </div>

      {#each rows as entry (entry.catalog_id)}
        <div class="model-block" class:expanded={expandedCatalogId === entry.catalog_id}>
          <button
            type="button"
            class="model-grid data-row"
            onclick={() => toggleExpanded(entry.catalog_id)}
            aria-expanded={expandedCatalogId === entry.catalog_id}
          >
            <div class="cell cell-logo">
              {#if modelLabId(entry.catalog_id)}
                <CatalogLogo
                  id={modelLabId(entry.catalog_id)!}
                  kind="lab"
                  size={22}
                  alt={displayName(entry)}
                />
              {/if}
            </div>
            <div class="cell cell-name">
              <strong>{displayName(entry)}</strong>
            </div>
            <div class="cell cell-id">
              <span class="mono muted">{entry.catalog_id}</span>
            </div>
            <div class="cell cell-family">{String(md(entry).family ?? '—')}</div>
            <div class="cell cell-context mono">{formatTokens(limitField(entry, 'context'))}</div>
            <div class="cell cell-output mono">{formatTokens(limitField(entry, 'output'))}</div>
            <div class="cell cell-knowledge">{String(md(entry).knowledge ?? '—')}</div>
            <div class="cell cell-release">{String(md(entry).release_date ?? '—')}</div>
            <div class="cell cell-capabilities">
              {#if capabilityFlags(entry).length > 0}
                <div class="cap-badges">
                  {#each capabilityFlags(entry).slice(0, 3) as flag}
                    <span class="badge badge-secondary">{flag}</span>
                  {/each}
                </div>
              {:else}
                <span class="muted">—</span>
              {/if}
            </div>
            <div class="cell cell-enabled">
              <span class="status-badge" class:enabled={entry.enabled}>
                {entry.enabled ? i18n.t('common.enabled') : i18n.t('common.disabled')}
              </span>
            </div>
            <div class="cell cell-chevron">
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                style="transform: rotate({expandedCatalogId === entry.catalog_id
                  ? 180
                  : 0}deg); transition: transform 0.2s;"
              >
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </div>
          </button>

          {#if expandedCatalogId === entry.catalog_id}
            {@const data = md(entry)}
            {@const benchmarks = parseBenchmarks(data.benchmarks)}
            {@const weights = parseWeights(data.weights)}
            {@const modalities = modalityChips(data.modalities)}
            <div class="detail-panel">
              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('models.section_models_dev')}</h4>
                </div>

                {#if Object.keys(data).length === 0}
                  <div class="detail-empty">{i18n.t('models.models_dev_empty')}</div>
                {:else}
                  <div class="detail-subsection">
                    <h5>{i18n.t('models.detail_basic')}</h5>
                    <div class="spec-grid">
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.col_name')}</span>
                        <span class="spec-value">{displayName(entry)}</span>
                      </div>
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.col_id')}</span>
                        <span class="spec-value mono">{entry.catalog_id}</span>
                      </div>
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.family')}</span>
                        <span class="spec-value">{String(data.family ?? '—')}</span>
                      </div>
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.knowledge_cutoff')}</span>
                        <span class="spec-value">{String(data.knowledge ?? '—')}</span>
                      </div>
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.release_date')}</span>
                        <span class="spec-value">{String(data.release_date ?? '—')}</span>
                      </div>
                      <div class="spec-item">
                        <span class="spec-label">{i18n.t('models.last_updated')}</span>
                        <span class="spec-value">{String(data.last_updated ?? '—')}</span>
                      </div>
                    </div>
                  </div>

                  <div class="detail-subsection">
                    <h5>{i18n.t('models.detail_limits')}</h5>
                    <div class="limit-cards">
                      <div class="limit-card">
                        <span class="limit-label">{i18n.t('models.col_context')}</span>
                        <span class="limit-value mono"
                          >{formatTokens(limitField(entry, 'context'))}</span
                        >
                      </div>
                      <div class="limit-card">
                        <span class="limit-label">{i18n.t('models.col_output')}</span>
                        <span class="limit-value mono"
                          >{formatTokens(limitField(entry, 'output'))}</span
                        >
                      </div>
                    </div>
                  </div>

                  <div class="detail-subsection">
                    <h5>{i18n.t('models.detail_capabilities')}</h5>
                    <div class="cap-grid">
                      {#each capabilityItems(entry) as cap}
                        <span class="cap-pill" class:on={cap.enabled}
                          >{cap.label} · {cap.enabled
                            ? i18n.t('models.detail_yes')
                            : i18n.t('models.detail_no')}</span
                        >
                      {/each}
                    </div>
                  </div>

                  {#if modalities.input.length > 0 || modalities.output.length > 0}
                    <div class="detail-subsection">
                      <h5>{i18n.t('models.detail_modalities')}</h5>
                      <div class="modality-row">
                        {#if modalities.input.length > 0}
                          <div class="modality-group">
                            <span class="modality-dir">In</span>
                            <div class="chip-row">
                              {#each modalities.input as item}
                                <span class="chip">{item}</span>
                              {/each}
                            </div>
                          </div>
                        {/if}
                        {#if modalities.output.length > 0}
                          <div class="modality-group">
                            <span class="modality-dir">Out</span>
                            <div class="chip-row">
                              {#each modalities.output as item}
                                <span class="chip">{item}</span>
                              {/each}
                            </div>
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/if}

                  {#if benchmarks.length > 0}
                    <div class="detail-subsection">
                      <h5>{i18n.t('models.benchmarks')}</h5>
                      <div class="mini-table-wrap">
                        <table class="mini-table">
                          <thead>
                            <tr>
                              <th>{i18n.t('models.detail_benchmark_name')}</th>
                              <th>{i18n.t('models.detail_benchmark_score')}</th>
                              <th>{i18n.t('models.detail_benchmark_metric')}</th>
                              <th>{i18n.t('models.detail_benchmark_date')}</th>
                            </tr>
                          </thead>
                          <tbody>
                            {#each benchmarks as bench}
                              <tr>
                                <td>
                                  {#if bench.source}
                                    <a
                                      href={bench.source}
                                      target="_blank"
                                      rel="noopener noreferrer"
                                      class="detail-link">{bench.name}</a
                                    >
                                  {:else}
                                    {bench.name}
                                  {/if}
                                </td>
                                <td class="mono">{bench.score}</td>
                                <td>{bench.metric}</td>
                                <td>{bench.date}</td>
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  {/if}

                  {#if weights.length > 0}
                    <div class="detail-subsection">
                      <h5>{i18n.t('models.weights')}</h5>
                      <div class="link-list">
                        {#each weights as weight}
                          <a
                            href={weight.url}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="detail-link">{weight.label} ↗</a
                          >
                        {/each}
                      </div>
                    </div>
                  {/if}
                {/if}
              </section>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('models.section_aa')}</h4>
                </div>
                {#if entry.artificial_analysis}
                  <div class="spec-grid">
                    <div class="spec-item">
                      <span class="spec-label">{i18n.t('models.col_name')}</span>
                      <span class="spec-value">{entry.artificial_analysis.name}</span>
                    </div>
                    <div class="spec-item">
                      <span class="spec-label">slug</span>
                      <span class="spec-value mono">{entry.artificial_analysis.slug}</span>
                    </div>
                    <div class="spec-item">
                      <span class="spec-label">{i18n.t('models.aa_creator')}</span>
                      <span class="spec-value"
                        >{entry.artificial_analysis.creator_name ||
                          entry.artificial_analysis.creator_slug ||
                          '—'}</span
                      >
                    </div>
                  </div>
                  {#if aaPerformanceRows(entry).length > 0}
                    <div class="detail-subsection compact-top">
                      <h5>{i18n.t('models.aa_performance')}</h5>
                      <div class="score-grid">
                        {#each aaPerformanceRows(entry) as score}
                          <div class="score-card">
                            <span class="score-label">{score.label}</span>
                            <span class="score-value mono">{score.value}</span>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/if}
                  {#if aaScoreRows(entry).length > 0}
                    <div class="detail-subsection compact-top">
                      <h5>{i18n.t('models.aa_scores')}</h5>
                      <div class="score-grid">
                        {#each aaScoreRows(entry) as score}
                          <div class="score-card">
                            <span class="score-label">{score.label}</span>
                            <span class="score-value mono">{score.value}</span>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/if}
                {:else}
                  <div class="detail-empty">{i18n.t('models.aa_empty')}</div>
                {/if}
              </section>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('models.section_gateways')}</h4>
                  {#if endpointCache[entry.catalog_id]?.length}
                    <span class="gateway-count">{endpointCache[entry.catalog_id].length}</span>
                  {/if}
                </div>
                {#if endpointLoading === entry.catalog_id}
                  <div class="detail-empty">{i18n.t('models.endpoints_loading')}</div>
                {:else if (endpointCache[entry.catalog_id] ?? []).length === 0}
                  <div class="detail-empty">{i18n.t('models.no_sub_providers')}</div>
                {:else}
                  <div class="gateway-grid">
                    {#each endpointCache[entry.catalog_id] ?? [] as ep (ep.id)}
                      <article class="gateway-card" class:degraded={ep.status < 0}>
                        <div class="gateway-card-head">
                          <div class="gateway-title">
                            {#if ep.provider_tag}
                              <CatalogLogo
                                id={ep.provider_tag}
                                kind="provider"
                                size={20}
                                alt={ep.provider_name}
                              />
                            {/if}
                            <span class="gateway-name">{ep.provider_name}</span>
                          </div>
                          <button
                            type="button"
                            class="gateway-toggle"
                            class:on={ep.enabled}
                            onclick={() => toggleEndpoint(entry.catalog_id, ep)}
                          >
                            {ep.enabled ? i18n.t('common.enabled') : i18n.t('common.disabled')}
                          </button>
                        </div>
                        <div class="gateway-health" class:ok={ep.status === 0}>
                          {ep.status === 0
                            ? i18n.t('models.endpoint_online')
                            : i18n.t('models.endpoint_degraded')}
                        </div>
                        <div class="gateway-pricing mono">
                          <span>In {formatEndpointCost(ep.input_cost)}</span>
                          <span class="gateway-sep">·</span>
                          <span>Out {formatEndpointCost(ep.output_cost)}</span>
                        </div>
                        {#if ep.native_model_id}
                          <div class="gateway-native mono">{ep.native_model_id}</div>
                        {/if}
                        <div class="gateway-meta">
                          {#if ep.quantization && ep.quantization !== 'unknown'}
                            <span class="gateway-badge">{ep.quantization}</span>
                          {/if}
                          {#if ep.supports_tools}
                            <span class="gateway-badge">{i18n.t('models.tool_call')}</span>
                          {/if}
                          {#if ep.context_length}
                            <span class="gateway-badge">{formatTokens(ep.context_length)} ctx</span>
                          {/if}
                          {#if ep.uptime_30m != null}
                            <span class="gateway-badge">{ep.uptime_30m.toFixed(1)}% up</span>
                          {/if}
                        </div>
                      </article>
                    {/each}
                  </div>
                {/if}
              </section>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('models.section_settings')}</h4>
                </div>
                <div class="settings-block">
                  <div class="settings-toggle-row">
                    <label class="toggle" onclick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={entry.enabled}
                        onchange={() => toggleEnabled(entry)}
                      />
                      <span class="toggle-slider"></span>
                    </label>
                    <div>
                      <div class="settings-label">{i18n.t('models.enabled_label')}</div>
                      <div class="settings-hint">{i18n.t('models.enabled_hint')}</div>
                    </div>
                    <span class="status-badge" class:enabled={entry.enabled}>
                      {entry.enabled ? i18n.t('common.enabled') : i18n.t('common.disabled')}
                    </span>
                  </div>
                  <div class="settings-meta">
                    <span class="spec-label">{i18n.t('models.settings_path_hint')}</span>
                    <code class="settings-path">{settingsPath(entry)}</code>
                  </div>
                </div>
              </section>
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if rows.length === 0}
      <div class="empty-row">{i18n.t('models.empty')}</div>
    {/if}
  </section>
{/if}

<style>
  .model-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) auto;
    gap: 10px;
    margin-bottom: 12px;
    align-items: stretch;
  }

  .toolbar-col {
    min-width: 0;
    display: flex;
    align-items: center;
  }

  .toolbar-count {
    justify-content: flex-end;
    padding-right: 4px;
  }

  .search-wrap {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .search-icon {
    position: absolute;
    left: 11px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-muted);
    pointer-events: none;
  }

  .search-input {
    padding-left: 34px;
    width: 100%;
  }

  .filter-segment {
    display: flex;
    flex: 1;
    min-width: 0;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 3px;
    gap: 2px;
  }

  .segment-btn {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
    padding: 7px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .segment-btn:hover:not(.active) {
    color: var(--text-secondary);
    background: rgba(255, 255, 255, 0.02);
  }

  .segment-btn.active {
    background: rgba(59, 130, 246, 0.15);
    color: #60a5fa;
    border: 1px solid rgba(59, 130, 246, 0.2);
    font-weight: 600;
  }

  .model-table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .model-grid {
    display: grid;
    grid-template-columns:
      40px minmax(140px, 1.2fr) minmax(140px, 1fr)
      90px 72px 72px 88px 92px minmax(120px, 0.9fr) 72px 28px;
    align-items: center;
    gap: 8px;
    width: 100%;
  }

  .header-row {
    background: var(--bg-primary);
    border-bottom: 1px solid var(--border);
    padding: 10px 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .header-row .sortable {
    cursor: pointer;
    user-select: none;
  }

  .header-row .sortable:hover,
  .header-row .active-sort {
    color: var(--accent);
  }

  .model-block {
    border-bottom: 1px solid var(--border);
  }

  .model-block:last-child {
    border-bottom: 0;
  }

  .data-row {
    padding: 10px 12px;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease;
    width: 100%;
  }

  .data-row:hover,
  .model-block.expanded .data-row {
    background: var(--bg-primary);
  }

  .cell {
    min-width: 0;
    font-size: 13px;
  }

  .cell-logo {
    display: flex;
    justify-content: center;
  }

  .cell-name strong {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell-id .mono {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .cap-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .badge-secondary {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    color: var(--text-primary);
    font-size: 10px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 600;
    border: 1px solid var(--border);
    background: var(--bg-primary);
    color: var(--text-secondary);
  }

  .status-badge.enabled {
    color: var(--success);
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.08);
  }

  .detail-panel {
    padding: 0 12px 16px;
    background: var(--bg-tertiary, var(--bg-primary));
    border-top: 1px solid var(--border);
  }

  .detail-section {
    padding-top: 14px;
  }

  .detail-section + .detail-section {
    border-top: 1px solid var(--border);
    margin-top: 4px;
  }

  .detail-section-head {
    margin-bottom: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .detail-section-head h4 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .gateway-count {
    font-size: 11px;
    font-weight: 700;
    padding: 1px 7px;
    border-radius: 999px;
    background: rgba(59, 130, 246, 0.15);
    color: #60a5fa;
  }

  .gateway-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px;
  }

  .gateway-card {
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .gateway-card.degraded {
    opacity: 0.72;
  }

  .gateway-card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }

  .gateway-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .gateway-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    word-break: break-word;
  }

  .gateway-toggle {
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    background: var(--bg-primary);
    color: var(--text-muted);
  }

  .gateway-toggle.on {
    color: var(--success);
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.08);
  }

  .gateway-health {
    font-size: 11px;
    font-weight: 600;
    color: #f59e0b;
  }

  .gateway-health.ok {
    color: var(--success);
  }

  .gateway-pricing {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .gateway-sep {
    margin: 0 4px;
    color: var(--text-muted);
  }

  .gateway-native {
    font-size: 11px;
    color: var(--text-muted);
    word-break: break-all;
  }

  .gateway-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .gateway-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-primary);
    color: var(--text-muted);
  }

  .detail-subsection {
    margin-top: 12px;
  }

  .detail-subsection.compact-top {
    margin-top: 10px;
  }

  .detail-subsection h5 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .spec-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px 16px;
  }

  .spec-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .spec-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .spec-value {
    font-size: 12px;
    word-break: break-word;
  }

  .limit-cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 160px));
    gap: 10px;
  }

  .limit-card {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-secondary);
  }

  .limit-label {
    display: block;
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 4px;
  }

  .limit-value {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .cap-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .cap-pill {
    font-size: 11px;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-secondary);
    color: var(--text-muted);
  }

  .cap-pill.on {
    color: var(--text-primary);
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.08);
  }

  .modality-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .modality-group {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .modality-dir {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    min-width: 28px;
  }

  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    font-size: 11px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-secondary);
    color: var(--text-secondary);
  }

  .mini-table-wrap {
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .mini-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .mini-table th,
  .mini-table td {
    padding: 8px 10px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }

  .mini-table th {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-primary);
  }

  .mini-table tr:last-child td {
    border-bottom: 0;
  }

  .detail-link {
    color: var(--accent);
    text-decoration: none;
    font-size: 12px;
  }

  .detail-link:hover {
    text-decoration: underline;
  }

  .link-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .score-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
  }

  .score-card {
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-secondary);
  }

  .score-label {
    display: block;
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 4px;
  }

  .score-value {
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .detail-empty {
    padding: 12px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
  }

  .settings-block {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .settings-toggle-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .settings-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .settings-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .settings-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .settings-path {
    font-size: 12px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    color: var(--text-secondary);
  }

  .empty-row {
    padding: 32px;
    color: var(--text-muted);
    text-align: center;
  }

  .muted {
    color: var(--text-muted);
  }

  @media (max-width: 1200px) {
    .model-table-wrap {
      overflow-x: auto;
    }

    .model-grid {
      min-width: 1100px;
    }

    .spec-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
