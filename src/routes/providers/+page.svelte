<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Provider, UpdateProvider, ApiKeyConfig, ProviderEndpoint, Model } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { dataRevision } from '$lib/data-revision.svelte';
  import CatalogLogo from '$lib/components/CatalogLogo.svelte';
  import { modelLabId } from '$lib/models-dev';

  type ProviderRow = Provider & { has_key: boolean };
  type SortKey = 'name' | 'id' | 'model_count' | 'enabled';

  let providers = $state<Provider[]>([]);
  let providerModelNames = $state<Record<string, string[]>>({});
  let loading = $state(true);
  let searchQuery = $state('');
  let statusFilter = $state<'all' | 'enabled' | 'disabled'>('all');
  let keyFilter = $state<'all' | 'configured' | 'missing'>('all');
  let sortKey = $state<SortKey>('name');
  let sortDir = $state<'asc' | 'desc'>('asc');
  let enabledDrafts = $state<Record<string, boolean>>({});
  let keyListDrafts = $state<Record<string, ApiKeyConfig[]>>({});
  let endpointDrafts = $state<Record<string, ProviderEndpoint[]>>({});
  let expandedProviderId = $state<string | null>(null);

  let providerRows = $derived<ProviderRow[]>(
    providers.map((provider) => {
      const draftKeys = keyListDrafts[provider.id] || [];
      return {
        ...provider,
        has_key: draftKeys.some((k) => k.key.trim().length > 0),
      };
    })
  );

  let rows = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    let result = providerRows.filter((provider) => {
      if (keyFilter === 'configured' && !provider.has_key) return false;
      if (keyFilter === 'missing' && provider.has_key) return false;
      if (statusFilter === 'enabled' && !enabledDrafts[provider.id]) return false;
      if (statusFilter === 'disabled' && enabledDrafts[provider.id]) return false;

      if (!query) return true;
      return (
        provider.name.toLowerCase().includes(query) || provider.id.toLowerCase().includes(query)
      );
    });

    result = [...result].sort((a, b) => {
      const left = a[sortKey];
      const right = b[sortKey];
      const order = sortDir === 'asc' ? 1 : -1;
      if (typeof left === 'number' && typeof right === 'number') return (left - right) * order;
      if (typeof left === 'boolean' && typeof right === 'boolean')
        return (Number(left) - Number(right)) * order;
      return String(left ?? '').localeCompare(String(right ?? '')) * order;
    });

    return result;
  });

  onMount(loadData);

  function rebuildProviderModelNames(models: Model[]) {
    const map: Record<string, Set<string>> = {};
    for (const model of models) {
      const gatewayIds = new Set<string>();
      if (model.provider_id) gatewayIds.add(model.provider_id);
      const topId = model.top_provider?.id;
      if (typeof topId === 'string') gatewayIds.add(topId);
      const labId = modelLabId(model.name);
      if (labId) gatewayIds.add(labId);

      for (const gatewayId of gatewayIds) {
        if (!map[gatewayId]) map[gatewayId] = new Set();
        map[gatewayId].add(model.name);
      }
    }

    providerModelNames = Object.fromEntries(
      Object.entries(map).map(([id, names]) => [id, [...names].sort()])
    );
  }

  function modelsForProvider(provider: ProviderRow): string[] {
    if (provider.catalog_models && provider.catalog_models.length > 0) {
      return provider.catalog_models;
    }
    return providerModelNames[provider.id] || [];
  }

  async function loadData() {
    loading = true;
    try {
      const [rawProviders, rawModels] = await Promise.all([
        api.providers.list(),
        api.models.list(),
      ]);
      providers = rawProviders;
      rebuildProviderModelNames(rawModels);
      enabledDrafts = Object.fromEntries(rawProviders.map((p) => [p.id, p.enabled]));
      keyListDrafts = Object.fromEntries(
        rawProviders.map((p) => [
          p.id,
          p.api_keys ? p.api_keys.map((k) => ({ ...k, subscribed: k.subscribed ?? false })) : [],
        ])
      );
      endpointDrafts = Object.fromEntries(
        rawProviders.map((p) => [p.id, p.endpoints ? p.endpoints.map((e) => ({ ...e })) : []])
      );
      if (expandedProviderId && !rawProviders.some((p) => p.id === expandedProviderId)) {
        expandedProviderId = null;
      }
    } catch (e) {
      providers = [];
      providerModelNames = {};
      toast.error(e instanceof Error ? e.message : i18n.t('providers.load_failed'));
    } finally {
      loading = false;
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

  function toggleExpanded(providerId: string) {
    expandedProviderId = expandedProviderId === providerId ? null : providerId;
  }

  function endpointProtocols(providerId: string): string[] {
    const eps = endpointDrafts[providerId] || [];
    return [...new Set(eps.map((ep) => ep.protocol))].sort();
  }

  function keyCount(providerId: string): number {
    return (keyListDrafts[providerId] || []).filter((k) => k.key.trim().length > 0).length;
  }

  function isKeyRateLimited(keyConfig: ApiKeyConfig): boolean {
    if (!keyConfig.quota_reset_at) return false;
    const resetAt = Date.parse(keyConfig.quota_reset_at);
    return Number.isFinite(resetAt) && resetAt > Date.now();
  }

  function formatQuotaReset(keyConfig: ApiKeyConfig): string | null {
    if (!isKeyRateLimited(keyConfig) || !keyConfig.quota_reset_at) return null;
    const resetAt = new Date(keyConfig.quota_reset_at);
    return resetAt.toLocaleString();
  }

  function formatEnv(env: string[] | null | undefined) {
    return env && env.length > 0 ? env : [];
  }

  function addKey(providerId: string) {
    if (!keyListDrafts[providerId]) {
      keyListDrafts[providerId] = [];
    }
    keyListDrafts[providerId].push({ key: '', enabled: true, subscribed: false });
  }

  async function removeKey(provider: ProviderRow, index: number) {
    if (keyListDrafts[provider.id]) {
      keyListDrafts[provider.id].splice(index, 1);
      await autoSaveKeys(provider);
    }
  }

  async function toggleStatus(provider: ProviderRow) {
    const currentEnabled = enabledDrafts[provider.id];
    const newEnabled = !currentEnabled;

    const draftKeys = keyListDrafts[provider.id] || [];
    const hasEnabledKey = draftKeys.some((k) => k.enabled && k.key.trim().length > 0);

    if (newEnabled && !hasEnabledKey) {
      toast.error(i18n.t('providers.enable_requires_key'));
      return;
    }

    enabledDrafts[provider.id] = newEnabled;
    try {
      await api.providers.update(provider.id, { enabled: newEnabled });
      toast.success(i18n.t('providers.status_updated').replace('{name}', provider.name));
      dataRevision.touchProviders();
      await loadData();
    } catch (e) {
      enabledDrafts[provider.id] = currentEnabled;
      toast.error(e instanceof Error ? e.message : i18n.t('providers.status_update_failed'));
    }
  }

  async function autoSaveKeys(provider: ProviderRow) {
    const draftKeys = keyListDrafts[provider.id] || [];
    const enabled = enabledDrafts[provider.id] ?? false;
    const keysToSave = draftKeys.filter((k) => k.key.trim().length > 0);
    const hasEnabledKey = keysToSave.some((k) => k.enabled);

    if (enabled && !hasEnabledKey) {
      toast.error(i18n.t('providers.enabled_requires_key'));
      enabledDrafts[provider.id] = false;
      try {
        await api.providers.update(provider.id, { api_keys: keysToSave, enabled: false });
        dataRevision.touchProviders();
        await loadData();
      } catch (e) {
        toast.error(e instanceof Error ? e.message : i18n.t('providers.status_update_failed'));
      }
      return;
    }

    try {
      await api.providers.update(provider.id, { api_keys: keysToSave });
      toast.success(i18n.t('providers.keys_saved').replace('{name}', provider.name));
      dataRevision.touchProviders();
      await loadData();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('providers.keys_save_failed'));
    }
  }

  function addEndpoint(providerId: string) {
    if (!endpointDrafts[providerId]) {
      endpointDrafts[providerId] = [];
    }
    endpointDrafts[providerId].push({
      id: crypto.randomUUID(),
      protocol: 'openai-chat',
      url: '',
      label: null,
      priority: 50,
      enabled: true,
    });
  }

  async function removeEndpoint(provider: ProviderRow, index: number) {
    if (endpointDrafts[provider.id]) {
      endpointDrafts[provider.id].splice(index, 1);
      await autoSaveEndpoints(provider);
    }
  }

  async function autoSaveEndpoints(provider: ProviderRow) {
    const endpoints = endpointDrafts[provider.id] || [];

    for (const ep of endpoints) {
      if (ep.url.trim() && !ep.url.startsWith('http://') && !ep.url.startsWith('https://')) {
        toast.error(i18n.t('providers.endpoint_invalid_url'));
        return;
      }
    }

    try {
      await api.providers.update(provider.id, { endpoints });
      toast.success(i18n.t('providers.endpoints_saved').replace('{name}', provider.name));
      dataRevision.touchProviders();
      await loadData();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('providers.endpoints_save_failed'));
    }
  }
