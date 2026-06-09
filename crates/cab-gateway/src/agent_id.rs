//! Resolve CAB-supported coding-agent IDs from inbound gateway request headers.

use axum::http::HeaderMap;

const SUPPORTED_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "opencode",
    "hermes",
    "kilocode",
    "openclaw",
    "pi",
];

/// Identify the calling coding agent using explicit CAB headers first, then
/// vendor-specific signals such as `originator` and `User-Agent`.
pub fn extract_agent_id(headers: &HeaderMap) -> String {
    if let Some(value) = header_value(headers, "x-cab-agent") {
        if let Some(id) = normalize_agent_id(&value) {
            return id;
        }
    }

    if let Some(value) = header_value(headers, "originator") {
        if let Some(id) = map_originator(&value) {
            return id;
        }
    }

    if let Some(ua) = header_value(headers, "user-agent") {
        if let Some(id) = map_user_agent(&ua) {
            return id;
        }
    }

    "unknown".to_string()
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn normalize_agent_id(value: &str) -> Option<String> {
    let lower = value.trim().to_ascii_lowercase();
    if SUPPORTED_AGENT_IDS.contains(&lower.as_str()) {
        return Some(lower);
    }

    match lower.as_str() {
        "claude" => Some("claude-code".to_string()),
        "kilo" | "kilo-code" => Some("kilocode".to_string()),
        _ => None,
    }
}

fn map_originator(originator: &str) -> Option<String> {
    let lower = originator.to_ascii_lowercase();
    if lower.contains("codex") {
        return Some("codex".to_string());
    }
    None
}

fn map_user_agent(ua: &str) -> Option<String> {
    let lower = ua.to_ascii_lowercase();

    if lower.contains("kilo-code") || lower.contains("kilocode") {
        return Some("kilocode".to_string());
    }
    if lower.contains("openclaw") {
        return Some("openclaw".to_string());
    }
    if lower.contains("pi-coding-agent") || lower.contains("pi-coding") {
        return Some("pi".to_string());
    }
    if lower.contains("hermesagent") || lower.contains("hermes/") {
        return Some("hermes".to_string());
    }
    if lower.contains("opencode/") || lower.starts_with("opencode") {
        return Some("opencode".to_string());
    }
    if lower.contains("codex") {
        return Some("codex".to_string());
    }
    if lower.contains("claude") {
        return Some("claude-code".to_string());
    }

    // Legacy / non-dashboard agents still routable via custom agent_pattern rules.
    if lower.contains("cursor") {
        return Some("cursor".to_string());
    }
    if lower.contains("copilot") {
        return Some("copilot".to_string());
    }
    if lower.contains("continue") {
        return Some("continue".to_string());
    }
    if lower.contains("cline") {
        return Some("cline".to_string());
    }
    if lower.contains("aider") {
        return Some("aider".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        map
    }

    #[test]
    fn prefers_x_cab_agent_header() {
        let headers = headers_with(&[
            ("user-agent", "OpenAI/JS 6.0"),
            ("x-cab-agent", "openclaw"),
        ]);
        assert_eq!(extract_agent_id(&headers), "openclaw");
    }

    #[test]
    fn maps_codex_originator() {
        let headers = headers_with(&[
            ("user-agent", "OpenAI/JS 6.0"),
            ("originator", "codex_exec"),
        ]);
        assert_eq!(extract_agent_id(&headers), "codex");
    }

    #[test]
    fn maps_default_user_agents_for_supported_cas() {
        let cases = [
            (("user-agent", "claude-cli/2.1.165"), "claude-code"),
            (("user-agent", "codex_exec/0.134.0"), "codex"),
            (("user-agent", "opencode/1.14.48 ai-sdk/5"), "opencode"),
            (("user-agent", "HermesAgent/0.16.0"), "hermes"),
            (
                ("user-agent", "Kilo-Code/7.3.40 ai-sdk/provider-utils/4.0.23"),
                "kilocode",
            ),
            (("user-agent", "OpenClaw/2026.6.1 (cab-probe)"), "openclaw"),
            (("user-agent", "pi-coding-agent/0.79.0"), "pi"),
        ];
        for ((name, value), expected) in cases {
            let headers = headers_with(&[(name, value)]);
            assert_eq!(extract_agent_id(&headers), expected, "ua={value}");
        }
    }

    #[test]
    fn unknown_when_no_signal() {
        let headers = headers_with(&[("user-agent", "OpenAI/JS 6.39.1")]);
        assert_eq!(extract_agent_id(&headers), "unknown");
    }
}
