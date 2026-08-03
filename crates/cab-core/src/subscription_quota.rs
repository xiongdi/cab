//! Subscription key quota tracking: parse rate-limit headers and detect recovery windows.

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Default recovery window when upstream does not provide Retry-After.
/// Kept short so a burst 429 does not lock a sole provider for an hour.
pub const DEFAULT_QUOTA_RESET_SECS: i64 = 60;

/// Max 429 retries on the same key/endpoint after the first failure (1 + N attempts).
pub const RATE_LIMIT_MAX_RETRIES: u32 = 4;

/// Base delay for exponential backoff when Retry-After is absent.
pub const RATE_LIMIT_BASE_DELAY_MS: u64 = 500;

/// Cap on exponential backoff delay.
pub const RATE_LIMIT_MAX_DELAY_MS: u64 = 8_000;

/// Honor Retry-After only when the wait is at most this long; otherwise failover.
pub const RATE_LIMIT_MAX_HONOR_RETRY_AFTER_MS: u64 = 15_000;

/// True when the key is still inside a recorded quota recovery window.
pub fn is_key_rate_limited(key: &crate::types::ApiKeyConfig) -> bool {
    key.quota_reset_at
        .as_ref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .is_some_and(|reset_at| reset_at > Utc::now())
}

/// Resolve when a rate-limited key becomes usable again.
pub fn resolve_quota_reset_at(retry_after: Option<DateTime<Utc>>, body: &str) -> DateTime<Utc> {
    retry_after
        .or_else(|| parse_quota_reset_from_body(body))
        .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(DEFAULT_QUOTA_RESET_SECS))
}

/// Delay before the next same-key 429 retry.
///
/// `attempt` is 0-based for the first retry after the initial 429.
/// Returns `None` when Retry-After is longer than [`RATE_LIMIT_MAX_HONOR_RETRY_AFTER_MS`]
/// (caller should cool the key and failover instead of waiting).
pub fn rate_limit_backoff_delay(
    attempt: u32,
    retry_after: Option<DateTime<Utc>>,
) -> Option<Duration> {
    if let Some(reset_at) = retry_after {
        let wait_ms = (reset_at - Utc::now()).num_milliseconds().max(0) as u64;
        if wait_ms > RATE_LIMIT_MAX_HONOR_RETRY_AFTER_MS {
            return None;
        }
        return Some(Duration::from_millis(wait_ms));
    }

    let shift = attempt.min(4);
    let delay = RATE_LIMIT_BASE_DELAY_MS
        .saturating_mul(1u64 << shift)
        .min(RATE_LIMIT_MAX_DELAY_MS);
    Some(Duration::from_millis(delay))
}

/// Parse standard rate-limit headers from an upstream error response.
pub fn extract_retry_after(headers: &axum::http::HeaderMap) -> Option<DateTime<Utc>> {
    for name in [
        "retry-after",
        "x-ratelimit-reset",
        "ratelimit-reset",
        "x-rate-limit-reset",
    ] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok())
            && let Some(dt) = parse_retry_after_value(value)
        {
            return Some(dt);
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
        if let Some(value) = json
            .get(key)
            .or_else(|| json.pointer(&format!("/error/{key}")))
        {
            if let Some(secs) = value.as_u64() {
                return Some(Utc::now() + chrono::Duration::seconds(secs as i64));
            }
            if let Some(raw) = value.as_str()
                && let Some(dt) = parse_retry_after_value(raw)
            {
                return Some(dt);
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
            quota_reset_at: Some((Utc::now() + chrono::Duration::minutes(10)).to_rfc3339()),
        };
        assert!(is_key_rate_limited(&key));
    }

    #[test]
    fn expired_rate_limit_window_is_usable() {
        let key = crate::types::ApiKeyConfig {
            key: "sk-test".into(),
            enabled: true,
            quota_reset_at: Some((Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
        };
        assert!(!is_key_rate_limited(&key));
    }

    #[test]
    fn invalid_or_missing_rate_limit_window_is_usable() {
        for quota_reset_at in [None, Some("not-a-date".to_string())] {
            let key = crate::types::ApiKeyConfig {
                key: "sk-test".into(),
                enabled: true,
                quota_reset_at,
            };
            assert!(!is_key_rate_limited(&key));
        }
    }

    #[test]
    fn parses_retry_after_millis_rfc2822_rfc3339_and_ignores_empty() {
        assert!(parse_retry_after_value("").is_none());
        let millis = Utc::now().timestamp_millis() + 600_000;
        assert!(parse_retry_after_value(&millis.to_string()).unwrap() > Utc::now());

        let rfc2822 = (Utc::now() + chrono::Duration::minutes(10)).to_rfc2822();
        assert!(parse_retry_after_value(&rfc2822).unwrap() > Utc::now());

        let rfc3339 = (Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        assert!(parse_retry_after_value(&rfc3339).unwrap() > Utc::now());
        assert!(parse_retry_after_value("nonsense").is_none());
    }

    #[test]
    fn extract_retry_after_uses_supported_headers_in_order() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-ratelimit-reset", "120".parse().unwrap());
        assert!(extract_retry_after(&headers).unwrap() > Utc::now());

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("retry-after", "".parse().unwrap());
        headers.insert("ratelimit-reset", "120".parse().unwrap());
        assert!(extract_retry_after(&headers).unwrap() > Utc::now());

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-rate-limit-reset", "120".parse().unwrap());
        assert!(extract_retry_after(&headers).unwrap() > Utc::now());

        assert!(extract_retry_after(&axum::http::HeaderMap::new()).is_none());
    }

    #[test]
    fn resolves_quota_reset_from_body_keys_or_default_window() {
        let explicit = Utc::now() + chrono::Duration::minutes(5);
        assert_eq!(
            resolve_quota_reset_at(Some(explicit), r#"{"retry_after": 1}"#),
            explicit
        );

        for body in [
            r#"{"retry_after": 120}"#,
            r#"{"retryAfter": "120"}"#,
            r#"{"error": {"reset_at": 120}}"#,
            r#"{"error": {"resetAt": "120"}}"#,
        ] {
            assert!(resolve_quota_reset_at(None, body) > Utc::now());
        }

        let fallback = resolve_quota_reset_at(None, "not-json");
        let delta = (fallback - Utc::now()).num_seconds();
        assert!((DEFAULT_QUOTA_RESET_SECS - 10..=DEFAULT_QUOTA_RESET_SECS + 10).contains(&delta));
    }

    #[test]
    fn exponential_backoff_sequence_without_retry_after() {
        assert_eq!(
            rate_limit_backoff_delay(0, None),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            rate_limit_backoff_delay(1, None),
            Some(Duration::from_millis(1_000))
        );
        assert_eq!(
            rate_limit_backoff_delay(2, None),
            Some(Duration::from_millis(2_000))
        );
        assert_eq!(
            rate_limit_backoff_delay(3, None),
            Some(Duration::from_millis(4_000))
        );
        assert_eq!(
            rate_limit_backoff_delay(4, None),
            Some(Duration::from_millis(8_000))
        );
        assert_eq!(
            rate_limit_backoff_delay(5, None),
            Some(Duration::from_millis(8_000))
        );
    }

    #[test]
    fn honors_short_retry_after_and_rejects_long() {
        let short = Utc::now() + chrono::Duration::seconds(2);
        let delay = rate_limit_backoff_delay(0, Some(short)).expect("short wait");
        assert!(delay.as_millis() <= 2_500);

        let long = Utc::now() + chrono::Duration::seconds(60);
        assert!(rate_limit_backoff_delay(0, Some(long)).is_none());
    }
}
