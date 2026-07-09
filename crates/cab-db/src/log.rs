use crate::InMemoryStore;
use cab_core::types::{LogQuery, PaginatedLogs, RequestLog};

const MAX_MEMORY_LOGS: usize = 500;

pub async fn insert(store: &InMemoryStore, log: &RequestLog) -> Result<(), String> {
    // Persist to SQLite (no-op if no pool, e.g. in tests)
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::append_log(&conn, log)?;
    }

    // Update in-memory ring buffer
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(pos) = inner.request_logs.iter().position(|l| l.id == log.id) {
        inner.request_logs[pos] = log.clone();
    } else {
        inner.request_logs.push(log.clone());
        if inner.request_logs.len() > MAX_MEMORY_LOGS {
            let overflow = inner.request_logs.len() - MAX_MEMORY_LOGS;
            inner.request_logs.drain(0..overflow);
        }
    }
    Ok(())
}

#[allow(clippy::collapsible_if)]
pub async fn query(store: &InMemoryStore, query: &LogQuery) -> Result<PaginatedLogs, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut logs: Vec<RequestLog> = inner.request_logs.to_vec();

    // Reverse list to show newest logs first
    logs.reverse();

    // Apply filters
    if let Some(ref agent) = query.agent {
        if !agent.is_empty() {
            logs.retain(|l| l.agent == *agent);
        }
    }
    if let Some(ref provider) = query.provider {
        if !provider.is_empty() {
            logs.retain(|l| l.provider == *provider);
        }
    }
    if let Some(ref model) = query.model {
        if !model.is_empty() {
            logs.retain(|l| l.model == *model);
        }
    }
    if let Some(ref status) = query.status {
        if !status.is_empty() {
            if let Ok(st) = status.parse::<i32>() {
                logs.retain(|l| l.status == st);
            }
        }
    }

    let total = logs.len() as i64;
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);
    let start = ((page - 1) * per_page) as usize;
    let end = (start + per_page as usize).min(logs.len());

    let paginated_data = if start < logs.len() {
        logs[start..end].to_vec()
    } else {
        Vec::new()
    };

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    Ok(PaginatedLogs {
        data: paginated_data,
        total,
        page,
        per_page,
        total_pages,
    })
}

pub async fn recent(store: &InMemoryStore, limit: i64) -> Result<Vec<RequestLog>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut logs: Vec<RequestLog> = inner.request_logs.to_vec();
    logs.reverse();
    let lim = (limit as usize).min(logs.len());
    Ok(logs[..lim].to_vec())
}

/// Clear all request logs from SQLite and the in-memory store.
pub async fn clear(store: &InMemoryStore) -> Result<i64, String> {
    let mut deleted: i64 = 0;
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        deleted = crate::sqlite::clear_all_logs(&conn)?;
    }
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.request_logs.clear();
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(id: &str, agent: &str, provider: &str, model: &str, status: i32) -> RequestLog {
        RequestLog {
            id: id.into(),
            timestamp: id.into(),
            agent: agent.into(),
            provider: provider.into(),
            model: model.into(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status,
            error: if status >= 400 {
                Some("error".into())
            } else {
                None
            },
            path: "/v1/test".into(),
            stream: false,
            request_body: None,
            response_body: None,
        }
    }

    #[tokio::test]
    async fn logs_insert_update_query_filters_pagination_and_recent() {
        let store = InMemoryStore::new();
        insert(&store, &log("1", "codex", "p1", "m1", 200))
            .await
            .unwrap();
        insert(&store, &log("2", "claude", "p2", "m2", 500))
            .await
            .unwrap();
        insert(&store, &log("3", "codex", "p1", "m3", 200))
            .await
            .unwrap();

        let mut updated = log("2", "claude", "p2", "m2", 429);
        updated.total_tokens = 99;
        insert(&store, &updated).await.unwrap();
        assert_eq!(store.inner.read().unwrap().request_logs.len(), 3);

        let page = query(
            &store,
            &LogQuery {
                agent: Some("codex".into()),
                provider: Some("p1".into()),
                model: Some("m3".into()),
                status: Some("200".into()),
                page: Some(1),
                per_page: Some(1),
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.total_pages, 1);
        assert_eq!(page.data[0].id, "3");

        let all = query(
            &store,
            &LogQuery {
                status: Some("not-a-number".into()),
                page: Some(2),
                per_page: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.page, 2);
        assert_eq!(all.per_page, 2);
        assert_eq!(all.total_pages, 2);
        assert_eq!(all.data.len(), 1);
        assert_eq!(all.data[0].id, "1");

        let out_of_range = query(
            &store,
            &LogQuery {
                page: Some(99),
                per_page: Some(10),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(out_of_range.data.is_empty());

        let recent = recent(&store, 2).await.unwrap();
        assert_eq!(
            recent
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["3", "2"]
        );
        assert_eq!(recent[1].total_tokens, 99);
    }
}
