/** models.dev static assets (see https://models.dev) */
export const MODELS_DEV_ORIGIN = 'https://models.dev';

export type CatalogLogoKind = 'provider' | 'lab';

/** Gateway/provider logo: `/logos/{provider}.svg` */
export function providerLogoUrl(providerId: string): string {
  const slug = providerId.trim().toLowerCase();
  return `${MODELS_DEV_ORIGIN}/logos/${encodeURIComponent(slug)}.svg`;
}

/** Model vendor/lab logo: `/logos/labs/{lab}.svg` */
export function labLogoUrl(labId: string): string {
  const slug = labId.trim().toLowerCase();
  return `${MODELS_DEV_ORIGIN}/logos/labs/${encodeURIComponent(slug)}.svg`;
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
