import { describe, expect, it } from 'vitest';
import {
  aggregateCacheHitRatePct,
  cacheHitRatePct,
  displayInputTokens,
  formatCacheHitRatePct,
  freshInputTokens,
  isAnthropicDisjointCache,
  isLegacyInclusiveInput,
  promptTokens,
} from './cache-tokens';

describe('cache-tokens', () => {
  it('Anthropic: write is a disjoint prompt part', () => {
    const u = {
      path: '/v1/messages',
      input_tokens: 225,
      output_tokens: 12,
      cache_read_tokens: 34560,
      cache_creation_tokens: 100,
      total_tokens: 225 + 34560 + 100 + 12,
    };
    expect(isAnthropicDisjointCache(u)).toBe(true);
    expect(freshInputTokens(u)).toBe(225);
    expect(displayInputTokens(u)).toBe(225 + 34560 + 100);
    expect(cacheHitRatePct(u)).toBe(Math.round((34560 / (225 + 34560 + 100)) * 10000) / 100);
  });

  it('OpenAI: write overlays input; display prompt = input + read only', () => {
    const u = {
      path: '/v1/chat/completions',
      input_tokens: 60, // prompt 100 - read 40; write 10 sits inside the 60
      output_tokens: 5,
      cache_read_tokens: 40,
      cache_creation_tokens: 10,
      total_tokens: 105,
    };
    expect(isAnthropicDisjointCache(u)).toBe(false);
    expect(freshInputTokens(u)).toBe(60);
    expect(displayInputTokens(u)).toBe(100);
    expect(promptTokens(u)).toBe(100);
    expect(cacheHitRatePct(u)).toBe(40);
    expect(formatCacheHitRatePct(cacheHitRatePct(u)!)).toBe('40.00');
  });

  it('corrects legacy OpenAI inclusive input', () => {
    const u = {
      path: '/v1/chat/completions',
      input_tokens: 9328,
      output_tokens: 27,
      cache_read_tokens: 1024,
      cache_creation_tokens: 0,
      total_tokens: 9355,
    };
    expect(isLegacyInclusiveInput(u)).toBe(true);
    expect(freshInputTokens(u)).toBe(9328 - 1024);
    expect(displayInputTokens(u)).toBe(9328);
    expect(cacheHitRatePct(u)).toBe(10.98);
  });

  it('aggregates hit rate by prompt tokens', () => {
    const rows = [
      {
        path: '/v1/messages',
        input_tokens: 100,
        output_tokens: 1,
        cache_read_tokens: 900,
        total_tokens: 1001,
      },
      {
        path: '/v1/messages',
        input_tokens: 100,
        output_tokens: 1,
        cache_read_tokens: 0,
        total_tokens: 101,
      },
    ];
    // 900/1100 = 81.818... → 81.82
    expect(aggregateCacheHitRatePct(rows)).toBe(81.82);
  });
});
