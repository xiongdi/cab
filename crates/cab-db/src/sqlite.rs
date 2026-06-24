use std::collections::HashMap;
use std::path::PathBuf;

use cab_core::types::{
    Agent, AgentUsageSummary, Model, ModelUsageSummary, PersistedState, Provider,
    ProviderUsageSummary, RequestLog, Route, Settings, UsageRecord, UsageSummary,
};
use rusqlite::Connection;
use serde_json;

use crate::endpoint::ModelEndpoint;

const SCHEMA_VERSION: u32 = 2;

pub fn db_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join(".cab-fallback");
            tracing::warn!(
                "Neither HOME nor USERPROFILE set; falling back to {}",
                tmp.display()
            );
            tmp.to_string_lossy().into_owned()
        });
    PathBuf::from(home).join(".cab").join("cab.db")
}

pub fn pool() -> Result<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        // Restrict ~/.cab directory to owner-only (0700)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            } else {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let manager = r2d2_sqlite::SqliteConnectionManager::file(&path);
    let pool = r2d2::Pool::builder()
        .max_size(4)
        .build(manager)
        .map_err(|e| format!("Failed to create SQLite pool: {e}"))?;

    // Restrict database file to owner-only (0600) — it contains gateway_key and provider API keys
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }

    Ok(pool)
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS schema_version (
             version INTEGER PRIMARY KEY
         );

         CREATE TABLE IF NOT EXISTS settings (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             data TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS agents (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             mode TEXT NOT NULL,
             model_id TEXT,
             api_key TEXT NOT NULL,
             endpoint TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS routes (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             agent_pattern TEXT NOT NULL,
             routing_strategy TEXT NOT NULL,
             model_id TEXT NOT NULL,
             fallback_ids TEXT NOT NULL DEFAULT '[]',
             priority INTEGER NOT NULL DEFAULT 0,
             enabled INTEGER NOT NULL DEFAULT 1,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS request_logs (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             agent TEXT NOT NULL,
             provider TEXT NOT NULL,
             model TEXT NOT NULL,
             input_tokens INTEGER NOT NULL,
             output_tokens INTEGER NOT NULL,
             total_tokens INTEGER NOT NULL,
             latency_ms INTEGER NOT NULL,
             status INTEGER NOT NULL,
             error TEXT,
             path TEXT NOT NULL,
             stream INTEGER NOT NULL DEFAULT 0
         );

         CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON request_logs(timestamp);
         CREATE INDEX IF NOT EXISTS idx_logs_agent ON request_logs(agent);
         CREATE INDEX IF NOT EXISTS idx_logs_provider ON request_logs(provider);

         CREATE TABLE IF NOT EXISTS usage_records (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             model_id TEXT NOT NULL,
             service_provider_id TEXT NOT NULL,
             agent_id TEXT NOT NULL,
             input_tokens INTEGER NOT NULL DEFAULT 0,
             output_tokens INTEGER NOT NULL DEFAULT 0,
             cache_read_tokens INTEGER NOT NULL DEFAULT 0,
             cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
             cost_usd REAL NOT NULL DEFAULT 0.0,
             subscription INTEGER NOT NULL DEFAULT 0,
             request_id TEXT
         );

         CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON usage_records(timestamp);
         CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_records(provider_id);
         CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model_id);

         CREATE TABLE IF NOT EXISTS subscription_quotas (
             provider_id TEXT NOT NULL,
             period_start TEXT NOT NULL,
             period_end TEXT NOT NULL,
             token_cap INTEGER NOT NULL,
             tokens_used INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (provider_id, period_start)
         );

         -- v2: catalog tables (providers, models, endpoints from models.dev sync)
         CREATE TABLE IF NOT EXISTS catalog_providers (
             id TEXT PRIMARY KEY,
             data TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS catalog_models (
             id TEXT PRIMARY KEY,
             provider_id TEXT NOT NULL,
             data TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS model_endpoints (
             id TEXT PRIMARY KEY,
             model_id TEXT NOT NULL,
             data TEXT NOT NULL
         );

         CREATE INDEX IF NOT EXISTS idx_catalog_models_provider ON catalog_models(provider_id);
         CREATE INDEX IF NOT EXISTS idx_model_endpoints_model ON model_endpoints(model_id);
         ",
    )
    .map_err(|e| format!("Schema init failed: {e}"))?;

    let current: Option<u32> = conn
        .query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    match current {
        None => {
            // Fresh install
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                rusqlite::params![SCHEMA_VERSION],
            )
            .map_err(|e| format!("Failed to set schema version: {e}"))?;
        }
        Some(v) if v < SCHEMA_VERSION => {
            // Run migrations sequentially
            if v < 2 {
                migrate_v1_to_v2(conn)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<(), String> {
    tracing::info!("Migrating schema from v1 to v2: importing catalog data into SQLite");

    // Try to import from existing JSON cache files
    if let Err(e) = import_catalog_from_json_cache(conn) {
        tracing::warn!("Catalog import from JSON cache (optional): {e}");
    }

    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
        rusqlite::params![SCHEMA_VERSION],
    )
    .map_err(|e| format!("Failed to update schema version: {e}"))?;

    tracing::info!("Schema migration v1 → v2 complete");
    Ok(())
}

/// One-time import: read ~/.cab/catalog/models.dev/catalog.json and write to SQLite tables.
/// No-op if the file doesn't exist or tables already have data.
fn import_catalog_from_json_cache(conn: &Connection) -> Result<(), String> {
    // Only import if catalog_providers is empty
    let has_data: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM catalog_providers", [], |row| {
            row.get(0)
        })
        .unwrap_or(false);
    if has_data {
        return Ok(());
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let catalog_path = PathBuf::from(home)
        .join(".cab")
        .join("catalog")
        .join("models.dev")
        .join("catalog.json");

    if !catalog_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&catalog_path)
        .map_err(|e| format!("Read {catalog_path:?}: {e}"))?;
    let raw: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Parse catalog.json: {e}"))?;

    // Import providers
    if let Some(providers) = raw.get("providers").and_then(|v| v.as_object()) {
        for (id, val) in providers {
            let data = serde_json::to_string(val).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR IGNORE INTO catalog_providers (id, data) VALUES (?1, ?2)",
                rusqlite::params![id, data],
            )
            .map_err(|e| format!("Insert provider {id}: {e}"))?;
        }
        tracing::info!("Imported {} providers from catalog cache", providers.len());
    }

    // Import models
    if let Some(models) = raw.get("models").and_then(|v| v.as_object()) {
        for (id, val) in models {
            let provider_id = val
                .get("provider_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = serde_json::to_string(val).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR IGNORE INTO catalog_models (id, provider_id, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, provider_id, data],
            )
            .map_err(|e| format!("Insert model {id}: {e}"))?;
        }
        tracing::info!("Imported {} models from catalog cache", models.len());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Catalog Providers CRUD
// ---------------------------------------------------------------------------

pub fn save_catalog_providers(
    conn: &Connection,
    providers: &HashMap<String, Provider>,
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM catalog_providers", [])
        .map_err(|e| e.to_string())?;
    for (id, provider) in providers {
        let data = serde_json::to_string(provider).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO catalog_providers (id, data) VALUES (?1, ?2)",
            rusqlite::params![id, data],
        )
        .map_err(|e| format!("Insert provider {id}: {e}"))?;
    }
    tx.commit().map_err(|e| format!("Commit providers: {e}"))?;
    Ok(())
}

pub fn load_catalog_providers(conn: &Connection) -> Result<HashMap<String, Provider>, String> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM catalog_providers")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((id, data))
        })
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, data) = row.map_err(|e| e.to_string())?;
        if let Ok(provider) = serde_json::from_str::<Provider>(&data) {
            map.insert(id, provider);
        } else {
            tracing::warn!("Skipping invalid provider data for {id}");
        }
    }
    Ok(map)
}

