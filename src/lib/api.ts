/* ═══════════════════════════════════════════════════════════════
   CAB — REST API Client
   Communicates with the Rust backend on localhost:3125
   ═══════════════════════════════════════════════════════════════ */

import type {
  Provider,
  UpdateProvider,
  Model,
  ModelCatalogEntry,
  ModelEndpoint,
  UpdateModel,
  Route,
  CreateRoute,
  UpdateRoute,
  PaginatedLogs,
  LogFilter,
  DashboardStats,
  Settings,
  UpdateSettings,
  CatalogStatusResponse,
  SyncCatalogResponse,
  Agent,
  UpdateAgent,
  RouteExplainRequest,
  RouteExplainResult,
  RoutableModel,
} from './types';

let resolvedPort: number | null = null;

function isTauriRuntime(): boolean {
  if (typeof window === 'undefined') return false;
  return '__TAURI_INTERNALS__' in window || '__TAURI__' in window;
}

async function getApiBase(): Promise<string> {
  if (resolvedPort !== null) {
    return `http://127.0.0.1:${resolvedPort}/api`;
  }

  // UI served on gateway port — same origin, no Tauri invoke needed.
  if (typeof window !== 'undefined' && window.location.port === '3125') {
    resolvedPort = 3125;
    return 'http://127.0.0.1:3125/api';
  }

  // Tauri asset protocol: ask Rust for the gateway port.
  if (isTauriRuntime()) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const port = await invoke<number>('get_gateway_port');
      resolvedPort = port;
      return `http://127.0.0.1:${port}/api`;
    } catch (e) {
      console.warn('Failed to get gateway port from Tauri, using default 3125', e);
    }
  }

  resolvedPort = 3125;
  return 'http://127.0.0.1:3125/api';
}

class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const apiBase = await getApiBase();
  try {
    const res = await fetch(`${apiBase}${path}`, {
      cache: 'no-store',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      ...options,
    });

    if (!res.ok) {
      const text = await res.text().catch(() => 'Unknown error');
      throw new ApiError(text, res.status);
    }

    // Handle 204 No Content
    if (res.status === 204) {
      return undefined as T;
    }

    return await res.json();
  } catch (err) {
    if (err instanceof ApiError) throw err;
    throw new ApiError(
      err instanceof TypeError ? 'Cannot connect to CAB gateway. Is it running?' : String(err),
      0
    );
  }
}

export const api = {
  // ── Dashboard ─────────────────────────────────────────────
  dashboard: {
    getStats: () => request<DashboardStats>('/dashboard/stats'),
  },

  // ── Providers ─────────────────────────────────────────────
  providers: {
    list: () => request<Provider[]>('/providers'),

    get: (id: string) => request<Provider>(`/providers/${id}`),

    update: (id: string, data: UpdateProvider) =>
      request<Provider>(`/providers/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data),
      }),
  },

  // ── Models ────────────────────────────────────────────────
  models: {
    list: () => request<Model[]>('/models'),

    listRoutable: () => request<RoutableModel[]>('/models/routable'),

    listCatalog: () => request<ModelCatalogEntry[]>('/models/catalog'),

    get: (id: string) => request<Model>(`/models/${id}`),

    endpoints: (modelName: string) =>
      request<ModelEndpoint[]>(`/models/${encodeURIComponent(modelName)}/endpoints`),

    updateEndpoint: (id: string, enabled: boolean) =>
      request<ModelEndpoint>('/model-endpoints', {
        method: 'PUT',
        body: JSON.stringify({ id, enabled }),
      }),

    update: (id: string, data: UpdateModel) =>
      request<Model>(`/models/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data),
      }),
  },

  // ── Routes ────────────────────────────────────────────────
  routes: {
    list: () => request<Route[]>('/routes'),

    get: (id: string) => request<Route>(`/routes/${id}`),

    create: (data: CreateRoute) =>
      request<Route>('/routes', {
        method: 'POST',
        body: JSON.stringify(data),
      }),

    update: (id: string, data: UpdateRoute) =>
      request<Route>(`/routes/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data),
      }),

    delete: (id: string) => request<void>(`/routes/${id}`, { method: 'DELETE' }),

    explain: (data: RouteExplainRequest) =>
      request<RouteExplainResult>('/routing/explain', {
        method: 'POST',
        body: JSON.stringify(data),
      }),
  },

  // ── Logs ──────────────────────────────────────────────────
  logs: {
    list: (filter?: LogFilter) => {
      const params = new URLSearchParams();
      if (filter) {
        Object.entries(filter).forEach(([key, value]) => {
          if (value !== undefined && value !== '') {
            params.set(key, String(value));
          }
        });
      }
      const qs = params.toString();
      return request<PaginatedLogs>(`/logs${qs ? `?${qs}` : ''}`);
    },
  },

  // ── Settings ──────────────────────────────────────────────
  settings: {
    get: () => request<Settings>('/settings'),

    update: (data: UpdateSettings) =>
      request<Settings>('/settings', {
        method: 'PUT',
        body: JSON.stringify(data),
      }),

    getCatalogStatus: () => request<CatalogStatusResponse>('/settings/catalog-status'),

    syncCatalog: () => request<SyncCatalogResponse>('/settings/sync-catalog', { method: 'POST' }),
  },

  // ── Agents ────────────────────────────────────────────────
  agents: {
    list: () => request<Agent[]>('/agents'),
    get: (id: string) => request<Agent>(`/agents/${id}`),
    update: (id: string, data: UpdateAgent) =>
      request<Agent>(`/agents/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data),
      }),
  },
};

export { ApiError };
