/** Brand tile colors for models.dev provider/lab logos (SVGs use currentColor). */
export type CatalogBrandStyle = {
  bg: string;
  fg: string;
};

const BRAND_STYLES: Record<string, CatalogBrandStyle> = {
  anthropic: { bg: '#c96442', fg: '#ffffff' },
  openai: { bg: '#10a37f', fg: '#ffffff' },
  google: { bg: '#4285f4', fg: '#ffffff' },
  meta: { bg: '#0082fb', fg: '#ffffff' },
  'meta-llama': { bg: '#0082fb', fg: '#ffffff' },
  deepseek: { bg: '#4d6bfe', fg: '#ffffff' },
  mistral: { bg: '#ff6b35', fg: '#ffffff' },
  mistralai: { bg: '#ff6b35', fg: '#ffffff' },
  alibaba: { bg: '#ff6a00', fg: '#ffffff' },
  qwen: { bg: '#7c3aed', fg: '#ffffff' },
  baidu: { bg: '#2932e1', fg: '#ffffff' },
  minimax: { bg: '#06b6d4', fg: '#ffffff' },
  moonshotai: { bg: '#6366f1', fg: '#ffffff' },
  bytedance: { bg: '#fe2c55', fg: '#ffffff' },
  'bytedance-seed': { bg: '#fe2c55', fg: '#ffffff' },
  tencent: { bg: '#1296db', fg: '#ffffff' },
  xiaomi: { bg: '#ff6900', fg: '#ffffff' },
  stepfun: { bg: '#8b5cf6', fg: '#ffffff' },
  zhipuai: { bg: '#0066ff', fg: '#ffffff' },
  'z-ai': { bg: '#0ea5e9', fg: '#ffffff' },
  cohere: { bg: '#39d353', fg: '#0f172a' },
  perplexity: { bg: '#20b2aa', fg: '#ffffff' },
  xai: { bg: '#111827', fg: '#ffffff' },
  'x-ai': { bg: '#111827', fg: '#ffffff' },
  microsoft: { bg: '#0078d4', fg: '#ffffff' },
  amazon: { bg: '#ff9900', fg: '#111827' },
  'amazon-bedrock': { bg: '#ff9900', fg: '#111827' },
  azure: { bg: '#0078d4', fg: '#ffffff' },
  nvidia: { bg: '#76b900', fg: '#111827' },
  ibm: { bg: '#0f62fe', fg: '#ffffff' },
  'ibm-granite': { bg: '#0f62fe', fg: '#ffffff' },
  openrouter: { bg: '#6366f1', fg: '#ffffff' },
  groq: { bg: '#f55036', fg: '#ffffff' },
  together: { bg: '#0ea5e9', fg: '#ffffff' },
  fireworks: { bg: '#7c3aed', fg: '#ffffff' },
  ollama: { bg: '#ffffff', fg: '#111827' },
  vercel: { bg: '#111827', fg: '#ffffff' },
  cloudflare: { bg: '#f6821f', fg: '#ffffff' },
  upstage: { bg: '#2563eb', fg: '#ffffff' },
  huggingface: { bg: '#ffd21e', fg: '#111827' },
};

const PREFIX_RULES: Array<[string, string]> = [
  ['alibaba', 'alibaba'],
  ['anthropic', 'anthropic'],
  ['openai', 'openai'],
  ['google', 'google'],
  ['azure', 'azure'],
  ['amazon', 'amazon'],
  ['mistral', 'mistral'],
  ['moonshot', 'moonshotai'],
  ['deepseek', 'deepseek'],
  ['zhipu', 'zhipuai'],
  ['glm', 'zhipuai'],
  ['minimax', 'minimax'],
  ['meta', 'meta'],
  ['llama', 'meta'],
  ['cohere', 'cohere'],
  ['xai', 'xai'],
  ['grok', 'xai'],
];

function normalizeBrandKey(id: string): string {
  return id.trim().toLowerCase();
}

function styleForKey(key: string): CatalogBrandStyle | undefined {
  if (BRAND_STYLES[key]) return BRAND_STYLES[key];
  for (const [prefix, target] of PREFIX_RULES) {
    if (key.startsWith(prefix) && BRAND_STYLES[target]) {
      return BRAND_STYLES[target];
    }
  }
  return undefined;
}

function hashColor(key: string): CatalogBrandStyle {
  let hash = 0;
  for (let i = 0; i < key.length; i += 1) {
    hash = key.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  return { bg: `hsl(${hue} 58% 42%)`, fg: '#ffffff' };
}

/** Resolve brand tile colors for a models.dev provider or lab id. */
export function catalogBrandStyle(id: string): CatalogBrandStyle {
  const key = normalizeBrandKey(id);
  return styleForKey(key) ?? hashColor(key);
}
