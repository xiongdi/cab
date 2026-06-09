import type { Agent } from './types';

/** Map legacy CAB modes to v0.1 UI modes. */
export function normalizeLoadedMode(mode: string): Agent['mode'] {
  if (mode === 'config') return 'auto';
  if (mode === 'proxy') return 'native';
  return mode as Agent['mode'];
}

export function modeBadgeClass(mode: Agent['mode']): string {
  if (mode === 'native') return 'badge-neutral';
  if (mode === 'auto') return 'badge-warning';
  return 'badge-success';
}

export function isSupportedAgentMode(mode: string): mode is Agent['mode'] {
  return mode === 'native' || mode === 'auto' || mode === 'manual';
}
