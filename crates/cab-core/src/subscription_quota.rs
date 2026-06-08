//! Subscription key quota tracking: parse rate-limit headers and detect recovery windows.

use chrono::{DateTime, Utc};

/// Default recovery window when upstream does not provide Retry-After.
pub const DEFAULT_QUOTA_RESET_SECS: i64 = 3600;

/// True when the key is still inside a recorded quota recovery window.
pub fn is_key_rate_limited(key: &crate::types::ApiKeyConfig) -> bool {
    key.quota_reset_at
        .as_ref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .is_some_and(|reset_at| reset_at > Utc::now())
}

/// Resolve when a rate-limited key becomes usable again.
pub fn resolve_quota_reset_at(
    retry_after: Option<DateTime<Utc>>,
    body: &str,
) -> DateTime<Utc> {
    retry_after
        .or_else(|| parse_quota_reset_from_body(body))
        .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(DEFAULT_QUOTA_RESET_SECS))
}

/// Parse standard rate-limit headers from an upstream error response.
pub fn extract_retry_after(headers: &axum::http::HeaderMap) -> Option<DateTime<Utc>> {
    for name in [
        "retry-after",
        "x-ratelimit-reset",
        "ratelimit-reset",
        "x-rate-limit-reset",
    ] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Some(dt) = parse_retry_after_value(value) {
                return Some(dt);
            }
        }
    }
    None
}

fn parse_retry_after_value(value: &str) -> Option<DateTime<Utc>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Utc::now() + chrono::Duration::seconds(secs as i64));
    }

    if let Ok(epoch) = trimmed.parse::<i64>() {
        if epoch > 1_000_000_000_000 {
            return DateTime::from_timestamp_millis(epoch);
        }
        return DateTime::from_timestamp(epoch, 0);
    }

    DateTime::parse_from_rfc2822(trimmed)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
}

fn parse_quota_reset_from_body(body: &str) -> Option<DateTime<Utc>> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    for key in ["retry_after", "retryAfter", "reset_at", "resetAt"] {
        if let Some(value) = json.get(key).or_else(|| json.pointer(&format!("/error/{key}"))) {
            if let Some(secs) = value.as_u64() {
                return Some(Utc::now() + chrono::Duration::seconds(secs as i64));
            }
            if let Some(raw) = value.as_str() {
                if let Some(dt) = parse_retry_after_value(raw) {
                    return Some(dt);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_retry_after_seconds() {
        let dt = parse_retry_after_value("120").expect("parsed");
        let delta = (dt - Utc::now()).num_seconds();
        assert!((110..=130).contains(&delta));
    }

    #[test]
    fn parses_retry_after_unix_timestamp() {
        let epoch = Utc::now().timestamp() + 600;
        let dt = parse_retry_after_value(&epoch.to_string()).expect("parsed");
        assert!(dt > Utc::now());
    }

    #[test]
    fn detects_active_rate_limit_window() {
        let key = crate::types::ApiKeyConfig {
            key: "sk-test".into(),
            enabled: true,
            subscribed: true,
            quota_reset_at: Some((Utc::now() + chrono::Duration::minutes(10)).to_rfc3339()),
        };
        assert!(is_key_rate_limited(&key));
    }

    #[test]
    fn expired_rate_limit_window_is_usable() {
        let key = crate::types::ApiKeyConfig {
            key: "sk-test".into(),
            enabled: true,
            subscribed: true,
            quota_reset_at: Some((Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
        };
        assert!(!is_key_rate_limited(&key));
    }
}
