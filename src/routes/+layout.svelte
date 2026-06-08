<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toast from '$lib/components/Toast.svelte';

  let { children } = $props();
</script>

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
