<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Settings, UpdateSettings, CatalogSourceStatus } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import pkg from '../../../package.json';

  const appVersion = pkg.version;

  let settings = $state<Settings | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let catalogSources = $state<CatalogSourceStatus[]>([]);
  let catalogSyncing = $state(false);

  let formPort = $state(3125);
  let formRetention = $state(30);
  let formKey = $state('');
  let formArtificialAnalysisKey = $state('');
  let showKey = $state(false);
  let showArtificialAnalysisKey = $state(false);

  function formatSyncedAt(value: string | null | undefined) {
    if (!value) return i18n.t('settings.catalog_not_cached');
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString();
  }

  function catalogRecordSummary(source: CatalogSourceStatus) {
    const parts: string[] = [];
    if (source.providers != null) {
      parts.push(`${source.providers} gateways`);
    }
    if (source.models != null) {
      parts.push(`${source.models} models`);
    }
    return parts.length > 0 ? parts.join(' · ') : i18n.t('settings.catalog_not_cached');
  }

  async function loadCatalogStatus() {
    try {
      const res = await api.settings.getCatalogStatus();
      catalogSources = res.sources;
    } catch {
      catalogSources = [];
    }
  }

  onMount(async () => {
    loading = true;
    try {
      settings = await api.settings.get();
      formPort = settings.gateway_port;
      formRetention = settings.log_retention_days;
      formKey = settings.gateway_key || '';
      formArtificialAnalysisKey = settings.artificial_analysis_api_key || '';
    } catch {
      settings = {
        gateway_port: 3125,
        log_retention_days: 30,
        gateway_status: 'running',
        gateway_key: '',
        artificial_analysis_api_key: '',
      };
      formPort = 3125;
      formRetention = 30;
      formKey = '';
      formArtificialAnalysisKey = '';
    } finally {
      loading = false;
    }

    await loadCatalogStatus();
  });

  async function handleSave(showSyncToast = false) {
    saving = true;
    try {
      const data: UpdateSettings = {
        gateway_port: formPort,
        log_retention_days: formRetention,
        gateway_key: formKey,
        artificial_analysis_api_key: formArtificialAnalysisKey.trim() || null,
        providers: settings?.providers,
        models: settings?.models,
      };
      settings = await api.settings.update(data);
      if (showSyncToast) {
        toast.success(i18n.t('settings.sync_success'));
      } else {
        toast.success(i18n.t('common.success'));
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      saving = false;
    }
  }

  async function handleCopy() {
    if (!formKey) {
      toast.warning(i18n.t('settings.key_empty'));
      return;
    }
    try {
      await navigator.clipboard.writeText(formKey);
      toast.success(i18n.t('settings.key_copied'));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    }
  }

  function handleRefreshKey() {
    formKey = 'cab-token-' + crypto.randomUUID();
    toast(i18n.t('settings.key_refreshed'));
  }

  function handleSync() {
    handleSave(true);
  }

  async function handleCatalogSync() {
    catalogSyncing = true;
    try {
      const aaKeyChanged =
        (settings?.artificial_analysis_api_key || '') !== formArtificialAnalysisKey.trim();
      if (aaKeyChanged) {
        await handleSave(false);
      }

      const res = await api.settings.syncCatalog();
      catalogSources = res.sources;
      const benchmarks =
        res.sources.find((source) => source.id === 'artificial-analysis')?.models ?? 0;
      toast.success(
        i18n
          .t('settings.catalog_update_success')
          .replace('{providers}', String(res.providers))
          .replace('{models}', String(res.applied_models))
          .replace('{benchmarks}', String(benchmarks))
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      catalogSyncing = false;
    }
  }

  const statusColors: Record<string, string> = {
    running: 'var(--success)',
    stopped: 'var(--text-muted)',
    error: 'var(--error)',
  };

  const statusDotClass: Record<string, string> = {
    running: 'dot-active',
    stopped: 'dot-inactive',
    error: 'dot-error',
  };
</script>

<PageHeader title={i18n.t('settings.title')} description={i18n.t('settings.subtitle')} />

{#if loading}
  <div class="settings-grid">
    <div class="skeleton" style="height: 200px; border-radius: var(--radius-lg);"></div>
    <div class="skeleton" style="height: 200px; border-radius: var(--radius-lg);"></div>
  </div>
{:else}
  <div class="settings-grid">
    <!-- Gateway Status -->
    <Card>
      <h3 class="card-section-title">{i18n.t('settings.gateway_status')}</h3>
      <div class="status-display">
        <div class="status-row">
          <span class="status-label">{i18n.t('common.status')}</span>
          <span class="status-value">
            <span class="dot {statusDotClass[settings?.gateway_status ?? 'running']}"></span>
            <span
              style:color={statusColors[settings?.gateway_status ?? 'running']}
              style="text-transform:capitalize"
            >
              {i18n.t(`settings.${settings?.gateway_status ?? 'running'}`)}
            </span>
          </span>
        </div>
        <div class="status-row">
          <span class="status-label">{i18n.t('settings.port')}</span>
          <span class="status-value mono"
            >http://localhost:{settings?.gateway_port ?? formPort}</span
          >
        </div>
        <div class="status-row">
          <span class="status-label">{i18n.t('settings.api_base')}</span>
          <span class="status-value mono"
            >http://localhost:{settings?.gateway_port ?? formPort}/api</span
          >
        </div>
      </div>
    </Card>

    <!-- Configuration -->
    <Card>
      <h3 class="card-section-title">{i18n.t('settings.title')}</h3>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          handleSave();
        }}
      >
        <div class="form-group">
          <label class="label" for="s-port">{i18n.t('settings.port')}</label>
          <input
            class="input mono"
            id="s-port"
            type="number"
            bind:value={formPort}
            min="1024"
            max="65535"
          />
        </div>
        <div class="form-group">
          <label class="label" for="s-retention">{i18n.t('settings.retention')}</label>
          <input
            class="input mono"
            id="s-retention"
            type="number"
            bind:value={formRetention}
            min="1"
            max="365"
          />
        </div>
        <div class="form-group">
          <label class="label" for="s-key">{i18n.t('settings.gateway_key')}</label>
          <div class="key-input-container">
            <input
              class="input mono"
              id="s-key"
              type={showKey ? 'text' : 'password'}
              bind:value={formKey}
              placeholder="cab-token-..."
            />
            <button
              type="button"
              class="btn btn-secondary btn-icon"
              onclick={() => (showKey = !showKey)}
              title={showKey ? i18n.t('settings.hide') : i18n.t('settings.show')}
            >
              {#if showKey}
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path d="M9.88 9.88a3 3 0 1 0 4.24 4.24" /><path
                    d="M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68"
                  /><path
                    d="M6.61 6.61A13.52 13.52 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61"
                  /><line x1="2" y1="2" x2="22" y2="22" /></svg
                >
              {:else}
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle
                    cx="12"
                    cy="12"
                    r="3"
                  /></svg
                >
              {/if}
            </button>
            <button
              type="button"
              class="btn btn-secondary"
              onclick={handleCopy}
              title={i18n.t('settings.copy_key')}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /><path
                  d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
                /></svg
              >
              {i18n.t('settings.copy_key')}
            </button>
            <button
              type="button"
              class="btn btn-secondary"
              onclick={handleRefreshKey}
              title={i18n.t('settings.refresh_key')}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" /><path
                  d="M16 3h5v5"
                /><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" /><path
                  d="M8 21H3v-5"
                /></svg
              >
              {i18n.t('settings.refresh_key')}
            </button>
            <button
              type="button"
              class="btn btn-secondary"
              onclick={handleSync}
              title={i18n.t('settings.sync_key')}
              disabled={saving}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                  d="M3 3v5h5"
                /><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path
                  d="M16 16h5v5"
                /></svg
              >
              {i18n.t('settings.sync_key')}
            </button>
          </div>
          <p class="help-text">{i18n.t('settings.gateway_key_tip')}</p>
        </div>
        <div class="form-group">
          <label class="label" for="s-aa-key">{i18n.t('settings.artificial_analysis_key')}</label>
          <div class="key-input-container">
            <input
              class="input mono"
              id="s-aa-key"
              type={showArtificialAnalysisKey ? 'text' : 'password'}
              bind:value={formArtificialAnalysisKey}
              placeholder="AA API Key"
            />
            <button
              type="button"
              class="btn btn-secondary btn-icon"
              onclick={() => (showArtificialAnalysisKey = !showArtificialAnalysisKey)}
              title={showArtificialAnalysisKey ? i18n.t('settings.hide') : i18n.t('settings.show')}
            >
              {#if showArtificialAnalysisKey}
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path d="M9.88 9.88a3 3 0 1 0 4.24 4.24" /><path
                    d="M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68"
                  /><path
                    d="M6.61 6.61A13.52 13.52 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61"
                  /><line x1="2" y1="2" x2="22" y2="22" /></svg
                >
              {:else}
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle
                    cx="12"
                    cy="12"
                    r="3"
                  /></svg
                >
              {/if}
            </button>
          </div>
          <p class="help-text">{i18n.t('settings.artificial_analysis_key_tip')}</p>
        </div>
        <div style="margin-top:20px">
          <button type="submit" class="btn btn-primary" disabled={saving}>
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M5 13l4 4L19 7" />
            </svg>
            {saving ? i18n.t('common.loading') : i18n.t('common.save')}
          </button>
        </div>
      </form>
    </Card>
  </div>

  <div class="catalog-section">
    <Card>
      <div class="catalog-header">
        <div>
          <h3 class="card-section-title">{i18n.t('settings.catalog_title')}</h3>
          <p class="help-text">{i18n.t('settings.catalog_subtitle')}</p>
        </div>
        <button
          type="button"
          class="btn btn-primary"
          onclick={handleCatalogSync}
          disabled={catalogSyncing || saving}
        >
          <svg
            class:spin={catalogSyncing}
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
            <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
            <path d="M16 16h5v5" />
          </svg>
          {catalogSyncing
            ? i18n.t('settings.catalog_updating')
            : i18n.t('settings.catalog_update_btn')}
        </button>
      </div>

      <div class="catalog-table">
        <div class="catalog-row catalog-head">
          <span>{i18n.t('settings.catalog_source')}</span>
          <span>{i18n.t('settings.catalog_records')}</span>
          <span>{i18n.t('settings.catalog_last_sync')}</span>
        </div>
        {#each catalogSources as source}
          <div class="catalog-row">
            <div class="catalog-source">
              <strong>{source.name}</strong>
              <a
                href={source.url}
                target="_blank"
                rel="noopener noreferrer"
                class="link-styled mono">{source.url}</a
              >
            </div>
            <span class="catalog-records">{catalogRecordSummary(source)}</span>
            <span class="catalog-synced">{formatSyncedAt(source.synced_at)}</span>
          </div>
        {:else}
          <div class="catalog-row">
            <span class="muted">{i18n.t('settings.catalog_not_cached')}</span>
          </div>
        {/each}
      </div>
    </Card>
  </div>

  <!-- Info -->
  <div class="info-section">
    <Card>
      <h3 class="card-section-title">{i18n.t('settings.about')}</h3>
      <div class="about-grid">
        <div class="about-row">
          <span class="about-label">{i18n.t('settings.version')}</span>
          <span class="about-value mono">{appVersion}</span>
        </div>
        <div class="about-row">
          <span class="about-label">{i18n.t('settings.runtime')}</span>
          <span class="about-value mono">Tauri 2 + SvelteKit</span>
        </div>
        <div class="about-row">
          <span class="about-label">{i18n.t('settings.backend')}</span>
          <span class="about-value mono">Rust (Axum)</span>
        </div>
        <div class="about-row">
          <span class="about-label">{i18n.t('settings.license')}</span>
          <span class="about-value">MIT</span>
        </div>
      </div>
    </Card>
  </div>
{/if}

<style>
  .settings-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin-bottom: 20px;
  }

  .card-section-title {
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 16px;
    background: linear-gradient(135deg, #fff 0%, var(--text-secondary) 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .status-display {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }

  .status-row:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .status-label {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .status-value {
    font-size: 13px;
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .form-group {
    margin-bottom: 16px;
  }

  .form-group:last-of-type {
    margin-bottom: 0;
  }

  .key-input-container {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .key-input-container .input {
    flex: 1;
  }

  .help-text {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 6px;
    line-height: 1.4;
  }

  .about-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .about-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }

  .about-row:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .about-label {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .about-value {
    font-size: 13px;
    font-weight: 500;
  }

  .catalog-section {
    margin-bottom: 20px;
  }

  .catalog-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 16px;
  }

  .catalog-table {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .catalog-row {
    display: grid;
    grid-template-columns: 1.4fr 1fr 1fr;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    align-items: center;
  }

  .catalog-row:last-child {
    border-bottom: none;
  }

  .catalog-head {
    background: var(--bg-tertiary);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .catalog-source {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .catalog-source a {
    font-size: 11px;
  }

  .catalog-records,
  .catalog-synced {
    font-size: 12px;
    color: var(--text-secondary);
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