</script>

<PageHeader title={i18n.t('providers.title')} description={i18n.t('providers.subtitle')} />

{#if loading}
  <div class="skeleton" style="height: 260px; border-radius: var(--radius-lg);"></div>
{:else}
  <section class="provider-toolbar">
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
          placeholder={i18n.t('providers.search_placeholder')}
          bind:value={searchQuery}
        />
      </div>
    </div>
    <div class="toolbar-col">
      <div class="filter-segment" role="group" aria-label={i18n.t('providers.filter_key_group')}>
        <button
          type="button"
          class="segment-btn"
          class:active={keyFilter === 'all'}
          onclick={() => (keyFilter = 'all')}
        >
          {i18n.t('providers.filter_all_keys')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={keyFilter === 'configured'}
          onclick={() => (keyFilter = 'configured')}
        >
          {i18n.t('providers.filter_has_key')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={keyFilter === 'missing'}
          onclick={() => (keyFilter = 'missing')}
        >
          {i18n.t('providers.filter_no_key')}
        </button>
      </div>
    </div>
    <div class="toolbar-col">
      <div class="filter-segment" role="group" aria-label={i18n.t('providers.filter_status_group')}>
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'all'}
          onclick={() => (statusFilter = 'all')}
        >
          {i18n.t('providers.filter_all_status')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'enabled'}
          onclick={() => (statusFilter = 'enabled')}
        >
          {i18n.t('providers.filter_enabled')}
        </button>
        <button
          type="button"
          class="segment-btn"
          class:active={statusFilter === 'disabled'}
          onclick={() => (statusFilter = 'disabled')}
        >
          {i18n.t('providers.filter_disabled')}
        </button>
      </div>
    </div>
  </section>

  <section class="provider-table-wrap">
    <div class="provider-table">
      <div class="provider-grid header-row">
        <div class="cell cell-logo">{i18n.t('providers.col_logo')}</div>
        <div
          class="cell cell-name sortable"
          class:active-sort={sortKey === 'name'}
          onclick={() => setSort('name')}
        >
          {i18n.t('providers.col_name')}
        </div>
        <div
          class="cell cell-id sortable"
          class:active-sort={sortKey === 'id'}
          onclick={() => setSort('id')}
        >
          {i18n.t('providers.col_id')}
        </div>
        <div
          class="cell cell-models sortable"
          class:active-sort={sortKey === 'model_count'}
          onclick={() => setSort('model_count')}
        >
          {i18n.t('providers.col_models')}
        </div>
        <div class="cell cell-endpoints">{i18n.t('providers.col_endpoints')}</div>
        <div class="cell cell-keys">{i18n.t('providers.col_keys')}</div>
        <div
          class="cell cell-status sortable"
          class:active-sort={sortKey === 'enabled'}
          onclick={() => setSort('enabled')}
        >
          {i18n.t('providers.col_status')}
        </div>
        <div class="cell cell-chevron" aria-hidden="true"></div>
      </div>

      {#each rows as provider (provider.id)}
        <div class="provider-block" class:expanded={expandedProviderId === provider.id}>
          <button
            type="button"
            class="provider-grid data-row"
            onclick={() => toggleExpanded(provider.id)}
            aria-expanded={expandedProviderId === provider.id}
          >
            <div class="cell cell-logo">
              <CatalogLogo id={provider.id} kind="provider" size={24} alt={provider.name} />
            </div>
            <div class="cell cell-name">
              <strong class="provider-name">{provider.name}</strong>
            </div>
            <div class="cell cell-id">
              <span class="mono muted">{provider.id}</span>
            </div>
            <div class="cell cell-models">
              <span class="count-badge">{provider.model_count ?? 0}</span>
            </div>
            <div class="cell cell-endpoints">
              {#if endpointProtocols(provider.id).length > 0}
                <div class="protocol-badges">
                  {#each endpointProtocols(provider.id) as protocol}
                    <span class="badge badge-secondary mono">{protocol}</span>
                  {/each}
                </div>
              {:else}
                <span class="muted">—</span>
              {/if}
            </div>
            <div class="cell cell-keys">
              <span class="count-badge">{keyCount(provider.id)}</span>
            </div>
            <div class="cell cell-status">
              <span class="status-badge" class:enabled={enabledDrafts[provider.id]}>
                {enabledDrafts[provider.id] ? i18n.t('common.enabled') : i18n.t('common.disabled')}
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
                style="transform: rotate({expandedProviderId === provider.id
                  ? 180
                  : 0}deg); transition: transform 0.2s;"
              >
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </div>
          </button>

          {#if expandedProviderId === provider.id}
            <div class="detail-panel">
              <div class="detail-meta">
                <div class="detail-field">
                  <span class="detail-label">{i18n.t('providers.detail_api')}</span>
                  {#if provider.api}
                    <span class="detail-value mono">{provider.api}</span>
                  {:else}
                    <span class="detail-value muted">—</span>
                  {/if}
                </div>
                <div class="detail-field">
                  <span class="detail-label">{i18n.t('providers.detail_doc')}</span>
                  {#if provider.doc}
                    <a
                      href={provider.doc}
                      target="_blank"
                      rel="noopener noreferrer"
                      class="detail-link">{provider.doc}</a
                    >
                  {:else}
                    <span class="detail-value muted">—</span>
                  {/if}
                </div>
                <div class="detail-field">
                  <span class="detail-label">{i18n.t('providers.detail_env')}</span>
                  <div class="env-list">
                    {#if formatEnv(provider.env).length > 0}
                      {#each formatEnv(provider.env) as env}
                        <span class="badge badge-secondary mono">{env}</span>
                      {/each}
                    {:else}
                      <span class="muted">—</span>
                    {/if}
                  </div>
                </div>
              </div>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('providers.detail_models')}</h4>
                  <span class="muted text-xs"
                    >{i18n
                      .t('providers.models_total')
                      .replace('{count}', String(provider.model_count ?? 0))}</span
                  >
                </div>
                {#if modelsForProvider(provider).length > 0}
                  <div class="model-name-list">
                    {#each modelsForProvider(provider) as modelName}
                      <span class="model-name-chip mono">{modelName}</span>
                    {/each}
                  </div>
                {:else}
                  <div class="detail-empty">{i18n.t('providers.no_models')}</div>
                {/if}
              </section>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('providers.detail_endpoints')}</h4>
                  <button
                    type="button"
                    class="btn btn-xs btn-accent"
                    onclick={() => addEndpoint(provider.id)}
                  >
                    + {i18n.t('providers.add_endpoint')}
                  </button>
                </div>
                {#if (endpointDrafts[provider.id] || []).length === 0}
                  <div class="detail-empty">{i18n.t('providers.endpoint_no_endpoints')}</div>
                {:else}
                  <div class="endpoint-list">
                    {#each endpointDrafts[provider.id] || [] as ep, index}
                      <div class="endpoint-row">
                        <select
                          class="select ep-protocol-select"
                          bind:value={ep.protocol}
                          onchange={() => autoSaveEndpoints(provider)}
                        >
                          <option value="openai-chat">openai-chat</option>
                          <option value="anthropic">anthropic</option>
                          <option value="openai-responses">openai-responses</option>
                        </select>
                        <input
                          class="input ep-url-input"
                          type="text"
                          placeholder="https://api.example.com/v1"
                          bind:value={ep.url}
                          onblur={() => autoSaveEndpoints(provider)}
                          onkeydown={(e) => {
                            if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
                          }}
                        />
                        <input
                          class="input ep-label-input"
                          type="text"
                          placeholder={i18n.t('providers.endpoint_label')}
                          bind:value={ep.label}
                          onblur={() => autoSaveEndpoints(provider)}
                        />
                        <input
                          class="input ep-priority-input"
                          type="number"
                          placeholder={i18n.t('providers.endpoint_priority')}
                          bind:value={ep.priority}
                          onblur={() => autoSaveEndpoints(provider)}
                        />
                        <label
                          class="toggle ep-enabled-toggle"
                          title={ep.enabled ? i18n.t('common.enabled') : i18n.t('common.disabled')}
                        >
                          <input
                            type="checkbox"
                            bind:checked={ep.enabled}
                            onchange={() => autoSaveEndpoints(provider)}
                          />
                          <span class="toggle-slider"></span>
                        </label>
                        <button
                          type="button"
                          class="btn-icon-delete"
                          onclick={() => removeEndpoint(provider, index)}
                          title={i18n.t('common.delete')}
                        >
                          <svg
                            width="12"
                            height="12"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                          >
                            <path
                              d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"
                            />
                          </svg>
                        </button>
                      </div>
                    {/each}
                  </div>
                {/if}
              </section>

              <section class="detail-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('providers.detail_keys')}</h4>
                  <button
                    type="button"
                    class="btn btn-xs btn-neutral"
                    onclick={() => addKey(provider.id)}
                  >
                    + {i18n.t('providers.add_key')}
                  </button>
                </div>
                <div class="keys-container">
                  {#each keyListDrafts[provider.id] || [] as keyConfig, index}
                    <div class="key-row">
                      <input
                        class="input mono key-input-field"
                        type="password"
                        placeholder={i18n.t('providers.placeholder_key')}
                        bind:value={keyConfig.key}
                        onblur={() => autoSaveKeys(provider)}
                        onkeydown={(e) => {
                          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
                        }}
                      />
                      <label
                        class="toggle key-toggle"
                        title={keyConfig.enabled
                          ? i18n.t('common.enabled')
                          : i18n.t('common.disabled')}
                      >
                        <input
                          type="checkbox"
                          bind:checked={keyConfig.enabled}
                          onchange={() => autoSaveKeys(provider)}
                        />
                        <span class="toggle-slider"></span>
                      </label>
                      <label
                        class="subscribed-toggle"
                        class:active={keyConfig.subscribed}
                        title={i18n.t('providers.key_subscribed_tip')}
                      >
                        <input
                          type="checkbox"
                          bind:checked={keyConfig.subscribed}
                          onchange={() => autoSaveKeys(provider)}
                        />
                        <span class="subscribed-label">{i18n.t('providers.key_subscribed')}</span>
                      </label>
                      {#if isKeyRateLimited(keyConfig)}
                        <span class="quota-badge" title={formatQuotaReset(keyConfig) || ''}>
                          {i18n.t('providers.key_quota_limited')}
                        </span>
                      {/if}
                      <button
                        type="button"
                        class="btn-icon-delete"
                        onclick={() => removeKey(provider, index)}
                        title={i18n.t('providers.delete_key')}
                      >
                        <svg
                          width="12"
                          height="12"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                        >
                          <path
                            d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"
                          />
                        </svg>
                      </button>
                    </div>
                  {/each}
                </div>
              </section>

              <section class="detail-section detail-status-section">
                <div class="detail-section-head">
                  <h4>{i18n.t('providers.detail_status')}</h4>
                </div>
                <div class="status-row">
                  <label class="toggle">
                    <input
                      type="checkbox"
                      checked={enabledDrafts[provider.id]}
                      onclick={(e) => {
                        e.preventDefault();
                        toggleStatus(provider);
                      }}
                    />
                    <span class="toggle-slider"></span>
                  </label>
                  <span class="status-badge" class:enabled={enabledDrafts[provider.id]}>
                    {enabledDrafts[provider.id]
                      ? i18n.t('common.enabled')
                      : i18n.t('common.disabled')}
                  </span>
                </div>
              </section>
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if rows.length === 0}
      <div class="empty-row">{i18n.t('providers.empty')}</div>
    {/if}
  </section>
{/if}

<style>
  .provider-toolbar {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 12px;
    align-items: stretch;
  }

  .toolbar-col {
    min-width: 0;
    display: flex;
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
    height: 100%;
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
    transition: all var(--transition-fast, 0.15s ease);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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

  .provider-table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .provider-table {
    width: 100%;
  }

  .provider-grid {
    display: grid;
    grid-template-columns: 52px minmax(160px, 1.4fr) 120px 72px minmax(140px, 1fr) 64px 88px 28px;
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

  .provider-block {
    border-bottom: 1px solid var(--border);
  }

  .provider-block:last-child {
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
  }

  .data-row:hover,
  .provider-block.expanded .data-row {
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

  .cell-name {
    overflow: hidden;
  }

  .provider-name {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-size: 14px;
  }

  .cell-models,
  .cell-keys,
  .cell-status,
  .cell-chevron {
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .cell-endpoints {
    display: flex;
    align-items: center;
  }

  .protocol-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .count-badge,
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

  .badge-secondary {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    color: var(--text-primary);
    font-size: 10px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }

  .detail-panel {
    padding: 0 12px 16px 12px;
    background: var(--bg-tertiary, var(--bg-primary));
    border-top: 1px solid var(--border);
  }

  .detail-meta {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    padding: 14px 0;
    border-bottom: 1px solid var(--border);
  }

  .detail-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .detail-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .detail-value,
  .detail-link {
    font-size: 12px;
    word-break: break-all;
  }

  .detail-link {
    color: var(--accent);
    text-decoration: none;
  }

  .detail-link:hover {
    text-decoration: underline;
  }

  .env-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .detail-section {
    padding-top: 14px;
  }

  .detail-section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .detail-section-head h4 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .detail-empty {
    padding: 12px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
  }

  .model-name-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    max-height: 160px;
    overflow: auto;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-secondary);
  }

  .model-name-chip {
    font-size: 11px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    border: 1px solid var(--border);
    color: var(--text-secondary);
  }

  .endpoint-list,
  .keys-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .endpoint-row,
  .key-row {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 6px 8px;
  }

  .ep-protocol-select {
    width: 130px;
    min-width: 130px;
    font-size: 12px;
    padding: 4px 6px;
  }

  .ep-url-input {
    flex: 1;
    min-width: 160px;
    font-size: 12px;
    padding: 4px 6px;
  }

  .ep-label-input {
    width: 100px;
    min-width: 80px;
    font-size: 12px;
    padding: 4px 6px;
  }

  .ep-priority-input {
    width: 70px;
    min-width: 70px;
    font-size: 12px;
    padding: 4px 6px;
    text-align: center;
  }

  .ep-enabled-toggle,
  .key-toggle {
    transform: scale(0.85);
  }

  .subscribed-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-primary);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    transition: all 0.15s ease;
  }

  .subscribed-toggle:hover {
    border-color: rgba(234, 179, 8, 0.4);
    color: var(--text-secondary);
  }

  .subscribed-toggle.active {
    border-color: rgba(234, 179, 8, 0.5);
    background: rgba(234, 179, 8, 0.12);
    color: #fbbf24;
  }

  .subscribed-toggle input {
    accent-color: #fbbf24;
  }

  .quota-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid rgba(239, 68, 68, 0.35);
    background: rgba(239, 68, 68, 0.1);
    color: #f87171;
    font-size: 10px;
    font-weight: 600;
    white-space: nowrap;
  }

  .key-input-field {
    border: none;
    background: transparent;
    padding: 4px 2px;
    font-size: 12px;
    flex: 1;
    min-width: 120px;
    outline: none;
    box-shadow: none;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .btn-icon-delete {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    transition: all 0.2s;
  }

  .btn-icon-delete:hover {
    color: var(--error, #ef4444);
    background: rgba(239, 68, 68, 0.1);
  }

  .muted {
    color: var(--text-muted);
  }

  .empty-row {
    padding: 32px;
    color: var(--text-muted);
    text-align: center;
  }

  @media (max-width: 1100px) {
    .provider-table-wrap {
      overflow-x: auto;
    }

    .provider-grid {
      min-width: 920px;
    }

    .detail-meta {
      grid-template-columns: 1fr;
    }
  }
</style>
