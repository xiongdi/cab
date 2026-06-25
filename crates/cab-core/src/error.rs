use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum CabError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Store error: {0}")]
    Database(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Provider error (status {status}): {body}")]
    ProviderError {
        status: u16,
        body: String,
        retry_after: Option<chrono::DateTime<chrono::Utc>>,
    },

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Unauthorized")]
    Unauthorized,
}

/// Mask secrets (API keys, bearer tokens) that upstream providers sometimes echo
/// back in their error bodies, so they are never surfaced to gateway clients or
/// persisted in request logs.
///
/// This is a best-effort, dependency-free scrubber. It masks:
/// 1. values that follow an auth-related keyword (`api key`, `authorization`,
///    `bearer`, `token`, `secret`, ...), and
/// 2. standalone tokens that begin with a well-known key prefix (`sk-`, `xai-`, ...).
pub fn redact_secrets(input: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "api key",
        "api_key",
        "api-key",
        "apikey",
        "x-api-key",
        "authorization",
        "bearer",
        "secret",
        "token",
    ];
    const PREFIXES: &[&str] = &["sk-", "sk_", "xai-", "gsk_", "ghp_", "aiza", "pk-", "rk-"];

    fn is_value_delim(b: u8) -> bool {
        matches!(
            b,
            b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b'}' | b')' | b']' | b';' | b'<'
        )
    }

    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let lower_bytes = lower.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    // 1) Keyword-driven: mask the value immediately following an auth keyword.
    for kw in KEYWORDS {
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(kw) {
            let kw_start = from + rel;
            let kw_end = kw_start + kw.len();
            from = kw_end;

            // Require word boundaries so substrings (e.g. "secret" inside
            // "LONGSECRET", "token" inside "tokens") are not treated as keys.
            let boundary_before =
                kw_start == 0 || !lower_bytes[kw_start - 1].is_ascii_alphanumeric();
            let boundary_after =
                kw_end >= lower_bytes.len() || !lower_bytes[kw_end].is_ascii_alphanumeric();
            if !boundary_before || !boundary_after {
                continue;
            }

            let mut i = kw_end;
            while i < bytes.len()
                && matches!(bytes[i], b' ' | b'\t' | b':' | b'=' | b'"' | b'\'' | b'(')
            {
                i += 1;
            }
            let val_start = i;
            while i < bytes.len() && !is_value_delim(bytes[i]) {
                i += 1;
            }
            if i > val_start {
                ranges.push((val_start, i));
            }
        }
    }

    // 2) Prefix-driven: mask tokens that start with a known key prefix.
    let mut i = 0usize;
    while i < bytes.len() {
        if is_value_delim(bytes[i]) || matches!(bytes[i], b':' | b'=' | b'(') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && !is_value_delim(bytes[i]) {
            i += 1;
        }
        let token = &lower_bytes[start..i];
        if token.len() >= 8 && PREFIXES.iter().any(|p| token.starts_with(p.as_bytes())) {
            ranges.push((start, i));
        }
    }

    if ranges.is_empty() {
        return input.to_string();
    }

    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges {
        if let Some(last) = merged.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
        } else {
            merged.push((s, e));
        }
    }

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    for (s, e) in merged {
        out.push_str(&input[cursor..s]);
        let secret = &input[s..e];
        if secret.chars().count() > 4 {
            let keep: String = secret.chars().skip(secret.chars().count() - 4).collect();
            out.push_str("***");
            out.push_str(&keep);
        } else {
            out.push_str("***");
        }
        cursor = e;
    }
    out.push_str(&input[cursor..]);
    out
}

impl IntoResponse for CabError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            CabError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            CabError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            CabError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            CabError::Proxy(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            CabError::ProviderError {
                status,
                body,
                retry_after: _,
            } => {
                let code = StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY);
                (code, redact_secrets(body))
            }
            CabError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            CabError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
        };

        let body = json!({
            "error": {
                "message": message,
                "type": format!("{:?}", status),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

/// Convenience Result alias used across the crate ecosystem.
pub type CabResult<T> = Result<T, CabError>;

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn masks_keyword_values_keeping_suffix() {
        let out = redact_secrets("Your api key: sk-abc123456789 is invalid");
        assert!(!out.contains("sk-abc123456789"), "got: {out}");
        assert!(out.contains("6789"), "should keep last 4: {out}");
        assert!(out.contains("is invalid"));
    }

    #[test]
    fn masks_bearer_and_json_token() {
        let out = redact_secrets(r#"{"error":"bad authorization","token":"xai-SECRETVALUE99"}"#);
        assert!(!out.contains("xai-SECRETVALUE99"), "got: {out}");
        let out2 = redact_secrets("Authorization: Bearer abcdef*secret*1234");
        assert!(!out2.contains("abcdefsecret"), "got: {out2}");
    }

    #[test]
    fn masks_prefixed_token_anywhere() {
        let out = redact_secrets("upstream rejected sk-ant-api03-LONGSECRET for region us");
        assert!(!out.contains("LONGSECRET"), "got: {out}");
        assert!(out.contains("for region us"));
    }

    #[test]
    fn leaves_clean_text_untouched() {
        let msg = "model not found for provider deepseek";
        assert_eq!(redact_secrets(msg), msg);
    }
}
