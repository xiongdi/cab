<script lang="ts">
  import { catalogBrandStyle } from '$lib/catalog-brand-colors';
  import { catalogLogoUrl, type CatalogLogoKind } from '$lib/models-dev';

  let {
    id,
    kind = 'provider',
    size = 20,
    alt = '',
    class: className = ''
  }: {
    id: string;
    kind?: CatalogLogoKind;
    size?: number;
    alt?: string;
    class?: string;
  } = $props();

  const svgCache = new Map<string, string>();

  let failed = $state(false);
  let svgMarkup = $state<string | null>(null);
  const src = $derived(catalogLogoUrl(id, kind));
  const label = $derived(alt || id);
  const initial = $derived(id.trim().charAt(0).toUpperCase() || '?');
  const brand = $derived(catalogBrandStyle(id));
  const showFallback = $derived(!id || failed);

  async function loadSvg(url: string): Promise<string | null> {
    const cached = svgCache.get(url);
    if (cached) return cached;

    try {
      const response = await fetch(url);
      if (!response.ok) return null;
      let markup = await response.text();
      if (!markup.includes('<svg')) return null;
      markup = markup
        .replace(/<\?xml[^?]*\?>/gi, '')
        .replace(/\swidth="[^"]*"/i, '')
        .replace(/\sheight="[^"]*"/i, '');
      svgCache.set(url, markup);
      return markup;
    } catch {
      return null;
    }
  }

  $effect(() => {
    failed = false;
    svgMarkup = null;
    if (!id) return;

    let cancelled = false;
    loadSvg(src).then((markup) => {
      if (cancelled) return;
      if (markup) {
        svgMarkup = markup;
      } else {
        failed = true;
      }
    });

    return () => {
      cancelled = true;
    };
  });
</script>

<span
  class="catalog-logo-shell {className}"
  class:is-fallback={showFallback}
  style:width="{size}px"
  style:height="{size}px"
  style:color={brand.fg}
  style:background={brand.bg}
  style:border-color="{brand.bg}55"
  title={label}
>
  {#if svgMarkup}
    <span class="catalog-logo-inline" aria-hidden={alt ? undefined : true}>
      {@html svgMarkup}
    </span>
  {:else if showFallback}
    <span
      class="catalog-logo-fallback"
      style:font-size="{Math.max(10, Math.round(size * 0.42))}px"
      aria-hidden={alt ? undefined : true}
    >{initial}</span>
  {:else}
    <span class="catalog-logo-spinner" aria-hidden="true"></span>
  {/if}
</span>

<style>
  .catalog-logo-inline {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }

  .catalog-logo-inline :global(svg) {
    display: block;
    width: 72%;
    height: 72%;
  }

  .catalog-logo-spinner {
    display: block;
    width: 40%;
    height: 40%;
    border-radius: 999px;
    border: 2px solid currentColor;
    border-top-color: transparent;
    opacity: 0.35;
    animation: catalog-logo-spin 0.8s linear infinite;
  }

  @keyframes catalog-logo-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
