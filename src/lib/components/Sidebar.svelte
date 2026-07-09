<script lang="ts">
  import { page } from '$app/stores';
  import { i18n } from '../i18n.svelte';
  import { gatewayHealth } from '../gateway-health.svelte';
  import { themeManager } from '../theme.svelte';
  import pkg from '../../../package.json';

  const statusDotClass: Record<string, string> = {
    running: 'dot-active',
    stopped: 'dot-inactive',
    error: 'dot-error',
    checking: 'dot-inactive',
  };

  // Svelte 5 derived state for reactive translation items
  const navItems = $derived([
    {
      path: '/',
      label: i18n.t('nav.dashboard'),
      icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1h-2z',
    },
    {
      path: '/providers',
      label: i18n.t('nav.providers'),
      icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01',
    },
    {
      path: '/models',
      label: i18n.t('nav.models'),
      icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z',
    },
    {
      path: '/routes',
      label: i18n.t('nav.routes'),
      icon: 'M13 10V3L4 14h7v7l9-11h-7z',
    },
    {
      path: '/agents',
      label: i18n.t('nav.agents'),
      icon: 'M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2 M9 11a4 4 0 100-8 4 4 0 000 8z M23 21v-2a4 4 0 00-3-3.87 M16 3.13a4 4 0 010 7.75',
    },
    {
      path: '/logs',
      label: i18n.t('nav.logs'),
      icon: 'M9 5H7a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01',
    },
    {
      path: '/usage',
      label: i18n.t('nav.usage'),
      icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z',
    },
    {
      path: '/settings',
      label: i18n.t('nav.settings'),
      icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z',
    },
  ]);

  function isActive(currentPath: string, itemPath: string): boolean {
    if (itemPath === '/') return currentPath === '/';
    return currentPath.startsWith(itemPath);
  }
</script>

<aside class="sidebar">
  <!-- Logo -->
  <div class="sidebar-header">
    <div class="logo">
      <div class="logo-icon">
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M12 2L2 7l10 5 10-5-10-5z" />
          <path d="M2 17l10 5 10-5" />
          <path d="M2 12l10 5 10-5" />
        </svg>
      </div>
      <div class="logo-text">
        <span class="logo-name">CAB</span>
        <span class="logo-subtitle">{i18n.t('common.app_subtitle')}</span>
      </div>
    </div>
  </div>

  <!-- Navigation -->
  <nav class="sidebar-nav">
    {#each navItems as item}
      <a href={item.path} class="nav-item" class:active={isActive($page.url.pathname, item.path)}>
        <svg
          class="nav-icon"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d={item.icon} />
        </svg>
        <span class="nav-label">{item.label}</span>
      </a>
    {/each}
  </nav>

  <!-- Footer with i18n toggle -->
  <div class="sidebar-footer-container">
    <div class="lang-switch">
      <button
        class="lang-btn"
        class:active={i18n.currentLang === 'zh'}
        onclick={() => i18n.setLang('zh')}
      >
        中文
      </button>
      <button
        class="lang-btn"
        class:active={i18n.currentLang === 'en'}
        onclick={() => i18n.setLang('en')}
      >
        English
      </button>
    </div>

    <div class="theme-switch">
      <button
        type="button"
        class="theme-btn"
        class:active={themeManager.current === 'light'}
        onclick={() => themeManager.set('light')}
        title="Light Theme"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
      </button>
      <button
        type="button"
        class="theme-btn"
        class:active={themeManager.current === 'dark'}
        onclick={() => themeManager.set('dark')}
        title="Dark Theme"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
      </button>
      <button
        type="button"
        class="theme-btn"
        class:active={themeManager.current === 'system'}
        onclick={() => themeManager.set('system')}
        title="System Preference"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
      </button>
    </div>

    <div class="sidebar-footer">
      <div class="gateway-status">
        <span class="dot {statusDotClass[gatewayHealth.status]}"></span>
        <span class="status-text">{i18n.t(`settings.${gatewayHealth.status}`)}</span>
      </div>
      <span class="version">v{pkg.version}</span>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    width: var(--sidebar-width);
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
    z-index: 50;
    user-select: none;
  }

  .sidebar-header {
    padding: 20px 16px 16px;
    border-bottom: 1px solid var(--border);
  }

  .logo {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .logo-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: linear-gradient(135deg, var(--accent), #8b5cf6);
    color: white;
    flex-shrink: 0;
  }

  .logo-text {
    display: flex;
    flex-direction: column;
  }

  .logo-name {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.04em;
    background: linear-gradient(135deg, #fff 0%, var(--text-secondary) 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .logo-subtitle {
    font-size: 10px;
    color: var(--text-muted);
    letter-spacing: 0.02em;
  }

  .sidebar-nav {
    flex: 1;
    padding: 12px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    text-decoration: none;
    font-size: 13px;
    font-weight: 450;
    transition: all var(--transition-fast);
    position: relative;
  }

  .nav-item:hover {
    background: var(--glass-bg-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background: var(--accent-muted);
    color: var(--accent-text);
  }

  .nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 16px;
    background: var(--accent);
    border-radius: 0 2px 2px 0;
  }

  .nav-icon {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .nav-item.active .nav-icon {
    opacity: 1;
  }

  .nav-label {
    white-space: nowrap;
  }

  .sidebar-footer-container {
    padding: 12px 16px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
  }

  .lang-switch,
  .theme-switch {
    display: flex;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px;
    margin-bottom: 8px;
  }

  .lang-btn,
  .theme-btn {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    padding: 4px 0;
    border-radius: 4px;
    cursor: pointer;
    transition: all var(--transition-fast);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .lang-btn.active,
  .theme-btn.active {
    background: var(--bg-primary);
    color: var(--text-primary);
    box-shadow: var(--shadow-xs);
  }

  .lang-btn:hover:not(.active),
  .theme-btn:hover:not(.active) {
    color: var(--text-secondary);
    background: var(--glass-bg-subtle);
  }

  .sidebar-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
  }

  .gateway-status {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .status-text {
    font-size: 11px;
    color: var(--text-muted);
  }

  .version {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
</style>
