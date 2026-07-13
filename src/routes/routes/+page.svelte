<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import { dataRevision } from '$lib/data-revision.svelte';
  import { gatewayHealth } from '$lib/gateway-health.svelte';
  import type { Model, Provider, RankedModelSummary, RouteExplainResult, RoutableModel, StrategyBoardResult } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { toast } from '$lib/components/Toast.svelte';

  let routableModels = $state<RoutableModel[]>([]);
  let providers = $state<Provider[]>([]);
  let loading = $state(true);
  let expandedStrategies = $state<Record<string, boolean>>({});
  let previewAgent = $state('codex');
  let previewModel = $state('auto');
  let previewPrompt = $state('Explain this Rust error and suggest a fix.');
  let previewLoading = $state(false);
  let previewResult = $state<RouteExplainResult | null>(null);
  let strategyBoard = $state<StrategyBoardResult | null>(null);

  const PREVIEW_AGENTS = ['codex', 'claude-code', 'opencode', 'kilocode', 'hermes', 'openclaw', 'pi'];

  // Strategy metadata
  const STRATEGIES = [
    {
      id: 'auto',
      icon: '⚡',
      color: '#6366f1',
      bg: 'rgba(99,102,241,0.06)',
      glow: 'rgba(99,102,241,0.15)',
      border: 'rgba(99,102,241,0.25)',
    },
    {
      id: 'balanced',
      icon: '⚖️',
      color: '#f59e0b',
      bg: 'rgba(245,158,11,0.06)',
      glow: 'rgba(245,158,11,0.15)',
      border: 'rgba(245,158,11,0.25)',
    },
    {
      id: 'cheapest',
      icon: '💰',
      color: '#22c55e',
      bg: 'rgba(34,197,94,0.06)',
      glow: 'rgba(34,197,94,0.15)',
      border: 'rgba(34,197,94,0.25)',
    },
    {
      id: 'intelligent',
      icon: '🧠',
      color: '#a855f7',
      bg: 'rgba(168,85,247,0.06)',
      glow: 'rgba(168,85,247,0.15)',
      border: 'rgba(168,85,247,0.25)',
    },
    {
      id: 'speed',
      icon: '🚀',
      color: '#06b6d4',
      bg: 'rgba(6,182,212,0.06)',
      glow: 'rgba(6,182,212,0.15)',
      border: 'rgba(6,182,212,0.25)',
    },
    {
      id: 'agentic',
      icon: '🤖',
      color: '#ff6b6b',
      bg: 'rgba(255,107,107,0.06)',
      glow: 'rgba(255,107,107,0.15)',
      border: 'rgba(255,107,107,0.25)',
    },
  ] as const;

  let hasLoaded = $state(false);
  let boardSeq = 0;

  function boardRequest() {
    return {
      agent: previewAgent,
      body: { messages: [{ role: 'user', content: previewPrompt }] },
    };
  }

  async function loadStrategyBoard(options?: { silent?: boolean }) {
    const seq = ++boardSeq;
    try {
      const board = await api.routes.strategyBoard(boardRequest());
      if (seq !== boardSeq) return;
      strategyBoard = board;
    } catch (e) {
      if (seq !== boardSeq) return;
      if (!options?.silent) {
        toast.error(e instanceof Error ? e.message : i18n.t('routes.load_failed'));
      }
    }
  }

  async function loadRouteData(options?: { silent?: boolean }) {
    if (!options?.silent) loading = true;
    try {
      const [providersList, routableList] = await Promise.all([
        api.providers.list(),
        api.models.listRoutable(),
      ]);
      providers = providersList;
      routableModels = routableList;
      hasLoaded = true;
      await loadStrategyBoard({ silent: true });
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('routes.load_failed'));
    } finally {
      if (!options?.silent) loading = false;
    }
  }

  onMount(() => {
    void loadRouteData();
  });

  afterNavigate(({ to }) => {
    if (to?.url.pathname === '/routes' && hasLoaded) {
      void loadRouteData({ silent: true });
    }
  });

  $effect(() => {
    void dataRevision.models;
    void dataRevision.providers;
    if (!hasLoaded || $page.url.pathname !== '/routes') return;
    void loadRouteData({ silent: true });
  });

  let previewBoardTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    previewAgent;
    previewPrompt;
    if (!hasLoaded || $page.url.pathname !== '/routes') return;
    clearTimeout(previewBoardTimer);
    previewBoardTimer = setTimeout(() => {
      void loadStrategyBoard({ silent: true });
    }, 300);
    return () => clearTimeout(previewBoardTimer);
  });

  let stopGatewayListener: (() => void) | undefined;
  $effect(() => {
    if ($page.url.pathname !== '/routes') return;
    stopGatewayListener?.();
    stopGatewayListener = gatewayHealth.onRunning(() => {
      if (!hasLoaded) return;
      void loadStrategyBoard({ silent: true });
    });
    return () => {
      stopGatewayListener?.();
      stopGatewayListener = undefined;
    };
  });

  onDestroy(() => {
    stopGatewayListener?.();
    clearTimeout(previewBoardTimer);
  });

  $effect(() => {
    if (!hasLoaded || $page.url.pathname !== '/routes') return;
    if (gatewayHealth.status === 'running' && !strategyBoard) {
      void loadStrategyBoard({ silent: true });
    }
  });

  // derived provider map for fast lookup
  const providerMap = $derived(new Map(providers.map((p) => [p.id, p])));

  type StrategyCandidate = {
    route: RoutableModel;
    model: Model;
    rank: number;
    summary: RankedModelSummary;
    serviceProviderId: string;
    displayStrategy: string;
  };

  /** Order and scores come from the gateway strategy-board API (cab-core routing). */
  function resolveRoutesForStrategy(strategyId: string): {
    displayStrategy: string;
    candidates: StrategyCandidate[];
  } {
    const board = strategyBoard?.strategies.find((strategy) => strategy.id === strategyId);
    if (!board) {
      return { displayStrategy: strategyId, candidates: [] };
    }

    const routeByKey = new Map(
      routableModels.map((route) => [`${route.name}\0${route.service_provider_id}`, route])
    );

    const candidates = board.candidates.flatMap((summary, index) => {
      const route = routeByKey.get(`${summary.model_id}\0${summary.provider_id}`);
      if (!route) return [];
      return [
        {
          route,
          model: route,
          rank: index + 1,
          summary,
          serviceProviderId: summary.provider_id,
          displayStrategy: board.display_strategy,
        },
      ];
    });

    return { displayStrategy: board.display_strategy, candidates };
  }

  function visibleCandidates(
    candidates: StrategyCandidate[],
    expanded: boolean,
    previewLimit = 5
  ): StrategyCandidate[] {
    return expanded ? candidates : candidates.slice(0, previewLimit);
  }

  function resolveCost(
    primary: number | null | undefined,
    fallback: number | null | undefined
  ): number | null {
    const value = primary ?? fallback;
    return typeof value === 'number' && value >= 0 ? value : null;
  }

  function routeInputCost(route: RoutableModel): number | null {
    return resolveCost(route.endpoint_input_cost, route.input_cost);
  }

  function routeOutputCost(route: RoutableModel): number | null {
    return resolveCost(route.endpoint_output_cost, route.output_cost);
  }

  function formatPrice(value: number | null): string {
    return value == null ? '—' : `$${value.toFixed(2)}`;
  }

  function formatPricePair(route: RoutableModel): string {
    const input = routeInputCost(route);
    const output = routeOutputCost(route);
    if (input == null || output == null) return '—';
    return `${formatPrice(input)} / ${formatPrice(output)}`;
  }

  const INPUT_OUTPUT_RATIO = 10;
  const INPUT_CACHE_HIT_RATE = 0.9;

  function routeCacheReadCost(route: RoutableModel): number | undefined {
    const cache = route.endpoint_cache_read_cost ?? route.pricing?.cache_read;
    return typeof cache === 'number' && cache >= 0 ? cache : undefined;
  }

  /** Endpoint-weighted cost (what you pay through this service provider). */
  function endpointEffectiveTokenCost(route: RoutableModel): number {
    const input = routeInputCost(route);
    const output = routeOutputCost(route);
    if (input == null || output == null) {
      return Number.POSITIVE_INFINITY;
    }
    const cacheRead = routeCacheReadCost(route);
    const blended =
      cacheRead !== undefined
        ? INPUT_CACHE_HIT_RATE * cacheRead + (1 - INPUT_CACHE_HIT_RATE) * input
        : input;
    return blended * INPUT_OUTPUT_RATIO + output;
  }

  function strategyMetricLabel(strategyId: string): string {
    if (strategyId === 'speed') return i18n.t('routes.speed');
    if (strategyId === 'cheapest') return i18n.t('routes.composite_price');
    if (strategyId === 'balanced' || strategyId === 'auto') return i18n.t('routes.value_score');
    if (strategyId === 'agentic') return i18n.t('routes.agentic_index');
    return i18n.t('routes.intel');
  }

  function formatStrategyMetric(
    displayStrategy: string,
    summary: RankedModelSummary,
    route: RoutableModel
  ): string {
    if (displayStrategy === 'speed') {
      if (summary.value == null || !Number.isFinite(summary.value)) return '—';
      return `${summary.value.toFixed(2)}s`;
    }
    if (displayStrategy === 'cheapest') {
      const cost = endpointEffectiveTokenCost(route);
      return Number.isFinite(cost) ? `$${cost.toFixed(2)}` : '—';
    }
    if (displayStrategy === 'balanced' || displayStrategy === 'auto') {
      return formatExplainValue(summary);
    }
    if (displayStrategy === 'intelligent' || displayStrategy === 'agentic') {
      return summary.value != null && Number.isFinite(summary.value) ? summary.value.toFixed(1) : '—';
    }
    return '—';
  }

  function formatExplainValue(candidate: RankedModelSummary, strategy?: string): string {
    if (candidate.value != null && Number.isFinite(candidate.value)) {
      if (strategy === 'speed') return `${candidate.value.toFixed(2)}s`;
      return candidate.value.toFixed(2);
    }
    if (candidate.value_unbounded) {
      return '∞';
    }
    return '—';
  }

  async function runRoutingPreview() {
    previewLoading = true;
    try {
      previewResult = await api.routes.explain({
        agent: previewAgent,
        model: previewModel,
        body: {
          messages: [{ role: 'user', content: previewPrompt }],
        },
      });
    } catch (e) {
      previewResult = null;
      toast.error(e instanceof Error ? e.message : i18n.t('routes.preview_failed'));
    } finally {
      previewLoading = false;
    }
  }
