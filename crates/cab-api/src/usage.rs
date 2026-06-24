use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;
use cab_core::types::{UsageQuery, UsageSummary};
use cab_db::sqlite;
use serde_json::json;

use crate::ApiState;

pub async fn get_summary(
    State(state): State<ApiState>,
    Query(query): Query<UsageQuery>,
) -> impl IntoResponse {
    let Some(pool) = state.pool.sqlite() else {
        return Json(UsageSummary::default()).into_response();
    };
    let range = query.range.as_deref().unwrap_or("month");
    let since = range_to_since(range);
    match pool.get() {
        Ok(conn) => match sqlite::usage_summary(&conn, &since) {
            Ok(summary) => Json(summary).into_response(),
            Err(e) => {
                tracing::error!("Usage summary failed: {e}");
                Json(UsageSummary::default()).into_response()
            }
        },
        Err(e) => {
            tracing::error!("Pool get failed: {e}");
            Json(UsageSummary::default()).into_response()
        }
    }
}

pub async fn get_records(
    State(state): State<ApiState>,
    Query(query): Query<UsageQuery>,
) -> impl IntoResponse {
    let Some(pool) = state.pool.sqlite() else {
        return Json(json!({"data": [], "total": 0})).into_response();
    };
    let range = query.range.as_deref().unwrap_or("month");
    let since = range_to_since(range);
    let per_page = query.per_page.unwrap_or(50);
    match pool.get() {
        Ok(conn) => match sqlite::query_usage(&conn, &since, per_page) {
            Ok(records) => {
                let total = records.len() as i64;
                Json(json!({"data": records, "total": total})).into_response()
            }
            Err(e) => {
                tracing::error!("Usage records query failed: {e}");
                Json(json!({"data": [], "total": 0})).into_response()
            }
        },
        Err(e) => {
            tracing::error!("Pool get failed: {e}");
            Json(json!({"data": [], "total": 0})).into_response()
        }
    }
}

fn range_to_since(range: &str) -> String {
    let now = chrono::Utc::now();
    let cutoff = match range {
        "day" => now - chrono::Duration::hours(24),
        "week" => now - chrono::Duration::days(7),
        "month" => now - chrono::Duration::days(30),
        _ => now - chrono::Duration::days(30),
    };
    cutoff.to_rfc3339()
}
