<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Settings, UpdateSettings, CatalogSourceStatus, CheckUpdateResponse } from '$lib/types';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Card from '$lib/components/Card.svelte';
  import { toast } from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { gatewayHealth } from '$lib/gateway-health.svelte';
  import { themeManager } from '$lib/theme.svelte';
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
  let formCacheAffinity = $state(true);
  let formCacheShaping = $state(true);
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

  let updateChecking = $state(false);
  let updateInstalling = $state(false);
  let updateInfo = $state<CheckUpdateResponse | null>(null);
  let updateChecked = $state(false);

  async function handleCheckUpdate() {
    updateChecking = true;
    try {
      updateInfo = await api.update.check();
      updateChecked = true;
      if (!updateInfo.available) {
        toast.success(i18n.t('settings.update_not_available'));
      } else {
        toast.success(i18n.t('settings.update_available').replace('{version}', updateInfo.latest_version));
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      updateChecking = false;
    }
  }

  async function handleInstallUpdate() {
    updateInstalling = true;
    try {
      const res = await api.update.install();
      if (res.success) {
        toast.success(i18n.t('settings.update_success'));
      } else {
        toast.error(res.message);
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : i18n.t('common.error'));
    } finally {
      updateInstalling = false;
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
      formCacheAffinity = settings.cache_affinity_enabled ?? true;
      formCacheShaping = settings.cache_request_shaping_enabled ?? true;
    } catch {
      settings = {
        gateway_port: 3125,
        log_retention_days: 30,
        gateway_key: '',
        cache_affinity_enabled: true,
        cache_request_shaping_enabled: true,
        artificial_analysis_api_key: '',
      };
      formPort = 3125;
      formRetention = 30;
      formKey = '';
      formArtificialAnalysisKey = '';
      formCacheAffinity = true;
      formCacheShaping = true;
    } finally {
      loading = false;
    }

    await loadCatalogStatus();

    try {
      updateInfo = await api.update.check();
      updateChecked = true;
    } catch {
      // Silent check fail
    }
  });

  async function handleSave(showSyncToast = false) {
    saving = true;
    try {
      const data: UpdateSettings = {
        gateway_port: formPort,
        log_retention_days: formRetention,
        gateway_key: formKey,
        cache_affinity_enabled: formCacheAffinity,
        cache_request_shaping_enabled: formCacheShaping,
        artificial_analysis_api_key: formArtificialAnalysisKey.trim() || null,
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
    checking: 'var(--text-muted)',
  };

  const statusDotClass: Record<string, string> = {
    running: 'dot-active',
    stopped: 'dot-inactive',
    error: 'dot-error',
    checking: 'dot-inactive',
  };
</script>

<PageHeader title={i18n.t('settings.title')} description={i18n.t('settings.subtitle')} />

{#if loading}
  <div class="settings-layout skeleton-loading">
    <div class="skeleton" style="height: 400px; border-radius: var(--radius-lg);"></div>
    <div class="skeleton" style="height: 500px; border-radius: var(--radius-lg);"></div>
  </div>
{:else}
  <div class="settings-layout">
    <!-- LEFT SIDEBAR: Gateway Profile & Catalog status (常驻侧栏卡片) -->
    <aside class="settings-sidebar">
      <!-- Profile Widget -->
      <div class="sidebar-widget profile-widget">
        <div class="widget-avatar-shell">
          <div class="widget-avatar">CAB</div>
        </div>
        <div class="widget-meta">
          <h4>{i18n.t('settings.about')}</h4>
          <span class="widget-version">{i18n.t('settings.version')} {appVersion}</span>
        </div>
        
        <div class="widget-status-list">
          <div class="widget-status-item">
            <span class="widget-status-label">{i18n.t('common.status')}</span>
            <span class="widget-status-value">
              <span class="pulse-dot {statusDotClass[gatewayHealth.status]}"></span>
              <span style:color={statusColors[gatewayHealth.status]} style="text-transform: capitalize; font-weight: 600;">
                {i18n.t(`settings.${gatewayHealth.status}`)}
              </span>
            </span>
          </div>
          <div class="widget-status-item">
            <span class="widget-status-label">{i18n.t('settings.api_base')}</span>
            <span class="widget-status-value mono text-xs select-all">http://localhost:{settings?.gateway_port ?? formPort}</span>
          </div>
          <div class="widget-status-item">
            <span class="widget-status-label">{i18n.t('settings.backend')}</span>
            <span class="widget-status-value">{i18n.t('settings.runtime_stack')}</span>
          </div>
        </div>
        
        <div class="widget-actions">
          <button
            type="button"
            class="btn btn-secondary btn-sm btn-full"
            onclick={handleCheckUpdate}
            disabled={updateChecking || updateInstalling}
          >
            {#if updateChecking}
              <svg class="spin" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /></svg>
              &nbsp;{i18n.t('settings.checking_update')}
            {:else}
              {i18n.t('settings.check_update')}
            {/if}
          </button>
        </div>
      </div>

      <!-- Sync Status Widget -->
      <div class="sidebar-widget sync-widget">
        <div class="widget-header">
          <h5>{i18n.t('settings.catalog_title')}</h5>
          <button
            type="button"
            class="btn-sync-icon"
            onclick={handleCatalogSync}
            disabled={catalogSyncing || saving}
            title={i18n.t('settings.catalog_update_btn')}
          >
            <svg
              class:spin={catalogSyncing}
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
            >
              <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
              <path d="M3 3v5h5" />
              <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
              <path d="M16 16h5v5" />
            </svg>
          </button>
        </div>
        
        <div class="catalog-mini-list">
          {#each catalogSources as source}
            <div class="catalog-mini-item">
              <div class="catalog-mini-meta">
                <span class="source-name">{source.name}</span>
                <span class="source-records">{catalogRecordSummary(source)}</span>
              </div>
              <span class="source-time">{formatSyncedAt(source.synced_at)}</span>
            </div>
          {:else}
            <div class="catalog-mini-empty">
              {i18n.t('settings.catalog_not_cached')}
            </div>
          {/each}
        </div>
      </div>
    </aside>

    <!-- RIGHT CONTENT: Settings Panels (OpenRouter 风格设置舱) -->
    <main class="settings-main">
      <!-- 1. Authentication & Service (认证与网关端口服务) -->
      <section class="settings-panel">
        <div class="panel-header">
          <h3>{i18n.t('settings.auth_panel_title')}</h3>
          <p>{i18n.t('settings.auth_panel_desc')}</p>
        </div>
        
        <form onsubmit={(e) => { e.preventDefault(); handleSave(); }}>
          <div class="panel-body">
            <!-- Port option row -->
            <div class="option-row">
              <div class="option-info">
                <label for="s-port" class="option-title">{i18n.t('settings.port')}</label>
                <span class="option-desc">{i18n.t('settings.port_desc')}</span>
              </div>
              <div class="option-control">
                <input
                  class="input mono port-input"
                  id="s-port"
                  type="number"
                  bind:value={formPort}
                  min="1024"
                  max="65535"
                />
              </div>
            </div>

            <!-- Log retention option row -->
            <div class="option-row">
              <div class="option-info">
                <label for="s-retention" class="option-title">{i18n.t('settings.retention')}</label>
                <span class="option-desc">{i18n.t('settings.retention_desc')}</span>
              </div>
              <div class="option-control">
                <input
                  class="input mono retention-input"
                  id="s-retention"
                  type="number"
                  bind:value={formRetention}
                  min="1"
                  max="365"
                />
              </div>
            </div>

            <!-- Gateway key option row -->
            <div class="option-row option-row--vertical">
              <div class="option-info">
                <label for="s-key" class="option-title">{i18n.t('settings.gateway_key')}</label>
                <span class="option-desc">
                  {i18n.t('settings.gateway_key_tip')}
                </span>
              </div>
              <div class="option-control-group">
                <div class="key-field-wrapper">
                  <input
                    class="input mono key-field"
                    id="s-key"
                    type={showKey ? 'text' : 'password'}
                    bind:value={formKey}
                    placeholder={i18n.t('settings.gateway_key_placeholder')}
                  />
                  <!-- Minimalist Icon Buttons -->
                  <div class="key-action-buttons">
                    <button
                      type="button"
                      class="icon-action-btn"
                      onclick={() => (showKey = !showKey)}
                      title={showKey ? i18n.t('settings.hide') : i18n.t('settings.show')}
                    >
                      {#if showKey}
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9.88 9.88a3 3 0 1 0 4.24 4.24" /><path d="M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68" /><path d="M6.61 6.61A13.52 13.52 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61" /><line x1="2" y1="2" x2="22" y2="22" /></svg>
                      {:else}
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle cx="12" cy="12" r="3" /></svg>
                      {/if}
                    </button>
                    <button
                      type="button"
                      class="icon-action-btn"
                      onclick={handleCopy}
                      title={i18n.t('settings.copy_key')}
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" /></svg>
                    </button>
                    <button
                      type="button"
                      class="icon-action-btn"
                      onclick={handleRefreshKey}
                      title={i18n.t('settings.refresh_key')}
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" /><path d="M16 3h5v5" /><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" /><path d="M8 21H3v-5" /></svg>
                    </button>
                    <button
                      type="button"
                      class="icon-action-btn"
                      onclick={handleSync}
                      title={i18n.t('settings.sync_key')}
                      disabled={saving}
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path d="M16 16h5v5" /></svg>
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <!-- Artificial Analysis API Key -->
            <div class="option-row option-row--vertical">
              <div class="option-info">
                <label for="s-aa-key" class="option-title">{i18n.t('settings.artificial_analysis_key')}</label>
                <span class="option-desc">
                  {i18n.t('settings.artificial_analysis_key_tip')}
                </span>
              </div>
              <div class="option-control-group">
                <div class="key-field-wrapper">
                  <input
                    class="input mono key-field"
                    id="s-aa-key"
                    type={showArtificialAnalysisKey ? 'text' : 'password'}
                    bind:value={formArtificialAnalysisKey}
                    placeholder={i18n.t('settings.aa_key_placeholder')}
                  />
                  <div class="key-action-buttons">
                    <button
                      type="button"
                      class="icon-action-btn"
                      onclick={() => (showArtificialAnalysisKey = !showArtificialAnalysisKey)}
                      title={showArtificialAnalysisKey ? i18n.t('settings.hide') : i18n.t('settings.show')}
                    >
                      {#if showArtificialAnalysisKey}
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9.88 9.88a3 3 0 1 0 4.24 4.24" /><path d="M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68" /><path d="M6.61 6.61A13.52 13.52 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61" /><line x1="2" y1="2" x2="22" y2="22" /></svg>
                      {:else}
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle cx="12" cy="12" r="3" /></svg>
                      {/if}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
          
          <!-- OpenRouter-style panel footer with high-contrast Save button -->
          <div class="panel-footer">
            <span class="footer-tip">💡 {i18n.t('settings.footer_port_restart_tip')}</span>
            <button type="submit" class="btn btn-primary" disabled={saving}>
              {saving ? i18n.t('common.loading') : i18n.t('common.save')}
            </button>
          </div>
        </form>
      </section>

      <!-- 2. Performance & Cache Shaping (性能与缓存优化) -->
      <section class="settings-panel">
        <div class="panel-header">
          <h3>{i18n.t('settings.perf_panel_title')}</h3>
          <p>{i18n.t('settings.perf_panel_desc')}</p>
        </div>
        
        <form onsubmit={(e) => { e.preventDefault(); handleSave(); }}>
          <div class="panel-body">
            <!-- Cache Affinity Row -->
            <div class="option-row">
              <div class="option-info">
                <span class="option-title">{i18n.t('settings.cache_affinity')}</span>
                <span class="option-desc">{i18n.t('settings.cache_affinity_tip')}</span>
              </div>
              <div class="option-control">
                <label class="toggle">
                  <input type="checkbox" bind:checked={formCacheAffinity} />
                  <span class="toggle-slider"></span>
                </label>
              </div>
            </div>

            <!-- Cache Shaping Row -->
            <div class="option-row">
              <div class="option-info">
                <span class="option-title">{i18n.t('settings.cache_shaping')}</span>
                <span class="option-desc">{i18n.t('settings.cache_shaping_tip')}</span>
              </div>
              <div class="option-control">
                <label class="toggle">
                  <input type="checkbox" bind:checked={formCacheShaping} />
                  <span class="toggle-slider"></span>
                </label>
              </div>
            </div>

            <!-- Appearance Theme Row -->
            <div class="option-row">
              <div class="option-info">
                <span class="option-title">{i18n.t('settings.theme_section_title')}</span>
                <span class="option-desc">{i18n.t('settings.theme_section_desc')}</span>
              </div>
              <div class="option-control">
                <div class="theme-segment">
                  <button
                    type="button"
                    class="theme-segment-btn"
                    class:active={themeManager.current === 'light'}
                    onclick={() => themeManager.set('light')}
                  >
                    {i18n.t('settings.theme_light')}
                  </button>
                  <button
                    type="button"
                    class="theme-segment-btn"
                    class:active={themeManager.current === 'dark'}
                    onclick={() => themeManager.set('dark')}
                  >
                    {i18n.t('settings.theme_dark')}
                  </button>
                  <button
                    type="button"
                    class="theme-segment-btn"
                    class:active={themeManager.current === 'system'}
                    onclick={() => themeManager.set('system')}
                  >
                    {i18n.t('settings.theme_system')}
                  </button>
                </div>
              </div>
            </div>
          </div>
          
          <div class="panel-footer">
            <span class="footer-tip">{i18n.t('settings.footer_affinity_tip')}</span>
            <button type="submit" class="btn btn-primary" disabled={saving}>
              {saving ? i18n.t('common.loading') : i18n.t('common.save')}
            </button>
          </div>
        </form>
      </section>

      <!-- 3. System Updates banner (发现新版本横幅) -->
      {#if updateInfo && updateInfo.available}
        <section class="settings-panel panel-update">
          <div class="panel-header">
            <h3 class="update-title">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="update-icon"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></svg>
              <span>{i18n.t('settings.update_available').replace('{version}', updateInfo.latest_version)}</span>
            </h3>
            {#if updateInfo.published_at}
              <span class="update-date">{i18n.tParams('settings.update_published_at', { date: new Date(updateInfo.published_at).toLocaleDateString() })}</span>
            {/if}
          </div>
          
          <div class="panel-body">
            {#if updateInfo.release_notes}
              <div class="update-notes">
                <span class="update-notes-title">{i18n.t('settings.release_notes')}</span>
                <pre class="update-notes-content">{updateInfo.release_notes}</pre>
              </div>
            {/if}
          </div>
          
          <div class="panel-footer panel-footer--update">
            <span class="footer-tip text-accent">{i18n.t('settings.update_ready_tip')}</span>
            <div class="update-actions">
              <button
                type="button"
                class="btn btn-primary btn-sm"
                onclick={handleInstallUpdate}
                disabled={updateInstalling}
              >
                {#if updateInstalling}
                  <svg class="spin" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /></svg>
                  &nbsp;{i18n.t('settings.updating')}
                {:else}
                  {i18n.t('settings.update_btn')}
                {/if}
              </button>
              {#if updateInfo.download_url}
                <a
                  href={updateInfo.download_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="btn btn-secondary btn-sm"
                >
                  {i18n.t('settings.update_manual_download')}
                </a>
              {/if}
            </div>
          </div>
        </section>
      {/if}
    </main>
  </div>
{/if}

<style>
  /* ── Layout ───────────────────────────────────────────── */
  .settings-layout {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 24px;
    align-items: start;
    margin-top: 4px;
  }

  /* ── Sidebar ──────────────────────────────────────────── */
  .settings-sidebar {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .sidebar-widget {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
    position: relative;
    overflow: hidden;
  }

  .profile-widget {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }

  .widget-avatar-shell {
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-full);
    margin-bottom: 12px;
  }

  .widget-avatar {
    width: 52px;
    height: 52px;
    background: linear-gradient(135deg, var(--accent), var(--accent-violet));
    border-radius: var(--radius-full);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 700;
    color: var(--on-accent);
    box-shadow: 0 0 15px rgba(59, 130, 246, 0.2);
  }

  .widget-meta h4 {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0 0 2px 0;
  }

  .widget-version {
    font-size: 10.5px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .widget-status-list {
    width: 100%;
    margin-top: 20px;
    border-top: 1px dashed var(--border);
    padding-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .widget-status-item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
  }

  .widget-status-label {
    font-size: 11px;
    color: var(--text-muted);
  }

  .widget-status-value {
    font-size: 12.5px;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .pulse-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    display: inline-block;
  }

  .pulse-dot.running { background-color: var(--success); }
  .pulse-dot.checking { background-color: var(--warning); }
  .pulse-dot.stopped, .pulse-dot.error { background-color: var(--error); }

  .widget-actions {
    width: 100%;
    margin-top: 20px;
  }

  .btn-full {
    width: 100%;
    justify-content: center;
  }

  /* Catalog status widget */
  .sync-widget .widget-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .sync-widget h5 {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .btn-sync-icon {
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

  .btn-sync-icon:hover {
    color: var(--text-primary);
    background: var(--surface-raised);
  }

  .catalog-mini-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .catalog-mini-item {
    background: var(--border-subtle);
    border-left: 2px solid rgba(59, 130, 246, 0.3);
    padding: 6px 0 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .catalog-mini-meta {
    display: flex;
    flex-direction: column;
  }

  .source-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .source-records {
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .source-time {
    font-size: 10.5px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .catalog-mini-empty {
    padding: 12px 0;
    text-align: center;
    color: var(--text-muted);
    font-size: 11px;
  }

  /* ── Right Panels (OpenRouter Style) ─────────────────── */
  .settings-main {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .settings-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: var(--shadow-sm);
  }

  .panel-header {
    padding: 20px 24px;
    border-bottom: 1px solid var(--border);
  }

  .panel-header h3 {
    font-size: 14.5px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px 0;
  }

  .panel-header p {
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.4;
    margin: 0;
  }

  .panel-body {
    padding: 8px 24px;
    display: flex;
    flex-direction: column;
  }

  /* Option Item Rows */
  .option-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 24px;
    padding: 20px 0;
    border-bottom: 1px dashed var(--border-dashed);
  }

  .option-row:last-child {
    border-bottom: none;
  }

  .option-row--vertical {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
  }

  .option-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
  }

  .option-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .option-desc {
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .option-control {
    flex-shrink: 0;
  }

  /* Specific Control Sizing */
  .port-input, .retention-input {
    width: 90px;
    text-align: center;
  }

  /* API Key Field Wrapper */
  .option-control-group {
    width: 100%;
  }

  .key-field-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
  }

  .key-field {
    padding-right: 140px; /* Space for overlay actions */
    width: 100%;
  }

  .key-action-buttons {
    position: absolute;
    right: 4px;
    display: flex;
    gap: 2px;
    align-items: center;
    background: linear-gradient(90deg, transparent, var(--bg-input-overlay) 20%);
    padding-left: 12px;
  }

  .icon-action-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-xs);
    transition: all var(--transition-fast);
  }

  .icon-action-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .icon-action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Panel bottom action bar */
  .panel-footer {
    padding: 16px 24px;
    background: var(--bg-tertiary);
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
  }

  .footer-tip {
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  /* ── Updates Panel Special ──────────────────────────── */
  .panel-update {
    border-color: rgba(59, 130, 246, 0.15);
    box-shadow: 0 4px 20px rgba(59,130,246,0.05);
  }

  .update-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--accent-text);
    margin: 0;
  }

  .update-icon {
    color: var(--accent-text);
    animation: bounce-micro 2s infinite;
  }

  @keyframes bounce-micro {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-2px); }
  }

  .update-date {
    font-size: 11px;
    color: var(--text-muted);
  }

  .update-notes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-card-expanded);
    padding: 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
  }

  .update-notes-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .update-notes-content {
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--text-secondary);
    max-height: 150px;
    overflow-y: auto;
    white-space: pre-wrap;
    font-family: var(--font-mono);
    margin: 0;
    scrollbar-width: thin;
  }

  .text-blue { color: var(--accent-text); }
  .update-actions {
    display: flex;
    gap: 8px;
  }

  /* ── Theme Segment ───────────────────────────────────── */
  .theme-segment {
    display: flex;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 3px;
    gap: 2px;
  }

  .theme-segment-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 11.5px;
    font-weight: 500;
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .theme-segment-btn:hover:not(.active) {
    color: var(--text-secondary);
    background: var(--bg-primary);
  }

  .theme-segment-btn.active {
    background: var(--bg-primary);
    color: var(--text-primary);
    box-shadow: var(--shadow-xs);
    font-weight: 600;
  }
  .panel-footer--update {
    background: var(--accent-muted);
    border-top: 1px solid var(--border);
  }

  .text-accent {
    color: var(--accent-text);
  }
  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 960px) {
    .settings-layout {
      grid-template-columns: 1fr;
      gap: 20px;
    }
  }
</style>
