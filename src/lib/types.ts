/* ═══════════════════════════════════════════════════════════════
   CAB — TypeScript Types
   API types are generated from openapi.yaml → see api-types.ts
   This file re-exports them and adds frontend-only types.
   ═══════════════════════════════════════════════════════════════ */

export type * from './api-types';
export {
  type ApiKeyConfig,
  type ProviderEndpoint,
  type Provider,
  type UpdateProvider,
  type Model,
  type RoutableModel,
  type ModelEndpoint,
  type UpdateModel,
  type Route,
  type CreateRoute,
  type UpdateRoute,
  type RequestLog,
  type LogFilter,
  type PaginatedLogs,
  type DashboardStats,
  type ProviderUserSettings,
  type ModelUserSettings,
  type BenchmarkEvaluations,
  type BenchmarkPerformance,
  type BenchmarkModelRecord,
  type ModelCatalogEntry,
  type Settings,
  type UpdateSettings,
  type CatalogSourceStatus,
  type CatalogStatusResponse,
  type SyncCatalogResponse,
  type Agent,
  type UpdateAgent,
  type RouteExplainRequest,
  type DecisionStep,
  type ResolvedSummary,
  type RankedModelSummary,
  type RouteExplainResult,
  type StrategyBoardRequest,
  type StrategyBoardStrategy,
  type StrategyBoardResult,
} from './api-types';

// ── Frontend-only types (not from API) ─────────────────────────

export interface Column<T = any> {
  key: string;
  label: string;
  sortable?: boolean;
  width?: string;
  align?: 'left' | 'center' | 'right';
  render?: (value: any, row: T) => string;
}

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}