pub fn upsert_catalog_provider(conn: &Connection, provider: &Provider) -> Result<(), String> {
    let data = serde_json::to_string(provider).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO catalog_providers (id, data) VALUES (?1, ?2)",
        rusqlite::params![provider.id, data],
    )
    .map_err(|e| format!("Upsert provider {}: {e}", provider.id))?;
    Ok(())
}

pub fn delete_catalog_provider(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn
        .execute(
            "DELETE FROM catalog_providers WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| format!("Delete provider {id}: {e}"))?;
    Ok(n > 0)
}

// ---------------------------------------------------------------------------
// Catalog Models CRUD
// ---------------------------------------------------------------------------

pub fn save_catalog_models(
    conn: &Connection,
    models: &HashMap<String, Model>,
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM catalog_models", [])
        .map_err(|e| e.to_string())?;
    for (id, model) in models {
        let data = serde_json::to_string(model).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO catalog_models (id, provider_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, model.provider_id, data],
        )
        .map_err(|e| format!("Insert model {id}: {e}"))?;
    }
    tx.commit().map_err(|e| format!("Commit models: {e}"))?;
    Ok(())
}

pub fn load_catalog_models(conn: &Connection) -> Result<HashMap<String, Model>, String> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM catalog_models")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((id, data))
        })
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, data) = row.map_err(|e| e.to_string())?;
        if let Ok(model) = serde_json::from_str::<Model>(&data) {
            map.insert(id, model);
        } else {
            tracing::warn!("Skipping invalid model data for {id}");
        }
    }
    Ok(map)
}

