import { api, ApiError } from './api';

export type GatewayStatus = 'running' | 'stopped' | 'error';

export async function probeGatewayHealth(): Promise<GatewayStatus> {
  try {
    await api.dashboard.getStats();
    return 'running';
  } catch (err) {
    if (err instanceof ApiError && err.status > 0) {
      return 'error';
    }
    return 'stopped';
  }
}

class GatewayHealthManager {
  status = $state<GatewayStatus | 'checking'>('checking');
  private timer: ReturnType<typeof setInterval> | null = null;

  async refresh() {
    const next = await probeGatewayHealth();
    const prev = this.status;
    this.status = next;
    if (prev !== 'running' && next === 'running') {
      for (const listener of this.onRunningListeners) {
        listener();
      }
    }
  }

  onRunning(listener: () => void) {
    this.onRunningListeners.add(listener);
    return () => this.onRunningListeners.delete(listener);
  }

  private onRunningListeners = new Set<() => void>();

  start(intervalMs = 15_000) {
    void this.refresh();
    if (typeof window === 'undefined') return;
    this.stop();
    this.timer = setInterval(() => void this.refresh(), intervalMs);
  }

  stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}

export const gatewayHealth = new GatewayHealthManager();