</script>

<PageHeader title={i18n.t('routes.title')} description={i18n.t('routes.page_desc')} />

<div class="preview-card-wrap">
<Card padding="24px">
  <h3 style="margin: 0 0 8px;">{i18n.t('routes.preview_title')}</h3>
  <p class="preview-desc">{i18n.t('routes.preview_desc')}</p>
  <div class="preview-form">
    <label>
      <span>{i18n.t('routes.preview_agent')}</span>
      <select bind:value={previewAgent}>
        {#each PREVIEW_AGENTS as agentId}
          <option value={agentId}>{agentId}</option>
        {/each}
      </select>
    </label>
    <label>
      <span>{i18n.t('routes.preview_model')}</span>
      <input bind:value={previewModel} placeholder="auto" />
    </label>
    <label class="preview-prompt">
      <span>{i18n.t('routes.preview_prompt')}</span>
      <textarea bind:value={previewPrompt} rows="2"></textarea>
    </label>
    <button class="preview-btn" onclick={runRoutingPreview} disabled={previewLoading}>
      {previewLoading ? i18n.t('routes.preview_running') : i18n.t('routes.preview_run')}
    </button>
  </div>

  {#if previewResult}
    <div class="preview-result">
      {#if previewResult.resolved}
        <div class="preview-resolved-outlet">
          <div class="outlet-header">
            <span class="pulse-dot active"></span>
            <strong>{i18n.t('routes.preview_resolved')}</strong>
          </div>
          <div class="outlet-body mono">
            <span class="resolved-model">{previewResult.resolved.model_id}</span>
            <span class="resolved-divider">➜</span>
            <span class="resolved-provider">{providerMap.get(previewResult.resolved.provider_id)?.name ?? previewResult.resolved.provider_id}</span>
            {#if previewResult.resolved.strategy}
              <span class="resolved-strategy-tag">{previewResult.resolved.strategy}</span>
            {/if}
          </div>
        </div>
      {/if}

      <div class="preview-steps-container">
        <strong>{i18n.t('routes.preview_steps')}</strong>
        <div class="decision-pipeline">
          {#each previewResult.decision_steps as step}
            <div class="decision-step" class:matched={step.matched}>
              <code class="step-code mono">{step.step}</code>
              <span class="step-detail">{step.detail}</span>
            </div>
          {/each}
        </div>
      </div>
      {#if previewResult.ranked_candidates.length > 0}
        {@const previewRankById = new Map(
          previewResult.ranked_candidates.map((candidate, index) => [
            `${candidate.model_id}\0${candidate.provider_id}`,
            index + 1,
          ])
        )}
        <div class="preview-block">
          <strong>{i18n.t('routes.preview_candidates')}</strong>
          <div class="pb-table-wrap">
            <table class="pb-table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>{i18n.t('routes.model_name')}</th>
                  <th>{i18n.t('routes.provider')}</th>
                  <th>{i18n.t('routes.intel')}</th>
                  <th>{i18n.t('routes.value_score')}</th>
                </tr>
              </thead>
              <tbody>
                {#each previewResult.ranked_candidates as candidate}
                  {@const strategy = previewResult.resolved?.strategy ?? ''}
                  <tr>
                    <td>{previewRankById.get(`${candidate.model_id}\0${candidate.provider_id}`) ?? '—'}</td>
                    <td>{candidate.model_id}</td>
                    <td>{providerMap.get(candidate.provider_id)?.name ?? candidate.provider_id}</td>
                    <td>
                      {#if strategy === 'intelligent' || strategy === 'agentic'}
                        {candidate.value != null && Number.isFinite(candidate.value) ? candidate.value.toFixed(1) : '—'}
                      {:else}
                        {candidate.capability != null ? candidate.capability.toFixed(1) : '—'}
                      {/if}
                    </td>
                    <td>
                      {#if strategy === 'intelligent' || strategy === 'agentic'}
                        {candidate.capability != null && Number.isFinite(candidate.capability) ? candidate.capability.toFixed(2) : '—'}
                      {:else}
                        {formatExplainValue(candidate, strategy)}
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</Card>
</div>

{#if loading}
  <div class="strategy-list">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 320px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
{:else}
  <div class="strategy-list">
    {#each STRATEGIES as s}
      {@const pools = resolveRoutesForStrategy(s.id)}
      {@const totalCandidates = pools.candidates.length}
      <div
        class="strategy-card-wrapper"
        style="--sc:{s.color}; --glow:{s.glow}; --sborder:{s.border}"
      >
        <Card padding="24px">
          <div class="strategy-header">
            <div class="strategy-title">
              <span class="st-icon">{s.icon}</span>
              <div class="st-text">
                <h3>{i18n.t('routes.strategies.' + s.id + '.label')}</h3>
                <span class="st-id">strategy: {s.id}</span>
              </div>
            </div>
            <span class="st-badge">
              <span class="st-dot"></span>
              {i18n.t('routes.active')}
            </span>
          </div>

          <p class="strategy-desc">
            {i18n.t('routes.strategies.' + s.id + '.desc')}
          </p>

          <!-- Strategy Rules & Policies -->
          <div class="resolved-block">
            <span class="rb-title">{i18n.t('routes.policy_rules')}</span>
            <div class="policy-desc-text">
              {i18n.t('routes.strategies.' + s.id + '.policy')}
            </div>
            <div class="policy-meta">
              <span class="meta-badge">
                {i18n.t('routes.mechanism_label')}
                <strong>
                  {i18n.t('routes.strategies.' + s.id + '.mechanism')}
                </strong>
              </span>
            </div>
          </div>

          <!-- Candidates Pool -->
          <div class="pool-block">
            <span class="pb-title">{i18n.t('routes.candidate_range')}</span>
            {#if totalCandidates > 0}
              {@const isExpanded = expandedStrategies[s.id] ?? false}
              {@const shown = visibleCandidates(pools.candidates, isExpanded)}
              <div class="pb-table-wrap">
                <table class="pb-table">
                  <thead>
                    <tr>
                      <th style="width: 50px; text-align: center;">{i18n.t('routes.rank')}</th>
                      <th style="width: 90px;">{i18n.t('routes.provider')}</th>
                      <th>{i18n.t('routes.model_name')}</th>
                      <th style="text-align: right; width: 130px;">{i18n.t('routes.price')}</th>
                      <th style="text-align: right; width: 70px;">
                        {strategyMetricLabel(pools.displayStrategy)}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each shown as c}
                      {@const provider = providerMap.get(c.serviceProviderId)}
                      <tr>
                        <td class="mono text-muted" style="text-align: center;">
                          {c.rank}
                        </td>
                        <td>
                          <span class="provider-badge">
                            {provider ? provider.name : c.serviceProviderId}
                          </span>
                        </td>
                        <td>
                          <div class="c-model-cell">
                            <div class="c-model-row">
                              <span class="c-name">{c.model.display_name}</span>
                            </div>
                            <span class="c-slug mono">{c.model.name}</span>
                          </div>
                        </td>
                        <td style="text-align: right;" class="mono text-secondary">
                          {formatPricePair(c.route)}
                        </td>
                        <td style="text-align: right;" class="mono text-accent">
                          {formatStrategyMetric(c.displayStrategy, c.summary, c.route)}
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              {#if totalCandidates > 5}
                <div style="display:flex; justify-content:center; margin-top: 8px;">
                  <button
                    class="btn btn-ghost btn-xs"
                    style="color: var(--accent); font-weight: 600; font-size: 11px;"
                    onclick={() => (expandedStrategies[s.id] = !isExpanded)}
                  >
                    {#if isExpanded}
                      {i18n.t('routes.show_less')}
                    {:else}
                      {i18n.tParams('routes.show_all_candidates', { count: totalCandidates })}
                    {/if}
                  </button>
                </div>
              {/if}
            {:else}
              <div class="pb-empty">
                ⚠️ {i18n.t('routes.no_models')}
              </div>
            {/if}
          </div>
        </Card>
      </div>
    {/each}
  </div>
{/if}

<style>
  .strategy-list {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 20px;
    margin-top: 10px;
  }

  @media (max-width: 1024px) {
    .strategy-list {
      grid-template-columns: 1fr;
    }
  }

  .strategy-card-wrapper {
    position: relative;
    border-radius: var(--radius-lg);
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .strategy-card-wrapper::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    border: 1px solid var(--sborder);
    pointer-events: none;
    transition: inherit;
  }

  .strategy-card-wrapper:hover {
    transform: translateY(-2px);
    box-shadow: 0 12px 30px -10px var(--glow);
  }

  .strategy-card-wrapper:hover::after {
    border-color: var(--sc);
  }

  .strategy-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 12px;
  }

  .strategy-title {
    display: flex;
    gap: 12px;
    align-items: center;
  }

  .st-icon {
    font-size: 28px;
    line-height: 1;
  }

  .st-text h3 {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  .st-id {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-mono);
    margin-top: 2px;
    display: block;
  }

  .st-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    background: rgba(34, 197, 94, 0.1);
    color: var(--success-text);
    border: 1px solid rgba(34, 197, 94, 0.25);
    border-radius: var(--radius-full);
    font-size: 10.5px;
    padding: 2px 7px;
    font-weight: 600;
  }

  .st-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #22c55e;
    box-shadow: 0 0 8px #22c55e;
  }

  .strategy-desc {
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.5;
    margin: 0 0 20px 0;
    min-height: 36px;
  }

  /* Resolved Block */
  .resolved-block {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    margin-bottom: 20px;
  }

  .rb-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    display: block;
    margin-bottom: 8px;
  }

  /* Pool Block */
  .pool-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .pb-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    display: block;
  }

  .pb-table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .pb-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    text-align: left;
  }

  .pb-table th {
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-weight: 600;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .pb-table td {
    padding: 8px 12px;
    font-size: 12px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border);
  }

  .pb-table tr:last-child td {
    border-bottom: none;
  }

  .c-model-cell {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .c-model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .c-name {
    font-weight: 500;
    color: var(--text-primary);
  }

  .c-slug {
    font-size: 10px;
    color: var(--text-muted);
  }

  .text-secondary {
    color: var(--text-secondary);
  }

  .text-accent {
    color: var(--accent);
    font-weight: 600;
  }

  .pb-empty {
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
    padding: 20px;
    background: var(--bg-secondary);
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
  }

  .policy-desc-text {
    font-size: 12.5px;
    color: var(--text-primary);
    line-height: 1.6;
    margin-bottom: 12px;
  }

  .policy-meta {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .meta-badge {
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
  }

  .meta-badge strong {
    color: var(--accent);
  }

  .preview-card-wrap {
    margin-bottom: 24px;
  }

  .preview-desc {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0 0 16px;
  }

  .preview-form {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    align-items: end;
  }

  .preview-form label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .preview-form select,
  .preview-form input,
  .preview-form textarea {
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    padding: 8px 10px;
    font: inherit;
    outline: none;
    transition: border-color var(--transition-fast);
  }

  .preview-form select:focus,
  .preview-form input:focus,
  .preview-form textarea:focus {
    border-color: var(--border-focus);
  }

  .preview-prompt {
    grid-column: 1 / -1;
  }

  .preview-btn {
    grid-column: 1 / -1;
    justify-self: start;
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--btn-primary-bg);
    color: var(--btn-primary-text);
    font-weight: 600;
    cursor: pointer;
    font-size: 12px;
    transition: all var(--transition-fast);
  }

  .preview-btn:hover:not(:disabled) {
    opacity: 0.9;
    box-shadow: var(--shadow-sm);
  }

  .preview-btn:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .preview-result {
    margin-top: 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  /* ── Resolved Export Outlet ──────────────────────────── */
  .preview-resolved-outlet {
    background: rgba(16, 185, 129, 0.03);
    border: 1px solid rgba(16, 185, 129, 0.15);
    border-radius: var(--radius-md);
    padding: 16px 20px;
    box-shadow: 0 0 15px rgba(16, 185, 129, 0.05);
  }

  .outlet-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .outlet-header strong {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #34d399;
  }

  .outlet-body {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    font-size: 13.5px;
    font-weight: 600;
  }

  .resolved-model {
    color: var(--text-primary);
  }

  .resolved-divider {
    color: var(--text-muted);
    font-size: 11px;
  }

  .resolved-provider {
    color: #a7f3d0;
  }

  .resolved-strategy-tag {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-xs);
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    font-weight: 500;
  }

  /* ── Decision Pipeline Topo ─────────────────────────── */
  .preview-steps-container > strong {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    display: block;
    margin-bottom: 12px;
  }

  .decision-pipeline {
    display: flex;
    flex-direction: column;
    gap: 10px;
    position: relative;
    padding-left: 18px;
  }

  .decision-pipeline::before {
    content: '';
    position: absolute;
    left: 5px;
    top: 8px;
    bottom: 8px;
    width: 1px;
    background: var(--connector-line);
  }

  .decision-step {
    position: relative;
    font-size: 12px;
    color: var(--text-secondary);
    padding: 8px 14px;
    border-radius: var(--radius-sm);
    background: var(--glass-bg-subtle);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .decision-step::before {
    content: '';
    position: absolute;
    left: -16px;
    top: 50%;
    transform: translateY(-50%);
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-muted);
    border: 2px solid var(--bg-primary);
  }

  .decision-step.matched {
    border-color: rgba(16, 185, 129, 0.2);
    background: rgba(16, 185, 129, 0.03);
    color: var(--text-primary);
  }

  .decision-step.matched::before {
    background: #10b981;
    box-shadow: 0 0 6px #10b981;
  }

  .step-code {
    color: var(--text-muted);
    background: var(--bg-badge);
    padding: 2px 6px;
    border-radius: var(--radius-xs);
    font-size: 11px;
  }

  .decision-step.matched .step-code {
    color: var(--success-text);
  }

  :global(html[data-theme="light"]) .decision-step.matched .step-code {
    background: var(--bg-badge);
  }

  .provider-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: var(--radius-xs);
    background: var(--bg-badge);
    border: 1px solid var(--border);
    color: var(--text-secondary);
  }
</style>