pub fn upsert_catalog_model(conn: &Connection, model: &Model) -> Result<(), String> {
    let data = serde_json::to_string(model).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO catalog_models (id, provider_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![model.id, model.provider_id, data],
    )
    .map_err(|e| format!("Upsert model {}: {e}", model.id))?;
    Ok(())
}

pub fn delete_catalog_model(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn
        .execute(
            "DELETE FROM catalog_models WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| format!("Delete model {id}: {e}"))?;
    Ok(n > 0)
}

// ---------------------------------------------------------------------------
// Model Endpoints CRUD
// ---------------------------------------------------------------------------

pub fn save_model_endpoints(
    conn: &Connection,
    endpoints: &HashMap<String, ModelEndpoint>,
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM model_endpoints", [])
        .map_err(|e| e.to_string())?;
    for (id, ep) in endpoints {
        let data = serde_json::to_string(ep).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO model_endpoints (id, model_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, ep.model_id, data],
        )
        .map_err(|e| format!("Insert endpoint {id}: {e}"))?;
    }
    tx.commit().map_err(|e| format!("Commit endpoints: {e}"))?;
    Ok(())
}

pub fn load_model_endpoints(conn: &Connection) -> Result<HashMap<String, ModelEndpoint>, String> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM model_endpoints")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((id, data))
        })
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, data) = row.map_err(|e| e.to_string())?;
        if let Ok(ep) = serde_json::from_str::<ModelEndpoint>(&data) {
            map.insert(id, ep);
        } else {
            tracing::warn!("Skipping invalid model_endpoint data for {id}");
        }
    }
    Ok(map)
}

pub fn upsert_model_endpoint(conn: &Connection, ep: &ModelEndpoint) -> Result<(), String> {
    let data = serde_json::to_string(ep).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO model_endpoints (id, model_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![ep.id, ep.model_id, data],
    )
    .map_err(|e| format!("Upsert endpoint {}: {e}", ep.id))?;
    Ok(())
}

pub fn clear_model_endpoints(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM model_endpoints", [])
        .map_err(|e| format!("Clear endpoints: {e}"))?;
    Ok(())
}

pub fn delete_model_endpoints_for(conn: &Connection, model_id: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "DELETE FROM model_endpoints WHERE model_id = ?1",
            rusqlite::params![model_id],
        )
        .map_err(|e| format!("Delete endpoints for {model_id}: {e}"))?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub fn save_settings(conn: &Connection, settings: &Settings) -> Result<(), String> {
    let data = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO settings (id, data) VALUES (1, ?1)",
        rusqlite::params![data],
    )
    .map_err(|e| format!("Save settings failed: {e}"))?;
    Ok(())
}

