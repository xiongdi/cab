import { describe, expect, it } from 'vite-plus/test';
import { isSupportedAgentMode, modeBadgeClass, normalizeLoadedMode } from './agents';

describe('normalizeLoadedMode', () => {
  it('maps legacy config → auto', () => {
    expect(normalizeLoadedMode('config')).toBe('auto');
  });

  it('maps legacy proxy → native', () => {
    expect(normalizeLoadedMode('proxy')).toBe('native');
  });

  it('passes through v0.1 modes', () => {
    expect(normalizeLoadedMode('auto')).toBe('auto');
    expect(normalizeLoadedMode('native')).toBe('native');
    expect(normalizeLoadedMode('manual')).toBe('manual');
  });
});

describe('modeBadgeClass', () => {
  it('assigns distinct badge classes', () => {
    expect(modeBadgeClass('native')).toBe('badge-neutral');
    expect(modeBadgeClass('auto')).toBe('badge-warning');
    expect(modeBadgeClass('manual')).toBe('badge-success');
  });
});

describe('isSupportedAgentMode', () => {
  it('rejects legacy modes', () => {
    expect(isSupportedAgentMode('proxy')).toBe(false);
    expect(isSupportedAgentMode('config')).toBe(false);
  });

  it('accepts v0.1 modes', () => {
    expect(isSupportedAgentMode('auto')).toBe(true);
    expect(isSupportedAgentMode('native')).toBe(true);
    expect(isSupportedAgentMode('manual')).toBe(true);
  });
});
