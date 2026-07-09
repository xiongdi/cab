export type CatalogLogoKind = 'provider' | 'lab';

/** Provider logo from static/logos/ — served by Vite dev or built frontend */
export function providerLogoUrl(providerId: string): string {
  const slug = providerId.trim().toLowerCase();
  return `/logos/${encodeURIComponent(slug)}.svg`;
}

/** Lab logo from static/logos/labs/ */
export function labLogoUrl(labId: string): string {
  const slug = labId.trim().toLowerCase();
  return `/logos/labs/${encodeURIComponent(slug)}.svg`;
}

export function catalogLogoUrl(id: string, kind: CatalogLogoKind = 'provider'): string {
  return kind === 'lab' ? labLogoUrl(id) : providerLogoUrl(id);
}

/** First segment of a models.dev catalog id, e.g. `deepseek/deepseek-chat` → `deepseek`. */
export function modelLabId(catalogModelId: string): string | null {
  const lab = catalogModelId.split('/')[0]?.trim();
  return lab || null;
}

export function modelLabLogoUrl(catalogModelId: string): string | null {
  const lab = modelLabId(catalogModelId);
  return lab ? labLogoUrl(lab) : null;
}