pub fn load_settings(conn: &Connection) -> Result<Option<Settings>, String> {
    let row: Result<String, _> =
        conn.query_row("SELECT data FROM settings WHERE id = 1", [], |row| {
            row.get(0)
        });
    match row {
        Ok(data) => {
            let settings = serde_json::from_str::<Settings>(&data).map_err(|e| e.to_string())?;
            Ok(Some(settings))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Load settings failed: {e}")),
    }
}

// ---------------------------------------------------------------------------
// State (agents + routes)
// ---------------------------------------------------------------------------

pub fn save_state(
    conn: &Connection,
    agents: &HashMap<String, Agent>,
    routes: &HashMap<String, Route>,
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM agents", [])
        .map_err(|e| e.to_string())?;
    for agent in agents.values() {
        tx.execute(
            "INSERT OR REPLACE INTO agents (id, name, mode, model_id, api_key, endpoint, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                agent.id,
                agent.name,
                agent.mode,
                agent.model_id,
                agent.api_key,
                agent.endpoint,
                agent.updated_at,
            ],
        )
        .map_err(|e| format!("Insert agent failed: {e}"))?;
    }
    tx.execute("DELETE FROM routes", [])
        .map_err(|e| e.to_string())?;
    for route in routes.values() {
        let fallback = serde_json::to_string(&route.fallback_ids).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO routes
             (id, name, agent_pattern, routing_strategy, model_id, fallback_ids, priority, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                route.id,
                route.name,
                route.agent_pattern,
                route.routing_strategy,
                route.model_id,
                fallback,
                route.priority,
                route.enabled as i64,
                route.created_at,
                route.updated_at,
            ],
        )
        .map_err(|e| format!("Insert route failed: {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("State commit failed: {e}"))?;
    Ok(())
}

pub fn load_state(conn: &Connection) -> Result<PersistedState, String> {
    let mut agents = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT id, name, mode, model_id, api_key, endpoint, updated_at FROM agents")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                mode: row.get(2)?,
                model_id: row.get(3)?,
                api_key: row.get(4)?,
                endpoint: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let agent = row.map_err(|e| e.to_string())?;
        agents.insert(agent.id.clone(), agent);
    }

    let mut routes = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, agent_pattern, routing_strategy, model_id, fallback_ids, priority, enabled, created_at, updated_at FROM routes",
        )
        .map_err(|e| e.to_string())?;
    let route_rows = stmt
        .query_map([], |row| {
            let fallback_str: String = row.get(5)?;
            let fallback_ids: Vec<String> = serde_json::from_str(&fallback_str).unwrap_or_default();
            Ok(Route {
                id: row.get(0)?,
                name: row.get(1)?,
                agent_pattern: row.get(2)?,
                routing_strategy: row.get(3)?,
                model_id: row.get(4)?,
                fallback_ids,
                priority: row.get(6)?,
                enabled: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in route_rows {
        let route = row.map_err(|e| e.to_string())?;
        routes.insert(route.id.clone(), route);
    }

    Ok(PersistedState {
        version: crate::state::STATE_VERSION,
        agents,
        routes,
    })
}

// ---------------------------------------------------------------------------
// Request Logs
// ---------------------------------------------------------------------------

pub fn append_log(conn: &Connection, log: &RequestLog) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO request_logs
         (id, timestamp, agent, provider, model, input_tokens, output_tokens, total_tokens, latency_ms, status, error, path, stream)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            log.id,
            log.timestamp,
            log.agent,
            log.provider,
            log.model,
            log.input_tokens,
            log.output_tokens,
            log.total_tokens,
            log.latency_ms,
            log.status,
            log.error,
            log.path,
            log.stream as i64,
        ],
    )
    .map_err(|e| format!("Append log failed: {e}"))?;
    Ok(())
}

pub fn update_log_tokens(
    conn: &Connection,
    log_id: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE request_logs SET input_tokens = ?2, output_tokens = ?3, total_tokens = ?4 WHERE id = ?1",
        rusqlite::params![log_id, input_tokens, output_tokens, input_tokens + output_tokens],
    )
    .map_err(|e| format!("Update log tokens failed: {e}"))?;
    Ok(())
}

