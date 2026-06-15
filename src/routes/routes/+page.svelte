<script lang="ts">
  import { afterNavigate } from '$app/navigation';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import { dataRevision } from '$lib/data-revision.svelte';
  import type { ApiKeyConfig, Model, Provider, RankedModelSummary, RouteExplainResult, RoutableModel } from '$lib/types';
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
  ] as const;

  let hasLoaded = $state(false);
  let loadSeq = 0;

  async function loadRouteData(options?: { silent?: boolean }) {
    const seq = ++loadSeq;
    if (!options?.silent) loading = true;
    try {
      const [providersList, routableList] = await Promise.all([
        api.providers.list(),
        api.models.listRoutable(),
      ]);
      if (seq !== loadSeq) return;
      providers = providersList;
      routableModels = routableList;
      hasLoaded = true;
    } catch (e) {
      if (seq !== loadSeq) return;
      toast.error(e instanceof Error ? e.message : i18n.t('routes.load_failed'));
    } finally {
      if (seq !== loadSeq) return;
      if (!options?.silent) loading = false;
    }
  }

  afterNavigate(({ to }) => {
    if (to?.url.pathname === '/routes') {
      loadRouteData({ silent: hasLoaded });
    }
  });

  $effect(() => {
    void dataRevision.models;
    void dataRevision.providers;
    if (!hasLoaded || $page.url.pathname !== '/routes') return;
    loadRouteData({ silent: true });
  });

  // derived provider map for fast lookup
  const providerMap = $derived(new Map(providers.map((p) => [p.id, p])));

  // Helper to sort routes by strategy (mirrors cab-core routing engine)
  const INPUT_OUTPUT_RATIO = 10;
  const INPUT_CACHE_HIT_RATE = 0.9;

  function isKeyRateLimited(key: ApiKeyConfig): boolean {
    if (!key.quota_reset_at) return false;
    const resetAt = Date.parse(key.quota_reset_at);
    return Number.isFinite(resetAt) && resetAt > Date.now();
  }

  function providerHasSubscribedKeyConfigured(providerId: string): boolean {
    const provider = providerMap.get(providerId);
    if (!provider) return false;
    return provider.api_keys.some(
      (key) => key.enabled && key.key.trim().length > 0 && key.subscribed
    );
  }

  function providerHasSubscribedKey(providerId: string): boolean {
    const provider = providerMap.get(providerId);
    if (!provider) return false;
    return provider.api_keys.some(
      (key) =>
        key.enabled && key.key.trim().length > 0 && key.subscribed && !isKeyRateLimited(key)
    );
  }

  type StrategyCandidate = {
    route: RoutableModel;
    model: Model;
    score: number | null;
    serviceProviderId: string;
    displayStrategy: string;
  };

  type StrategyCandidatePools = {
    displayStrategy: string;
    subscribed: StrategyCandidate[];
    payg: StrategyCandidate[];
  };

  /** Pure strategy metric — never adjusted for subscription. */
  function strategyMetricValue(route: RoutableModel, strategy: string): number | null {
    if (strategy === 'cheapest') {
      return endpointEffectiveTokenCost(route);
    }
    if (strategy === 'intelligent') {
      return capabilityIndicesMissing(route, strategy) ? null : (route.coding_index ?? null);
    }
    if (strategy === 'speed') {
      return modelOutputSpeed(route) || null;
    }
    const previewTask = strategy === 'auto' ? 'coding' : 'coding';
    if (strategy === 'balanced') {
      return capabilityIndicesMissing(route, strategy)
        ? null
        : capabilityValueScore(route, primaryCapability(route, previewTask));
    }
    if (strategy === 'auto') {
      return capabilityIndicesMissing(route, strategy)
        ? null
        : capabilityValueScore(
            route,
            hasCompositeIndices(route)
              ? compositeCapability(route, previewTask)
              : primaryCapability(route, previewTask)
          );
    }
    return null;
  }

  function compareStrategyCandidates(
    a: { route: RoutableModel; model: Model; score: number | null; serviceProviderId: string },
    b: { route: RoutableModel; model: Model; score: number | null; serviceProviderId: string },
    strategy: string
  ): number {
    const scoreOf = (s: number | null) => (s == null ? Number.NEGATIVE_INFINITY : s);
    if (strategy === 'cheapest') {
      if (a.score !== b.score) return (a.score ?? 0) - (b.score ?? 0);
      return (
        a.model.name.localeCompare(b.model.name) ||
        a.serviceProviderId.localeCompare(b.serviceProviderId)
      );
    }
    if (strategy === 'intelligent') {
      if (b.score !== a.score) return scoreOf(b.score) - scoreOf(a.score);
      return endpointEffectiveTokenCost(a.route) - endpointEffectiveTokenCost(b.route);
    }
    if (strategy === 'speed') {
      if (b.score !== a.score) return scoreOf(b.score) - scoreOf(a.score);
      const ttftDiff = modelTimeToFirstToken(a.model) - modelTimeToFirstToken(b.model);
      if (ttftDiff !== 0) return ttftDiff;
      return endpointEffectiveTokenCost(a.route) - endpointEffectiveTokenCost(b.route);
    }
    if (strategy === 'balanced' || strategy === 'auto') {
      if (b.score !== a.score) return scoreOf(b.score) - scoreOf(a.score);
      return endpointEffectiveTokenCost(a.route) - endpointEffectiveTokenCost(b.route);
    }
    return 0;
  }

  function compareRoutingCandidates(
    a: { route: RoutableModel; model: Model; score: number | null; serviceProviderId: string },
    b: { route: RoutableModel; model: Model; score: number | null; serviceProviderId: string },
    strategy: string
  ): number {
    const tier = (providerId: string) => (providerHasSubscribedKey(providerId) ? 0 : 1);
    const subscribedDiff = tier(a.serviceProviderId) - tier(b.serviceProviderId);
    if (subscribedDiff !== 0) return subscribedDiff;
    return compareStrategyCandidates(a, b, strategy);
  }

  function candidateKey(candidate: {
    model: Model;
    serviceProviderId: string;
  }): string {
    return `${candidate.model.name}\0${candidate.serviceProviderId}`;
  }

  function routingRankMap(
    candidates: StrategyCandidate[],
    strategy: string
  ): Map<string, number> {
    const sorted = [...candidates].sort((a, b) =>
      compareRoutingCandidates(a, b, strategy)
    );
    const ranks = new Map<string, number>();
    sorted.forEach((candidate, index) => {
      ranks.set(candidateKey(candidate), index + 1);
    });
    return ranks;
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

  function hasKnownPricing(route: RoutableModel): boolean {
    return routeInputCost(route) != null && routeOutputCost(route) != null;
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

  function capabilityValueScore(route: RoutableModel, capability: number): number {
    const input = routeInputCost(route);
    const output = routeOutputCost(route);
    if (input == null || output == null) {
      return Number.NEGATIVE_INFINITY;
    }
    const raw = endpointEffectiveTokenCost(route);
    if (raw <= 0) {
      return Number.POSITIVE_INFINITY;
    }
    return capability / raw;
  }

  function modelOutputSpeed(model: Model): number {
    const speed = model.output_speed_tps ?? 0;
    return speed > 0 ? speed : 0;
  }

  function modelTimeToFirstToken(model: Model): number {
    return model.time_to_first_token_secs ?? Number.POSITIVE_INFINITY;
  }

  function capabilityIndicesMissing(model: Model, strategy: string): boolean {
    if (strategy === 'cheapest') return false;
    if (strategy === 'intelligent') return model.coding_index == null;
    if (strategy === 'speed') return (model.output_speed_tps ?? 0) <= 0;
    return model.coding_index == null && model.overall_intelligence == null;
  }

  function hasCompositeIndices(model: Model): boolean {
    return (
      model.overall_intelligence != null &&
      model.coding_index != null &&
      model.agentic_index != null &&
      model.math_index != null
    );
  }

  function compositeCapability(model: Model, task: 'coding' | 'math' | 'agentic' | 'general'): number {
    const overall = model.overall_intelligence!;
    const coding = model.coding_index!;
    const agentic = model.agentic_index!;
    const math = model.math_index!;
    const weighted = (parts: [number, number][]) =>
      parts.reduce((sum, [score, weight]) => sum + score * weight, 0);
    if (task === 'coding') {
      return weighted([
        [coding, 0.55],
        [overall, 0.22],
        [agentic, 0.13],
        [math, 0.1],
      ]);
    }
    if (task === 'math') {
      return weighted([
        [math, 0.58],
        [overall, 0.24],
        [coding, 0.1],
        [agentic, 0.08],
      ]);
    }
    if (task === 'agentic') {
      return weighted([
        [agentic, 0.42],
        [overall, 0.28],
        [coding, 0.22],
        [math, 0.08],
      ]);
    }
    return weighted([
      [overall, 0.45],
      [coding, 0.22],
      [math, 0.18],
      [agentic, 0.15],
    ]);
  }

  function primaryCapability(
    model: Model,
    task: 'coding' | 'math' | 'agentic' | 'general'
  ): number {
    if (task === 'coding') return model.coding_index ?? model.overall_intelligence ?? 0;
    if (task === 'math') return model.math_index ?? model.overall_intelligence ?? 0;
    if (task === 'agentic') return model.agentic_index ?? model.overall_intelligence ?? 0;
    return model.overall_intelligence ?? 0;
  }

  function resolveRoutesForStrategy(
    strategy: string,
    routes: RoutableModel[]
  ): StrategyCandidatePools {
    const enabled = routes.filter(hasKnownPricing);
    const empty: StrategyCandidatePools = {
      displayStrategy: strategy,
      subscribed: [],
      payg: [],
    };
    if (enabled.length === 0) return empty;

    const hasSpeedData = enabled.some((r) => modelOutputSpeed(r) > 0);
    const displayStrategy =
      strategy === 'speed' && !hasSpeedData ? 'cheapest' : strategy;

    const mapped: StrategyCandidate[] = enabled.map((r) => ({
      route: r,
      model: r,
      score: strategyMetricValue(r, displayStrategy),
      serviceProviderId: r.service_provider_id,
      displayStrategy,
    }));

    const sortPool = (pool: StrategyCandidate[]) =>
      pool.sort((a, b) => compareStrategyCandidates(a, b, displayStrategy));

    return {
      displayStrategy,
      subscribed: sortPool(
        mapped.filter((c) => providerHasSubscribedKeyConfigured(c.serviceProviderId))
      ),
      payg: sortPool(
        mapped.filter((c) => !providerHasSubscribedKeyConfigured(c.serviceProviderId))
      ),
    };
  }

  const hasConfiguredSubscription = $derived(
    providers.some((p) => p.enabled && providerHasSubscribedKeyConfigured(p.id))
  );
  const hasActiveSubscription = $derived(
    providers.some((p) => p.enabled && providerHasSubscribedKey(p.id))
  );
  const subscriptionQuotaPaused = $derived(
    hasConfiguredSubscription && !hasActiveSubscription
  );

  function collapsedPoolLimits(
    subscribedLen: number,
    paygLen: number,
    previewLimit: number
  ): { subscribed: number; payg: number } {
    if (subscribedLen === 0) {
      return { subscribed: 0, payg: Math.min(paygLen, previewLimit) };
    }
    if (paygLen === 0) {
      return { subscribed: Math.min(subscribedLen, previewLimit), payg: 0 };
    }
    let subscribed = Math.min(subscribedLen, 3);
    let payg = Math.min(paygLen, previewLimit - subscribed);
    if (payg === 0) {
      subscribed = Math.min(subscribedLen, previewLimit - 1);
      payg = 1;
    }
    return { subscribed, payg };
  }

  function flattenCandidatePools(
    pools: StrategyCandidatePools,
    expanded: boolean,
    previewLimit = 5
  ): { label: string | null; items: StrategyCandidate[]; rankOffset: number }[] {
    const groups: { label: string | null; items: StrategyCandidate[]; rankOffset: number }[] =
      [];
    let rankOffset = 0;

    if (expanded) {
      if (pools.subscribed.length > 0) {
        groups.push({
          label: i18n.t('routes.subscribed_pool'),
          items: pools.subscribed,
          rankOffset,
        });
        rankOffset += pools.subscribed.length;
      }
      if (pools.payg.length > 0) {
        groups.push({
          label: i18n.t('routes.payg_pool'),
          items: pools.payg,
          rankOffset,
        });
      }
      return groups;
    }

    const limits = collapsedPoolLimits(
      pools.subscribed.length,
      pools.payg.length,
      previewLimit
    );
    if (limits.subscribed > 0) {
      const items = pools.subscribed.slice(0, limits.subscribed);
      groups.push({ label: i18n.t('routes.subscribed_pool'), items, rankOffset });
      rankOffset += items.length;
    }
    if (limits.payg > 0) {
      const items = pools.payg.slice(0, limits.payg);
      groups.push({ label: i18n.t('routes.payg_pool'), items, rankOffset });
    }
    return groups;
  }

  function groupPreviewCandidates(candidates: RankedModelSummary[]) {
    const subscribed = candidates.filter((c) =>
      providerHasSubscribedKeyConfigured(c.provider_id)
    );
    const payg = candidates.filter(
      (c) => !providerHasSubscribedKeyConfigured(c.provider_id)
    );
    return { subscribed, payg };
  }

  function strategyMetricLabel(strategyId: string): string {
    if (strategyId === 'speed') return i18n.t('routes.speed');
    if (strategyId === 'cheapest') return i18n.t('routes.composite_price');
    if (strategyId === 'balanced' || strategyId === 'auto') return i18n.t('routes.value_score');
    return i18n.t('routes.intel');
  }

  function formatStrategyMetric(
    strategyId: string,
    candidate: { model: Model; score: number | null }
  ): string {
    if (candidate.score == null || capabilityIndicesMissing(candidate.model, strategyId)) {
      return '—';
    }
    if (strategyId === 'speed') {
      return `${candidate.score.toFixed(1)} t/s`;
    }
    if (strategyId === 'cheapest') {
      return `$${candidate.score.toFixed(2)}`;
    }
    if (strategyId === 'balanced' || strategyId === 'auto') {
      return Number.isFinite(candidate.score) ? candidate.score.toFixed(2) : '∞';
    }
    return candidate.score.toFixed(1);
  }

  function formatExplainValue(candidate: RankedModelSummary): string {
    if (candidate.value != null && Number.isFinite(candidate.value)) {
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
        <div class="preview-block">
          <strong>{i18n.t('routes.preview_resolved')}</strong>
          <span>
            {previewResult.resolved.model_id} · {providerMap.get(previewResult.resolved.provider_id)?.name ?? previewResult.resolved.provider_id}
            {#if previewResult.resolved.strategy}
              · {previewResult.resolved.strategy}
            {/if}
          </span>
        </div>
      {/if}
      <div class="preview-block">
        <strong>{i18n.t('routes.preview_steps')}</strong>
        <ul>
          {#each previewResult.decision_steps as step}
            <li class:matched={step.matched} class:missed={!step.matched}>
              <code>{step.step}</code> — {step.detail}
            </li>
          {/each}
        </ul>
      </div>
      {#if previewResult.ranked_candidates.length > 0}
        {@const previewPools = groupPreviewCandidates(previewResult.ranked_candidates)}
        {@const previewRankById = new Map(
          previewResult.ranked_candidates.map((candidate, index) => [
            `${candidate.model_id}\0${candidate.provider_id}`,
            index + 1,
          ])
        )}
        <div class="preview-block">
          <strong>{i18n.t('routes.preview_candidates')}</strong>
          {#if subscriptionQuotaPaused}
            <p class="pool-quota-note">{i18n.t('routes.subscription_quota_paused')}</p>
          {/if}
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
                {#if previewPools.subscribed.length > 0}
                  <tr class="pool-divider">
                    <td colspan="5">{i18n.t('routes.subscribed_pool')}</td>
                  </tr>
                  {#each previewPools.subscribed as candidate}
                    <tr>
                      <td>{previewRankById.get(`${candidate.model_id}\0${candidate.provider_id}`) ?? '—'}</td>
                      <td>
                        <div class="c-model-row">
                          <span>{candidate.model_id}</span>
                          <span
                            class="subscribed-tag"
                            class:paused={!providerHasSubscribedKey(candidate.provider_id)}
                            title={providerHasSubscribedKey(candidate.provider_id)
                              ? i18n.t('routes.subscribed_tag_tip')
                              : i18n.t('routes.subscribed_tag_paused_tip')}
                          >
                            {i18n.t('routes.subscribed_tag')}
                          </span>
                        </div>
                      </td>
                      <td>{providerMap.get(candidate.provider_id)?.name ?? candidate.provider_id}</td>
                      <td>{candidate.capability != null ? candidate.capability.toFixed(1) : '—'}</td>
                      <td>{formatExplainValue(candidate)}</td>
                    </tr>
                  {/each}
                {/if}
                {#if previewPools.payg.length > 0}
                  <tr class="pool-divider">
                    <td colspan="5">{i18n.t('routes.payg_pool')}</td>
                  </tr>
                  {#each previewPools.payg as candidate}
                    <tr>
                      <td>{previewRankById.get(`${candidate.model_id}\0${candidate.provider_id}`) ?? '—'}</td>
                      <td>
                        <div class="c-model-row">
                          <span>{candidate.model_id}</span>
                        </div>
                      </td>
                      <td>{providerMap.get(candidate.provider_id)?.name ?? candidate.provider_id}</td>
                      <td>{candidate.capability != null ? candidate.capability.toFixed(1) : '—'}</td>
                      <td>{formatExplainValue(candidate)}</td>
                    </tr>
                  {/each}
                {/if}
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
      {@const pools = resolveRoutesForStrategy(s.id, routableModels)}
      {@const totalCandidates = pools.subscribed.length + pools.payg.length}
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
              {@const poolGroups = flattenCandidatePools(pools, isExpanded)}
              {@const routingRanks = routingRankMap(
                [...pools.subscribed, ...pools.payg],
                pools.displayStrategy
              )}
              {#if subscriptionQuotaPaused}
                <p class="pool-quota-note">{i18n.t('routes.subscription_quota_paused')}</p>
              {/if}
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
                    {#each poolGroups as group}
                      <tr class="pool-divider">
                        <td colspan="5">{group.label}</td>
                      </tr>
                      {#each group.items as c, idx}
                        {@const provider = providerMap.get(c.serviceProviderId)}
                        <tr>
                          <td class="mono text-muted" style="text-align: center;">
                            {routingRanks.get(candidateKey(c)) ?? '—'}
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
                                {#if providerHasSubscribedKeyConfigured(c.serviceProviderId)}
                                  <span
                                    class="subscribed-tag"
                                    class:paused={!providerHasSubscribedKey(c.serviceProviderId)}
                                    title={providerHasSubscribedKey(c.serviceProviderId)
                                      ? i18n.t('routes.subscribed_tag_tip')
                                      : i18n.t('routes.subscribed_tag_paused_tip')}
                                  >
                                    {i18n.t('routes.subscribed_tag')}
                                  </span>
                                {/if}
                              </div>
                              <span class="c-slug mono">{c.model.name}</span>
                            </div>
                          </td>
                          <td style="text-align: right;" class="mono text-secondary">
                            {formatPricePair(c.route)}
                          </td>
                          <td style="text-align: right;" class="mono text-accent">
                            {formatStrategyMetric(c.displayStrategy, c)}
                          </td>
                        </tr>
                      {/each}
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
  .pool-quota-note {
    margin: 0 0 10px;
    padding: 8px 10px;
    border-radius: var(--radius-md);
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.25);
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.5;
  }

  .pool-divider td {
    padding: 10px 12px 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
  }

  .pool-divider:first-child td {
    border-top: none;
  }

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
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: var(--radius-full);
    background: rgba(34, 197, 94, 0.08);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.15);
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
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.04);
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
    border: 1px solid rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .pb-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    text-align: left;
  }

  .pb-table th {
    background: rgba(255, 255, 255, 0.01);
    color: var(--text-muted);
    font-weight: 600;
    padding: 8px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  .pb-table td {
    padding: 8px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.02);
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
  }

  .subscribed-tag {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    padding: 3px 6px;
    border-radius: 999px;
    color: #86efac;
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.28);
    letter-spacing: 0.02em;
  }

  .subscribed-tag.paused {
    color: #fcd34d;
    background: rgba(245, 158, 11, 0.1);
    border-color: rgba(245, 158, 11, 0.35);
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
    background: rgba(255, 255, 255, 0.01);
    border: 1px dashed rgba(255, 255, 255, 0.05);
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
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.05);
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
    font-size: 13px;
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
    font-size: 12px;
    color: var(--text-secondary);
  }

  .preview-form select,
  .preview-form input,
  .preview-form textarea {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    padding: 8px 10px;
    font: inherit;
  }

  .preview-prompt {
    grid-column: 1 / -1;
  }

  .preview-btn {
    grid-column: 1 / -1;
    justify-self: start;
    padding: 8px 14px;
    border-radius: var(--radius-sm);
    border: 1px solid rgba(99, 102, 241, 0.35);
    background: rgba(99, 102, 241, 0.12);
    color: #c7d2fe;
    cursor: pointer;
  }

  .preview-btn:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .preview-result {
    margin-top: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .preview-block ul {
    margin: 8px 0 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .preview-block li.matched code {
    color: #86efac;
  }

  .preview-block li.missed code {
    color: #fca5a5;
  }

  .provider-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(99, 102, 241, 0.1);
    border: 1px solid rgba(99, 102, 241, 0.2);
    color: #818cf8;
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
