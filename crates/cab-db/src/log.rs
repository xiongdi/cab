use crate::InMemoryStore;
use cab_core::types::{LogQuery, PaginatedLogs, RequestLog};

pub async fn insert(store: &InMemoryStore, log: &RequestLog) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    // Deduplicate/Update existing log if it has the same ID (e.g. updating stream token count)
    if let Some(pos) = inner.request_logs.iter().position(|l| l.id == log.id) {
        inner.request_logs[pos] = log.clone();
    } else {
        inner.request_logs.push(log.clone());
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
