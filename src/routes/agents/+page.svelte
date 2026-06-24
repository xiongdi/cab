<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Agent, Model, Provider, UpdateAgent, Settings } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { modeBadgeClass, normalizeLoadedMode } from '$lib/agents';

  let agents = $state<Agent[]>([]);
  let models = $state<Model[]>([]);
  let providers = $state<Provider[]>([]);
  let routes = $state<any[]>([]);
  let settings = $state<Settings | null>(null);
  let loading = $state(true);
  let savingId = $state<string | null>(null);

  let agentForms = $state<Record<string, { model_id: string; api_key: string; endpoint: string }>>(
    {}
  );

  function countManualModels(allModels: Model[], allProviders: Provider[]): number {
    const activeProviderIds = new Set(
      allProviders
        .filter((p) => p.enabled && (p.api_key || p.id === 'provider-ollama'))
        .map((p) => p.id)
    );
    return allModels.filter((m) => m.enabled && activeProviderIds.has(m.provider_id)).length;
  }

  function modeLabel(mode: Agent['mode']): string {
    if (mode === 'native') return i18n.t('agents.mode_native');
    if (mode === 'auto') return i18n.t('agents.mode_auto');
    return i18n.t('agents.mode_manual');
  }

  function modeShortLabel(mode: Agent['mode']): string {
    if (mode === 'native') return i18n.t('agents.mode_native_short');
    if (mode === 'auto') return i18n.t('agents.mode_auto_short');
    return i18n.t('agents.mode_manual_short');
  }

  function getRouteDisplayName(id: string | null | undefined): string {
    if (!id) return '';
    if (id === 'auto') return i18n.t('routes.strategies.auto.label');
    if (id === 'balanced') return i18n.t('routes.strategies.balanced.label');
    if (id === 'intelligent') return i18n.t('routes.strategies.intelligent.label');
    if (id === 'price') return i18n.t('routes.strategies.cheapest.label');
    if (id === 'speed') return i18n.t('routes.strategies.speed.label');
    return routes.find((r) => r.id === id)?.name || id;
  }

  onMount(async () => {
    loading = true;
    try {
      const [rawAgents, rawModels, rawProviders, rawRoutes, rawSettings] = await Promise.all([
        api.agents.list(),
        api.models.list(),
        api.providers.list(),
        api.routes.list(),
        api.settings.get().catch(() => ({ gateway_port: 3125 }) as Settings),
      ]);
      agents = rawAgents.map((a) => ({
        ...a,
        mode: normalizeLoadedMode(a.mode),
      }));
      models = rawModels.filter((m) => m.enabled);
      providers = rawProviders;
      routes = rawRoutes;
      settings = rawSettings;

      const initialForms: Record<string, { model_id: string; api_key: string; endpoint: string }> =
        {};
      for (const a of rawAgents) {
        const normalizedMode = normalizeLoadedMode(a.mode);
        initialForms[a.id] = {
          model_id: normalizedMode === 'manual' ? '' : a.model_id || 'auto',
          api_key: a.api_key || '',
          endpoint: a.endpoint || '',
        };
      }
      agentForms = initialForms;
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      loading = false;
    }
  });

  async function handleModeChange(agent: Agent, mode: Agent['mode']) {
    if (agent.mode === mode) return;
    savingId = agent.id;
    try {
      const updates: UpdateAgent = { mode };
      if (mode === 'manual') {
        updates.model_id = null;
      } else if (mode === 'auto' && !agentForms[agent.id]?.model_id) {
        updates.model_id = 'auto';
        agentForms[agent.id] = { ...agentForms[agent.id], model_id: 'auto' };
      }
      const updated = await api.agents.update(agent.id, updates);
      const idx = agents.findIndex((a) => a.id === agent.id);
      if (idx !== -1) {
        agents[idx] = {
          ...updated,
          mode: normalizeLoadedMode(updated.mode),
          model_name: getRouteDisplayName(updated.model_id) || undefined,
        };
      }
      toast.success(
        i18n
          .t('agents.mode_switched')
          .replace('{name}', agent.name)
          .replace('{mode}', modeShortLabel(mode))
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      savingId = null;
    }
  }

  async function handleSaveDetails(agentId: string) {
    const form = agentForms[agentId];
    if (!form) return;
    const agent = agents.find((a) => a.id === agentId);
    if (!agent) return;

    savingId = agentId;
    try {
      const updates: UpdateAgent = {
        api_key: '',
        endpoint: '',
      };
      if (agent.mode === 'auto') {
        updates.model_id = form.model_id || 'auto';
      } else if (agent.mode === 'manual') {
        updates.model_id = null;
      }

      const updated = await api.agents.update(agentId, updates);
      const idx = agents.findIndex((a) => a.id === agentId);
      if (idx !== -1) {
        agents[idx] = {
          ...updated,
          model_name: getRouteDisplayName(updated.model_id) || undefined,
        };
      }
      toast.success(i18n.t('agents.save_success'));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('agents.save_details_failed'));
    } finally {
      savingId = null;
    }
  }

