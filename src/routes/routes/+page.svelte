<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Model, Provider } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { toast } from '$lib/components/Toast.svelte';

  let models = $state<Model[]>([]);
  let providers = $state<Provider[]>([]);
  let loading = $state(true);
  let expandedStrategies = $state<Record<string, boolean>>({});

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
  ] as const;

  onMount(async () => {
    loading = true;
    try {
      const [providersList, modelsList] = await Promise.all([
        api.providers.list(),
        api.models.list(),
      ]);
      providers = providersList;
      models = modelsList;
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('routes.load_failed'));
    } finally {
      loading = false;
    }
  });

  // derived provider map for fast lookup
  const providerMap = $derived(new Map(providers.map((p) => [p.id, p])));

  // Helper to sort models by strategy (mirrors cab-core routing engine)
  const INPUT_OUTPUT_RATIO = 3;

  function effectiveTokenCost(model: Model): number {
    const input = model.input_cost ?? 0;
    const output = model.output_cost ?? 0;
    return Math.max(input * INPUT_OUTPUT_RATIO + output, 0.001);
  }

  function primaryCapability(
    model: Model,
    task: 'coding' | 'math' | 'agentic' | 'general'
  ): number {
    if (task === 'coding') return model.coding_index;
    if (task === 'math') return model.math_index ?? model.overall_intelligence;
    if (task === 'agentic') return model.agentic_index;
    return model.overall_intelligence;
  }

  function resolveModelsForStrategy(strategy: string, list: Model[]) {
    // Find active provider IDs: enabled
    const activeProviderIds = new Set(providers.filter((p) => p.enabled).map((p) => p.id));

    // Filter active and non-negative priced models
    const enabled = list.filter(
      (m) =>
        m.enabled &&
        activeProviderIds.has(m.provider_id) &&
        (m.input_cost ?? 0) >= 0 &&
        (m.output_cost ?? 0) >= 0
    );

    if (enabled.length === 0) return [];

    const previewTask = strategy === 'auto' ? 'coding' : 'coding';

    const mapped = enabled.map((m) => {
      let score = 0;
      if (strategy === 'cheapest') {
        score = effectiveTokenCost(m);
      } else if (strategy === 'intelligent') {
        score = m.coding_index;
      } else if (strategy === 'balanced') {
        score = primaryCapability(m, previewTask) / effectiveTokenCost(m);
      } else {
        score =
          (m.coding_index * 0.55 +
            m.overall_intelligence * 0.22 +
            m.agentic_index * 0.13 +
            (m.math_index ?? 30) * 0.1) /
          effectiveTokenCost(m);
      }
      return { model: m, score };
    });

    mapped.sort((a, b) => {
      if (strategy === 'cheapest') {
        if (a.score !== b.score) return a.score - b.score;
        return b.model.coding_index - a.model.coding_index;
      } else if (strategy === 'intelligent') {
        if (b.score !== a.score) return b.score - a.score;
        return effectiveTokenCost(a.model) - effectiveTokenCost(b.model);
      } else if (strategy === 'balanced' || strategy === 'auto') {
        if (b.score !== a.score) return b.score - a.score;
        return effectiveTokenCost(a.model) - effectiveTokenCost(b.model);
      }
      return 0;
    });

    return mapped;
  }
</script>

<PageHeader title={i18n.t('routes.title')} description={i18n.t('routes.page_desc')} />

{#if loading}
  <div class="strategy-list">
    {#each Array(4) as _}
      <div class="skeleton" style="height: 320px; border-radius: var(--radius-lg);"></div>
    {/each}
  </div>
{:else}
  <div class="strategy-list">
    {#each STRATEGIES as s}
      {@const candidates = resolveModelsForStrategy(s.id, models)}
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
            {#if candidates.length > 0}
              {@const isExpanded = expandedStrategies[s.id] ?? false}
              {@const visibleCandidates = isExpanded ? candidates : candidates.slice(0, 5)}
              <div class="pb-table-wrap">
                <table class="pb-table">
                  <thead>
                    <tr>
                      <th style="width: 50px; text-align: center;">{i18n.t('routes.rank')}</th>
                      <th style="width: 90px;">{i18n.t('routes.provider')}</th>
                      <th>{i18n.t('routes.model_name')}</th>
                      <th style="text-align: right; width: 130px;">{i18n.t('routes.price')}</th>
                      <th style="text-align: right; width: 70px;">{i18n.t('routes.intel')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each visibleCandidates as c, idx}
                      {@const provider = providerMap.get(c.model.provider_id)}
                      <tr>
                        <td class="mono text-muted" style="text-align: center;">{idx + 1}</td>
                        <td>
                          <span class="provider-badge">
                            {provider ? provider.name : c.model.provider_id}
                          </span>
                        </td>
                        <td>
                          <div class="c-model-cell">
                            <span class="c-name">{c.model.display_name}</span>
                            <span class="c-slug mono">{c.model.name}</span>
                          </div>
                        </td>
                        <td style="text-align: right;" class="mono text-secondary">
                          ${c.model.input_cost?.toFixed(2)} / ${c.model.output_cost?.toFixed(2)}
                        </td>
                        <td style="text-align: right;" class="mono text-accent">
                          {c.model.coding_index.toFixed(1)}
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              {#if candidates.length > 5}
                <div style="display:flex; justify-content:center; margin-top: 8px;">
                  <button
                    class="btn btn-ghost btn-xs"
                    style="color: var(--accent); font-weight: 600; font-size: 11px;"
                    onclick={() => (expandedStrategies[s.id] = !isExpanded)}
                  >
                    {#if isExpanded}
                      {i18n.t('routes.show_less')}
                    {:else}
                      {i18n.tParams('routes.show_all_candidates', { count: candidates.length })}
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

  .c-name {
    font-weight: 500;
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