pub fn load_logs(conn: &Connection, limit: usize) -> Result<Vec<RequestLog>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, agent, provider, model, input_tokens, output_tokens, total_tokens, latency_ms, status, error, path, stream
             FROM request_logs ORDER BY timestamp DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            Ok(RequestLog {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                agent: row.get(2)?,
                provider: row.get(3)?,
                model: row.get(4)?,
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                total_tokens: row.get(7)?,
                latency_ms: row.get(8)?,
                status: row.get(9)?,
                error: row.get(10)?,
                path: row.get(11)?,
                stream: row.get::<_, i64>(12)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut logs = Vec::new();
    for row in rows {
        logs.push(row.map_err(|e| e.to_string())?);
    }
    logs.reverse();
    Ok(logs)
}

pub fn enforce_log_retention(conn: &Connection, retention_days: i64) -> Result<usize, String> {
    if retention_days <= 0 {
        return Ok(0);
    }
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
    let cutoff_str = cutoff.to_rfc3339();
    let count = conn
        .execute(
            "DELETE FROM request_logs WHERE timestamp < ?1",
            rusqlite::params![cutoff_str],
        )
        .map_err(|e| format!("Retention delete failed: {e}"))?;
    Ok(count)
}

pub fn clear_all_logs(conn: &Connection) -> Result<i64, String> {
    let count = conn
        .execute("DELETE FROM request_logs", [])
        .map_err(|e| format!("Clear logs failed: {e}"))?;
    Ok(count as i64)
}

// ---------------------------------------------------------------------------
// Usage Records
// ---------------------------------------------------------------------------

pub fn insert_usage(conn: &Connection, record: &UsageRecord) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO usage_records
         (id, timestamp, provider_id, model_id, service_provider_id, agent_id,
          input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
          cost_usd, subscription, request_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            record.id,
            record.timestamp,
            record.provider_id,
            record.model_id,
            record.service_provider_id,
            record.agent_id,
            record.input_tokens,
            record.output_tokens,
            record.cache_read_tokens,
            record.cache_creation_tokens,
            record.cost_usd,
            record.subscription as i64,
            record.request_id,
        ],
    )
    .map_err(|e| format!("Insert usage failed: {e}"))?;
    Ok(())
}

pub fn query_usage(conn: &Connection, since: &str, limit: i64) -> Result<Vec<UsageRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, provider_id, model_id, service_provider_id, agent_id,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    cost_usd, subscription, request_id
             FROM usage_records WHERE timestamp >= ?1
             ORDER BY timestamp DESC LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![since, limit], |row| {
            Ok(UsageRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                service_provider_id: row.get(4)?,
                agent_id: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                cache_read_tokens: row.get(8)?,
                cache_creation_tokens: row.get(9)?,
                cost_usd: row.get(10)?,
                subscription: row.get::<_, i64>(11)? != 0,
                request_id: row.get(12)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| e.to_string())?);
    }
    Ok(records)
}

pub fn usage_summary(conn: &Connection, since: &str) -> Result<UsageSummary, String> {
    let mut summary = UsageSummary::default();

    conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
         FROM usage_records WHERE timestamp >= ?1",
        rusqlite::params![since],
        |row| {
            summary.total_requests = row.get(0)?;
            summary.total_input_tokens = row.get(1)?;
            summary.total_output_tokens = row.get(2)?;
            summary.total_cost_usd = row.get(3)?;
            Ok(())
        },
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT provider_id, COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_records WHERE timestamp >= ?1 GROUP BY provider_id",
        )
        .map_err(|e| e.to_string())?;
    let provider_rows = stmt
        .query_map(rusqlite::params![since], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ProviderUsageSummary {
                    requests: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in provider_rows {
        let (id, ps) = row.map_err(|e| e.to_string())?;
        summary.by_provider.insert(id, ps);
    }

    let mut stmt = conn
        .prepare(
            "SELECT model_id, COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_records WHERE timestamp >= ?1 GROUP BY model_id",
        )
        .map_err(|e| e.to_string())?;
    let model_rows = stmt
        .query_map(rusqlite::params![since], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ModelUsageSummary {
                    requests: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in model_rows {
        let (id, ms) = row.map_err(|e| e.to_string())?;
        summary.by_model.insert(id, ms);
    }

    let mut stmt = conn
        .prepare(
            "SELECT agent_id, COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_records WHERE timestamp >= ?1 GROUP BY agent_id",
        )
        .map_err(|e| e.to_string())?;
    let agent_rows = stmt
        .query_map(rusqlite::params![since], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AgentUsageSummary {
                    requests: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in agent_rows {
        let (id, as_) = row.map_err(|e| e.to_string())?;
        summary.by_agent.insert(id, as_);
    }

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

pub fn init() -> Result<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, String> {
    let p = pool()?;
    {
        let conn = p.get().map_err(|e| format!("Pool get failed: {e}"))?;
        init_schema(&conn)?;
        tracing::info!("SQLite initialized at {}", db_path().display());
    }
    Ok(p)
}

#[cfg(test)]
pub fn test_pool() -> Result<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, String> {
    let manager = r2d2_sqlite::SqliteConnectionManager::memory();
    r2d2::Pool::builder()
        .max_size(2)
        .build(manager)
        .map_err(|e| format!("Failed to create test SQLite pool: {e}"))
}