</script>

<PageHeader title={i18n.t('agents.title')} description={i18n.t('agents.subtitle')} />

{#if loading}
  <div class="skeleton-grid">
    <div class="skeleton" style="height: 380px; border-radius: var(--radius-lg);"></div>
    <div class="skeleton" style="height: 380px; border-radius: var(--radius-lg);"></div>
    <div class="skeleton" style="height: 380px; border-radius: var(--radius-lg);"></div>
  </div>
{:else}
  <div class="agents-grid">
    {#each agents as agent}
      <Card padding="24px" hover={true} glow={agent.mode !== 'native'}>
        <div class="agent-card-content">
          <div class="agent-header">
            <div class="agent-icon" class:native={agent.mode === 'native'}>
              <img src="/agent-icons/{agent.id}.svg" alt={agent.name} loading="lazy" />
            </div>
            <div class="agent-title-block">
              <h3>{agent.name}</h3>
              <span class="badge {modeBadgeClass(agent.mode)}">
                {modeLabel(agent.mode)}
              </span>
            </div>
          </div>

          <p class="agent-desc">
            {#if agent.mode === 'native'}
              {i18n.t('agents.native_desc')}
            {:else if agent.mode === 'auto'}
              {i18n.t('agents.auto_desc')}
            {:else}
              {i18n.t('agents.manual_desc')}
            {/if}
          </p>

          <div class="mode-segment">
            <button
              class="segment-btn"
              class:active={agent.mode === 'native'}
              onclick={() => handleModeChange(agent, 'native')}
              disabled={savingId === agent.id}
            >
              {i18n.t('agents.mode_native_short')}
            </button>
            <button
              class="segment-btn"
              class:active={agent.mode === 'auto'}
              onclick={() => handleModeChange(agent, 'auto')}
              disabled={savingId === agent.id}
            >
              {i18n.t('agents.mode_auto_short')}
            </button>
            <button
              class="segment-btn"
              class:active={agent.mode === 'manual'}
              onclick={() => handleModeChange(agent, 'manual')}
              disabled={savingId === agent.id}
            >
              {i18n.t('agents.mode_manual_short')}
            </button>
          </div>

          {#if agent.mode !== 'native' && agentForms[agent.id]}
            <div class="agent-inputs fade-in">
              {#if agent.mode === 'auto'}
                <div class="form-group">
                  <label class="label" for="{agent.id}-model"
                    >{i18n.t('agents.routing_strategy')}</label
                  >
                  <select
                    class="select select-sm"
                    id="{agent.id}-model"
                    bind:value={agentForms[agent.id].model_id}
                  >
                    <option value="">{i18n.t('agents.select_routing_strategy')}</option>

                    <optgroup label={i18n.t('agents.builtin_system_routes')}>
                      <option value="auto">{i18n.t('agents.system_routes.auto')}</option>
                      <option value="balanced">{i18n.t('agents.system_routes.balanced')}</option>
                      <option value="intelligent"
                        >{i18n.t('agents.system_routes.intelligent')}</option
                      >
                      <option value="price">{i18n.t('agents.system_routes.price')}</option>
                      <option value="speed">{i18n.t('agents.system_routes.speed')}</option>
                    </optgroup>

                    {#if routes.length > 0}
                      <optgroup label={i18n.t('agents.custom_routing_rules')}>
                        {#each routes as r}
                          <option value={r.id}>⚡ {r.name} ({r.agent_pattern})</option>
                        {/each}
                      </optgroup>
                    {/if}
                  </select>
                </div>
              {:else}
                <p class="manual-hint">
                  {i18n
                    .t('agents.manual_hint')
                    .replace('{count}', countManualModels(models, providers).toString())}
                </p>
              {/if}

              <div class="save-actions">
                <button
                  class="btn btn-primary btn-sm btn-full"
                  onclick={() => handleSaveDetails(agent.id)}
                  disabled={savingId === agent.id}
                >
                  {savingId === agent.id ? i18n.t('common.loading') : i18n.t('common.save')}
                </button>
              </div>
            </div>
          {/if}
        </div>
      </Card>
    {/each}
  </div>

  <div style="margin-top: 40px;">
    <Card padding="28px">
      <div class="guide-section">
        <h3 class="guide-title">
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="var(--accent)"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="10" /><path d="M12 16v-4" /><path d="M12 8h.01" />
          </svg>
          <span>{i18n.t('agents.integration_guide_title')}</span>
        </h3>

        <div class="guides-tabs">
          <div class="guide-item">
            <h4>1. Claude Code</h4>
            <p class="guide-desc">{i18n.t('agents.guide_claude_code')}</p>
          </div>
          <div class="guide-item">
            <h4>2. Codex</h4>
            <p class="guide-desc">{i18n.t('agents.guide_codex')}</p>
          </div>
          <div class="guide-item">
            <h4>3. OpenCode</h4>
            <p class="guide-desc">{i18n.t('agents.guide_opencode')}</p>
          </div>
          <div class="guide-item">
            <h4>4. Hermes Agent</h4>
            <p class="guide-desc">{i18n.t('agents.guide_hermes')}</p>
          </div>
          <div class="guide-item">
            <h4>5. Kilo Code</h4>
            <p class="guide-desc">{i18n.t('agents.guide_kilocode')}</p>
          </div>
          <div class="guide-item">
            <h4>6. OpenClaw</h4>
            <p class="guide-desc">{i18n.t('agents.guide_openclaw')}</p>
          </div>
          <div class="guide-item">
            <h4>7. Pi</h4>
            <p class="guide-desc">{i18n.t('agents.guide_pi')}</p>
          </div>
          <div class="guide-item">
            <h4>8. Reasonix</h4>
            <p class="guide-desc">{i18n.t('agents.guide_reasonix')}</p>
          </div>
        </div>
      </div>
    </Card>
  </div>
{/if}

<style>
  .skeleton-grid,
  .agents-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 20px;
    margin-top: 8px;
  }

  .agent-card-content {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .agent-header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
  }

  .agent-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 46px;
    border-radius: var(--radius-md);
    transition: all var(--transition-fast);
    flex-shrink: 0;
    border: 1px solid var(--border);
    background: #fff;
    overflow: hidden;
    padding: 7px;
    box-sizing: border-box;
  }

  .agent-icon.native {
    filter: grayscale(1);
    opacity: 0.65;
  }

  .agent-icon img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }

  .agent-title-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
  }

  .agent-title-block h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .agent-desc {
    font-size: 12.5px;
    color: var(--text-muted);
    line-height: 1.6;
    margin: 0 0 20px 0;
    min-height: 52px;
  }

  .mode-segment {
    display: flex;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 3px;
    margin-bottom: 20px;
    gap: 2px;
  }

  .segment-btn {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
    padding: 6px 0;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all var(--transition-fast);
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

  .agent-inputs {
    display: flex;
    flex-direction: column;
    gap: 12px;
    border-top: 1px dashed var(--border);
    padding-top: 16px;
    margin-top: auto;
  }

  .save-actions {
    margin-top: 4px;
  }

  .btn-full {
    width: 100%;
    justify-content: center;
  }

  .manual-hint {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.6;
    margin: 0;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
  }

  .guide-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .guide-title {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 4px 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .guides-tabs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 24px;
  }

  @media (max-width: 900px) {
    .guides-tabs {
      grid-template-columns: 1fr;
      gap: 16px;
    }
  }

  .guide-item h4 {
    margin: 0 0 8px 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .guide-desc {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.6;
    margin: 0;
  }
</style>
