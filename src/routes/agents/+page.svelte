<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Agent, Model, Provider, UpdateAgent, Settings } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';

  let agents = $state<Agent[]>([]);
  let models = $state<Model[]>([]);
  let providers = $state<Provider[]>([]);
  let routes = $state<any[]>([]); // All custom routing rules
  let settings = $state<Settings | null>(null);
  let loading = $state(true);
  let savingId = $state<string | null>(null);
  let installingProxyId = $state<string | null>(null);
  let proxyLaunchByAgent = $state<Record<string, string>>({});

  // Form states mapped by agent ID to prevent form conflicts
  let agentForms = $state<Record<string, { model_id: string; api_key: string; endpoint: string }>>({});

  function countManualModels(allModels: Model[], allProviders: Provider[]): number {
    const activeProviderIds = new Set(
      allProviders
        .filter((p) => p.enabled && (p.api_key || p.id === 'provider-ollama'))
        .map((p) => p.id)
    );
    return allModels.filter((m) => m.enabled && activeProviderIds.has(m.provider_id)).length;
  }

  function supportsProxyMode(agentId: string): boolean {
    return agentId === 'antigravity';
  }

  function normalizeLoadedMode(mode: string): Agent['mode'] {
    if (mode === 'config') return 'auto';
    return mode as Agent['mode'];
  }

  function modeLabel(mode: Agent['mode']): string {
    if (mode === 'native') return i18n.t('agents.mode_native');
    if (mode === 'auto') return i18n.t('agents.mode_auto');
    if (mode === 'manual') return i18n.t('agents.mode_manual');
    return i18n.t('agents.mode_proxy');
  }

  function modeShortLabel(mode: Agent['mode']): string {
    if (mode === 'native') return i18n.t('agents.mode_native_short');
    if (mode === 'auto') return i18n.t('agents.mode_auto_short');
    if (mode === 'manual') return i18n.t('agents.mode_manual_short');
    return i18n.t('agents.mode_proxy_short');
  }

  function modeBadgeClass(mode: Agent['mode']): string {
    if (mode === 'native') return 'badge-neutral';
    if (mode === 'auto') return 'badge-warning';
    if (mode === 'manual') return 'badge-success';
    return 'badge-proxy';
  }

  function getRouteDisplayName(id: string | null | undefined): string {
    if (!id) return '';
    if (id === 'auto') return i18n.t('routes.strategies.auto.label');
    if (id === 'balanced') return i18n.t('routes.strategies.balanced.label');
    if (id === 'intelligent') return i18n.t('routes.strategies.intelligent.label');
    if (id === 'price') return i18n.t('routes.strategies.cheapest.label');
    return routes.find(r => r.id === id)?.name || id;
  }

  onMount(async () => {
    loading = true;
    try {
      const [rawAgents, rawModels, rawProviders, rawRoutes, rawSettings] = await Promise.all([
        api.agents.list(),
        api.models.list(),
        api.providers.list(),
        api.routes.list(),
        api.settings.get().catch(() => ({ gateway_port: 3125 } as Settings))
      ]);
      agents = rawAgents.map(a => ({
        ...a,
        mode: normalizeLoadedMode(a.mode)
      }));
      models = rawModels.filter(m => m.enabled);
      providers = rawProviders;
      routes = rawRoutes;
      settings = rawSettings;
      
      // Initialize forms with current agent details
      const initialForms: Record<string, { model_id: string; api_key: string; endpoint: string }> = {};
      for (const a of rawAgents) {
        const normalizedMode = normalizeLoadedMode(a.mode);
        initialForms[a.id] = {
          model_id: normalizedMode === 'manual' ? '' : (a.model_id || 'auto'),
          api_key: a.api_key || '',
          endpoint: a.endpoint || ''
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
      } else if ((mode === 'auto' || mode === 'proxy') && !agentForms[agent.id]?.model_id) {
        updates.model_id = 'auto';
        agentForms[agent.id] = { ...agentForms[agent.id], model_id: 'auto' };
      }
      const updated = await api.agents.update(agent.id, updates);
      const idx = agents.findIndex(a => a.id === agent.id);
      if (idx !== -1) {
        agents[idx] = {
          ...updated,
          mode: normalizeLoadedMode(updated.mode),
          model_name: getRouteDisplayName(updated.model_id) || undefined
        };
      }
      if (mode === 'proxy' && supportsProxyMode(agent.id)) {
        await handleInstallProxy(agent.id, false);
      }
      const modeText = modeShortLabel(mode);
      toast.success(
        i18n.t('agents.mode_switched')
          .replace('{name}', agent.name)
          .replace('{mode}', modeText)
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
    const agent = agents.find(a => a.id === agentId);
    if (!agent) return;
    
    savingId = agentId;
    try {
      const updates: UpdateAgent = {
        api_key: '',
        endpoint: ''
      };
      if (agent.mode === 'auto' || agent.mode === 'proxy') {
        updates.model_id = form.model_id || 'auto';
      } else if (agent.mode === 'manual') {
        updates.model_id = null;
      }
      
      const updated = await api.agents.update(agentId, updates);
      const idx = agents.findIndex(a => a.id === agentId);
      if (idx !== -1) {
        agents[idx] = {
          ...updated,
          model_name: getRouteDisplayName(updated.model_id) || undefined
        };
      }
      toast.success(i18n.t('agents.save_success'));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('agents.save_details_failed'));
    } finally {
      savingId = null;
    }
  }

  async function handleInstallProxy(agentId: string, showToast = true) {
    installingProxyId = agentId;
    try {
      const res = await api.agents.installProxy(agentId);
      proxyLaunchByAgent = { ...proxyLaunchByAgent, [agentId]: res.launch_example };
      if (showToast) {
        toast.success(i18n.t('agents.proxy_install_success'));
      }
    } catch (e) {
      if (showToast) {
        toast.error(e instanceof Error ? e.message : i18n.t('agents.proxy_install_failed'));
      } else {
        throw e;
      }
    } finally {
      installingProxyId = null;
    }
  }

  let hijacking = $state(false);

  async function handleHijackClaude() {
    hijacking = true;
    try {
      const res = await api.agents.hijackClaude();
      if (res.success) {
        toast.success(i18n.t('agents.hijack_success'));
      } else {
        toast.warning(res.message);
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('agents.hijack_failed'));
    } finally {
      hijacking = false;
    }
  }

  // Beautiful SVG icons map for Coding Agents
  const agentIcons: Record<string, string> = {
    cursor: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3l7.07 16.97 2.51-7.39 7.39-2.51L3 3z"/></svg>`,
    'claude-code': `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></svg>`,
    codex: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>`,
    opencode: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg>`,
    antigravity: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 16.5c-1.5 1.26-2.5 3.19-2.5 5.5h20c0-2.31-1-4.24-2.5-5.5"/><circle cx="12" cy="8" r="5"/><path d="M12 3v10"/><path d="M8 7h8"/></svg>`,
    hermes: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l7 4v6c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z"/><path d="M9 9h6"/><path d="M9 13h6"/><path d="M12 9v8"/></svg>`,
    kilocode: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 4h14v16H5z"/><path d="M9 8h6"/><path d="M9 12h3"/><path d="M9 16h6"/><path d="M15 12l2 2-2 2"/></svg>`,
    openclaw: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3h8l4 7-8 11L4 10l4-7z"/><path d="M8 3l4 18"/><path d="M16 3l-4 18"/><path d="M4 10h16"/></svg>`,
    pi: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 7h12"/><path d="M8 7v12"/><path d="M16 7v12"/><path d="M10 19h4"/><path d="M12 7c0-2 1-3 3-3h2"/><path d="M12 7c0-2-1-3-3-3H7"/></svg>`
  };
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
          <!-- Card Header -->
          <div class="agent-header">
            <div class="agent-icon" style="background: {agent.mode === 'native' ? 'rgba(255,255,255,0.03)' : 'rgba(59,130,246,0.1)'}; color: {agent.mode === 'native' ? 'var(--text-secondary)' : '#60a5fa'}">
              {@html agentIcons[agent.id] || agentIcons.codex}
            </div>
            <div class="agent-title-block">
              <h3>{agent.name}</h3>
              <span class="badge {modeBadgeClass(agent.mode)}">
                {modeLabel(agent.mode)}
              </span>
            </div>
          </div>

          <!-- Description -->
          <p class="agent-desc">
            {#if agent.mode === 'native'}
              {i18n.t('agents.native_desc')}
            {:else if agent.mode === 'auto'}
              {i18n.t('agents.auto_desc')}
            {:else if agent.mode === 'manual'}
              {i18n.t('agents.manual_desc')}
            {:else}
              {i18n.t('agents.proxy_desc')}
            {/if}
          </p>

          <!-- Mode Switcher Toggle Segment -->
          <div class="mode-segment" class:mode-segment-4={supportsProxyMode(agent.id)}>
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
            {#if supportsProxyMode(agent.id)}
              <button 
                class="segment-btn segment-btn-proxy" 
                class:active={agent.mode === 'proxy'} 
                onclick={() => handleModeChange(agent, 'proxy')}
                disabled={savingId === agent.id}
              >
                {i18n.t('agents.mode_proxy_short')}
              </button>
            {/if}
          </div>

          <!-- Dynamic inputs for auto, manual & proxy modes -->
          {#if agent.mode !== 'native' && agentForms[agent.id]}
            <div class="agent-inputs fade-in">
              {#if agent.mode === 'auto' || agent.mode === 'proxy'}
              <!-- Routing strategy selector (auto mode) -->
              <div class="form-group">
                <label class="label" for="{agent.id}-model">{i18n.t('agents.routing_strategy')}</label>
                <select class="select select-sm" id="{agent.id}-model" bind:value={agentForms[agent.id].model_id}>
                  <option value="">{i18n.t('agents.select_routing_strategy')}</option>
                  
                  <optgroup label={i18n.t('agents.builtin_system_routes')}>
                    <option value="auto">{i18n.t('agents.system_routes.auto')}</option>
                    <option value="balanced">{i18n.t('agents.system_routes.balanced')}</option>
                    <option value="intelligent">{i18n.t('agents.system_routes.intelligent')}</option>
                    <option value="price">{i18n.t('agents.system_routes.price')}</option>
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
                {i18n.t('agents.manual_hint').replace('{count}', countManualModels(models, providers).toString())}
              </p>
              {/if}

              <!-- Action buttons -->
              <div class="save-actions" style="display: flex; flex-direction: column; gap: 8px;">
                <button 
                  class="btn btn-primary btn-sm btn-full" 
                  onclick={() => handleSaveDetails(agent.id)}
                  disabled={savingId === agent.id}
                >
                  {savingId === agent.id ? i18n.t('common.loading') : i18n.t('common.save')}
                </button>

                {#if agent.mode === 'proxy' && supportsProxyMode(agent.id)}
                  <button 
                    class="btn btn-sm btn-full" 
                    style="border: 1px solid rgba(168, 85, 247, 0.4); background: rgba(168, 85, 247, 0.08); color: #c084fc;"
                    onclick={() => handleInstallProxy(agent.id)}
                    disabled={installingProxyId === agent.id}
                  >
                    {installingProxyId === agent.id ? i18n.t('agents.installing_proxy') : i18n.t('agents.install_proxy')}
                  </button>
                  {#if proxyLaunchByAgent[agent.id]}
                    <div class="proxy-launch">
                      <span class="label">{i18n.t('agents.proxy_launch_hint')}</span>
                      <code>{proxyLaunchByAgent[agent.id]}</code>
                    </div>
                  {/if}
                {/if}
                
                {#if agent.id === 'claude-code'}
                  <button 
                    class="btn btn-sm btn-full" 
                    style="border: 1px solid rgba(239, 68, 68, 0.4); background: rgba(239, 68, 68, 0.05); color: #f87171;"
                    onclick={handleHijackClaude}
                    disabled={hijacking}
                  >
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 6px;"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                    {hijacking ? i18n.t('agents.injecting_launching') : i18n.t('agents.hijack_launch_claude')}
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      </Card>
    {/each}
  </div>

  <!-- Detailed setup guides -->
  <div style="margin-top: 40px;">
    <Card padding="28px">
      <div class="guide-section">
        <h3 class="guide-title">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>
          </svg>
          <span>{i18n.t('agents.integration_guide_title')}</span>
        </h3>
        
        <div class="guides-tabs">
          <div class="guide-item">
            <h4>1. Cursor (Override Endpoint)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_cursor').replace('{port}', (settings?.gateway_port ?? 3125).toString())}
            </p>
          </div>

          <div class="guide-item">
            <h4>2. Claude Code (Automated JSON Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_claude_code')}
            </p>
          </div>

          <div class="guide-item">
            <h4>3. Codex (Automated TOML Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_codex')}
            </p>
          </div>

          <div class="guide-item">
            <h4>4. OpenCode (Automated JSON Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_opencode')}
            </p>
          </div>

          <div class="guide-item">
            <h4>5. Antigravity (Automated JSON Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_antigravity')}
            </p>
          </div>

          <div class="guide-item">
            <h4>6. Hermes Agent (Automated YAML Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_hermes')}
            </p>
          </div>

          <div class="guide-item">
            <h4>7. Kilo Code (Automated JSON Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_kilocode')}
            </p>
          </div>

          <div class="guide-item">
            <h4>8. OpenClaw (Automated CLI Config)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_openclaw')}
            </p>
          </div>

          <div class="guide-item">
            <h4>9. Pi (Automated JSON Write)</h4>
            <p class="guide-desc">
              {i18n.t('agents.guide_pi')}
            </p>
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

  .mode-segment-4 .segment-btn {
    font-size: 11px;
    padding: 6px 2px;
  }

  .segment-btn-proxy.active {
    background: rgba(168, 85, 247, 0.15);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }

  .badge-proxy {
    background: rgba(168, 85, 247, 0.12);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }

  .proxy-launch {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    background: rgba(168, 85, 247, 0.06);
    border: 1px dashed rgba(168, 85, 247, 0.25);
    border-radius: var(--radius-sm);
  }

  .proxy-launch code {
    font-size: 11px;
    word-break: break-all;
    color: #e9d5ff;
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
