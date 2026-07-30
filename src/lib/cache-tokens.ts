/** CAB token fields from request logs / usage records. */
export type TokenUsageLike = {
  input_tokens?: number | null;
  output_tokens?: number | null;
  total_tokens?: number | null;
  cache_read_tokens?: number | null;
  cache_creation_tokens?: number | null;
  /** Client-facing gateway path — protocol source of truth. */
  path?: string | null;
};

/**
 * Anthropic: `input` / `cache_read` / `cache_creation` partition the prompt.
 * OpenAI: `cache_creation` (write) is a billing overlay on the non-read prompt.
 */
export function isAnthropicDisjointCache(u: TokenUsageLike): boolean {
  const path = (u.path ?? '').trim();
  if (path === '/v1/messages') return true;
  if (path === '/v1/chat/completions' || path === '/v1/responses') return false;

  const input = u.input_tokens ?? 0;
  const cacheRead = u.cache_read_tokens ?? 0;
  const cacheCreation = u.cache_creation_tokens ?? 0;
  const output = u.output_tokens ?? 0;
  const total = u.total_tokens ?? 0;
  if (cacheCreation <= 0 || total <= 0) return false;
  return Math.abs(total - (input + cacheRead + cacheCreation + output)) <= 1;
}

/** Pre-normalization OpenAI rows: `input` still included cache read. */
export function isLegacyInclusiveInput(u: TokenUsageLike): boolean {
  const path = (u.path ?? '').trim();
  if (path === '/v1/messages') return false;

  const input = u.input_tokens ?? 0;
  const cacheRead = u.cache_read_tokens ?? 0;
  const output = u.output_tokens ?? 0;
  const total = u.total_tokens ?? 0;

  if (cacheRead <= 0) return false;
  if (input < cacheRead) return false;
  // Old OpenAI: total ≈ input + output (cache read still inside input).
  return total > 0 && Math.abs(total - (input + output)) <= 1;
}

/**
 * Prompt tokens not served from cache read.
 * (On Anthropic this also excludes cache write; on OpenAI write stays inside.)
 */
export function freshInputTokens(u: TokenUsageLike): number {
  const input = u.input_tokens ?? 0;
  const cacheRead = u.cache_read_tokens ?? 0;
  if (isLegacyInclusiveInput(u)) {
    return Math.max(0, input - cacheRead);
  }
  return Math.max(0, input);
}

/**
 * Full prompt size for display / hit-rate denominator.
 * - Anthropic: input + cache_read + cache_creation
 * - OpenAI: input + cache_read  (write already inside input)
 */
export function displayInputTokens(u: TokenUsageLike): number {
  if (isLegacyInclusiveInput(u)) {
    return Math.max(0, u.input_tokens ?? 0);
  }
  const fresh = freshInputTokens(u);
  const cacheRead = Math.max(0, u.cache_read_tokens ?? 0);
  const cacheCreation = Math.max(0, u.cache_creation_tokens ?? 0);
  if (isAnthropicDisjointCache(u)) {
    return fresh + cacheRead + cacheCreation;
  }
  return fresh + cacheRead;
}

/** Completion tokens only. */
export function displayOutputTokens(u: TokenUsageLike): number {
  return Math.max(0, u.output_tokens ?? 0);
}

export function promptTokens(u: TokenUsageLike): number {
  return displayInputTokens(u);
}

/** Hit rate = cache_read / full prompt (percent, 2 decimal places). */
export function cacheHitRatePct(u: TokenUsageLike): number | null {
  const cacheRead = u.cache_read_tokens ?? 0;
  const prompt = promptTokens(u);
  if (prompt <= 0) return null;
  if (cacheRead <= 0) return 0;
  return Math.min(100, Math.round((cacheRead / prompt) * 10000) / 100);
}

export function aggregateCacheHitRatePct(rows: TokenUsageLike[]): number {
  let cacheRead = 0;
  let prompt = 0;
  for (const row of rows) {
    cacheRead += row.cache_read_tokens ?? 0;
    prompt += promptTokens(row);
  }
  if (prompt <= 0) return 0;
  return Math.min(100, Math.round((cacheRead / prompt) * 10000) / 100);
}

/** Format a hit-rate percent with exactly two decimal places. */
export function formatCacheHitRatePct(pct: number): string {
  return pct.toFixed(2);
}
