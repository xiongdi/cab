<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toast from '$lib/components/Toast.svelte';
  import { i18n } from '$lib/i18n.svelte';
  import { gatewayHealth } from '$lib/gateway-health.svelte';

  let { children } = $props();

  onMount(() => gatewayHealth.start(10_000));
  onDestroy(() => gatewayHealth.stop());

  $effect(() => {
    if (typeof document === 'undefined') return;
    document.documentElement.lang = i18n.currentLang === 'zh' ? 'zh-CN' : 'en';
  });
</script>

<svelte:head>
  <title>{i18n.t('meta.title')}</title>
</svelte:head>

<div class="app-layout">
  <Sidebar />
  <main class="main-content">
    <div class="content-inner fade-in">
      {@render children()}
    </div>
  </main>
</div>

<Toast />

<style>
  .app-layout {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }

  .main-content {
    flex: 1;
    margin-left: var(--sidebar-width);
    overflow-y: auto;
    overflow-x: hidden;
  }

  .content-inner {
    padding: 32px 36px 48px;
    width: 100%;
    min-height: 100%;
  }
</style>
