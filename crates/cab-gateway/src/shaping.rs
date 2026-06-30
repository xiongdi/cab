//! Request-body shaping for upstream prefix-cache friendliness.
//!
//! Upstreams cache request *prefixes* and bill cache hits at a steep discount,
//! but the cache only hits when the prefix is byte-stable across turns. CAB
//! already re-serializes bodies during forwarding (which canonicalizes object
//! key order via `serde_json`), so this module adds the two remaining
//! stabilizations that clients commonly get wrong:
//!
//! 1. **Deterministic tool ordering** — tool schemas reordered between turns
//!    (common with some agent CLIs) break the cache; we sort them by name.
//! 2. **Anthropic `cache_control` breakpoints** — Anthropic only caches when the
//!    request explicitly marks breakpoints. When forwarding to an Anthropic
//!    endpoint and the client did *not* set any, we mark the end of the static
//!    prefix (tools + system) so it becomes cacheable.
//!
//! All shaping is gated behind the `cache_request_shaping_enabled` setting and
//! is intentionally conservative: it never changes request semantics, only the
//! ordering/annotation of the cacheable prefix.

use serde_json::Value;

/// Apply cache-friendly shaping to a request body destined for `endpoint_protocol`.
pub fn shape_request(body: &mut Value, endpoint_protocol: &str) {
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        normalize_system_messages(messages);
        if endpoint_protocol == "openai-chat" || endpoint_protocol == "openai-responses" {
            realign_openai_system_prompt(messages);
        }
    }
    sort_tools(body);
    if endpoint_protocol == "anthropic" {
        inject_anthropic_cache_control(body);
    }
}

fn normalize_system_messages(messages: &mut Vec<Value>) {
    messages.retain(|msg| {
        let is_target = msg.get("role").and_then(Value::as_str) == Some("system")
            && msg
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|c| c.contains("x-anthropic-billing-header"));
        !is_target
    });
}

fn extract_dynamic_parts(content: &str) -> (String, Option<String>) {
    let git_idx = content.find("gitStatus:");
    let date_idx = content.find("# currentDate");

    match (git_idx, date_idx) {
        (Some(g), Some(d)) => {
            let first = std::cmp::min(g, d);
            let static_part = content[..first].trim().to_string();
            let dynamic_part = content[first..].trim().to_string();
            (static_part, Some(dynamic_part))
        }
        (Some(g), None) => {
            let static_part = content[..g].trim().to_string();
            let dynamic_part = content[g..].trim().to_string();
            (static_part, Some(dynamic_part))
        }
        (None, Some(d)) => {
            let static_part = content[..d].trim().to_string();
            let dynamic_part = content[d..].trim().to_string();
            (static_part, Some(dynamic_part))
        }
        (None, None) => (content.to_string(), None),
    }
}

fn realign_openai_system_prompt(messages: &mut Vec<Value>) {
    let mut target_idx = None;
    let mut max_len = 0;

    for (i, msg) in messages.iter().enumerate() {
        if msg.get("role").and_then(Value::as_str) == Some("system") {
            let len = msg
                .get("content")
                .and_then(Value::as_str)
                .map_or(0, |c| c.len());
            if len > max_len {
                max_len = len;
                target_idx = Some(i);
            }
        }
    }

    if let Some(idx) = target_idx {
        let content = messages[idx]["content"].as_str().unwrap().to_string();
        let (static_part, dynamic_part) = extract_dynamic_parts(&content);
        if let Some(dyn_part) = dynamic_part {
            messages[idx]["content"] = Value::String(static_part);
            let dyn_message = serde_json::json!({
                "role": "system",
                "content": dyn_part
            });
            messages.push(dyn_message);
        }
    }
}

/// Best-effort tool name for sorting: Anthropic uses `tool.name`, OpenAI uses
/// `tool.function.name`.
fn tool_name(tool: &Value) -> &str {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            tool.get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
}

/// Sort the top-level `tools` array deterministically: primarily by tool name,
/// then by the tool's full serialized form as a tiebreak. Tool definition order
/// carries no semantic meaning for either Anthropic or OpenAI, so this is safe
/// and makes the prefix stable regardless of client ordering. The full-form
/// tiebreak keeps ordering stable even for same-named or unnamed tools (e.g.
/// two tools that differ only in their schema), matching Reasonix's
/// name → description → parameters total ordering.
fn sort_tools(body: &mut Value) {
    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        tools.sort_by(|a, b| {
            tool_name(a)
                .cmp(tool_name(b))
                .then_with(|| a.to_string().cmp(&b.to_string()))
        });
    }
}

/// Returns true if the body already carries any `cache_control` marker, in which
/// case the client (e.g. Claude Code) is managing breakpoints itself and we must
/// not interfere (Anthropic caps breakpoints at 4).
fn has_cache_control(body: &Value) -> bool {
    fn walk(v: &Value) -> bool {
        match v {
            Value::Object(map) => map.contains_key("cache_control") || map.values().any(walk),
            Value::Array(arr) => arr.iter().any(walk),
            _ => false,
        }
    }
    // Only the prefix regions can legitimately carry breakpoints; restricting the
    // scan there avoids false positives from user text that mentions the term.
    body.get("tools").map(walk).unwrap_or(false) || body.get("system").map(walk).unwrap_or(false)
}

fn ephemeral() -> Value {
    serde_json::json!({ "type": "ephemeral" })
}

/// Add `cache_control` breakpoints to the static prefix (tools + system) of an
/// Anthropic request when the client set none. This caches the largest stable
/// region (tool schemas and system prompt) without touching the conversation,
/// staying well under Anthropic's 4-breakpoint limit.
fn inject_anthropic_cache_control(body: &mut Value) {
    if !body.is_object() || has_cache_control(body) {
        return;
    }

    // Breakpoint at the end of the tools array.
    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut)
        && let Some(obj) = tools.last_mut().and_then(Value::as_object_mut)
    {
        obj.insert("cache_control".to_string(), ephemeral());
    }

    // Breakpoint at the end of the system prompt. Anthropic accepts `system` as a
    // string or an array of text blocks; normalize a string to a single block so
    // we can attach the marker.
    match body.get_mut("system") {
        Some(Value::String(text)) => {
            let block = serde_json::json!({
                "type": "text",
                "text": std::mem::take(text),
                "cache_control": ephemeral(),
            });
            if let Some(obj) = body.as_object_mut() {
                obj.insert("system".to_string(), Value::Array(vec![block]));
            }
        }
        Some(Value::Array(blocks)) => {
            if let Some(Value::Object(last)) = blocks.last_mut() {
                last.insert("cache_control".to_string(), ephemeral());
            }
        }
        _ => {}
    }
}

/// Free function used by tests to inspect a tool list's order.
#[cfg(test)]
fn tool_names(body: &Value) -> Vec<String> {
    body.get("tools")
        .and_then(Value::as_array)
        .map(|tools| tools.iter().map(|t| tool_name(t).to_string()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_anthropic_tools_by_name() {
        let mut body = json!({
            "tools": [{ "name": "zebra" }, { "name": "alpha" }, { "name": "mango" }]
        });
        sort_tools(&mut body);
        assert_eq!(tool_names(&body), vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn sorts_same_named_tools_by_full_form() {
        // Two tools share a name; order must still be deterministic via the
        // full-form tiebreak (here the input_schema string).
        let mut body = json!({
            "tools": [
                { "name": "run", "input_schema": { "z": 1 } },
                { "name": "run", "input_schema": { "a": 1 } }
            ]
        });
        sort_tools(&mut body);
        let tools = body["tools"].as_array().unwrap();
        // serde canonicalizes keys, so `{"a":1}` sorts before `{"z":1}`.
        assert_eq!(tools[0]["input_schema"]["a"], 1);
        assert_eq!(tools[1]["input_schema"]["z"], 1);
    }

    #[test]
    fn sorts_openai_tools_by_function_name() {
        let mut body = json!({
            "tools": [
                { "type": "function", "function": { "name": "b" } },
                { "type": "function", "function": { "name": "a" } }
            ]
        });
        sort_tools(&mut body);
        assert_eq!(tool_names(&body), vec!["a", "b"]);
    }

    #[test]
    fn injects_cache_control_on_tools_and_system_string() {
        let mut body = json!({
            "system": "you are helpful",
            "tools": [{ "name": "a" }, { "name": "b" }]
        });
        inject_anthropic_cache_control(&mut body);

        let tools = body["tools"].as_array().unwrap();
        assert!(tools[1].get("cache_control").is_some());
        assert!(tools[0].get("cache_control").is_none());

        let system = body["system"].as_array().unwrap();
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "you are helpful");
        assert!(system[0].get("cache_control").is_some());
    }

    #[test]
    fn injects_on_system_array_last_block() {
        let mut body = json!({
            "system": [
                { "type": "text", "text": "a" },
                { "type": "text", "text": "b" }
            ]
        });
        inject_anthropic_cache_control(&mut body);
        let system = body["system"].as_array().unwrap();
        assert!(system[0].get("cache_control").is_none());
        assert!(system[1].get("cache_control").is_some());
    }

    #[test]
    fn skips_when_client_already_set_cache_control() {
        let mut body = json!({
            "system": [
                { "type": "text", "text": "a", "cache_control": { "type": "ephemeral" } }
            ],
            "tools": [{ "name": "a" }]
        });
        let before = body.clone();
        inject_anthropic_cache_control(&mut body);
        assert_eq!(
            body, before,
            "must not touch a body that already has breakpoints"
        );
    }

    #[test]
    fn shape_request_non_anthropic_skips_cache_control() {
        let mut body = json!({
            "system": "hi",
            "tools": [{ "name": "b" }, { "name": "a" }]
        });
        shape_request(&mut body, "openai-chat");
        assert_eq!(tool_names(&body), vec!["a", "b"]);
        // system stays a plain string for non-anthropic endpoints.
        assert!(body["system"].is_string());
    }

    #[test]
    fn test_normalize_system_messages() {
        let mut body = json!({
            "messages": [
                { "role": "system", "content": "x-anthropic-billing-header: cc_version=2.1.141.b75; cc_entrypoint=sdk-cli; cch=835ab;" },
                { "role": "system", "content": "You are a Claude agent." },
                { "role": "user", "content": "Hello" }
            ]
        });

        let messages = body["messages"].as_array_mut().unwrap();
        normalize_system_messages(messages);

        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["content"], "You are a Claude agent.");
        assert_eq!(body["messages"][1]["content"], "Hello");
    }

    #[test]
    fn test_realign_openai_system_prompt() {
        let mut messages = vec![
            json!({
                "role": "system",
                "content": "You are a helpful assistant.\n\ngitStatus: modified files\n\n# currentDate\nToday's date is 2026/06/29."
            }),
            json!({
                "role": "user",
                "content": "Hello"
            }),
        ];

        realign_openai_system_prompt(&mut messages);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
        assert_eq!(messages[1]["content"], "Hello");
        assert_eq!(messages[2]["role"], "system");
        assert_eq!(
            messages[2]["content"],
            "gitStatus: modified files\n\n# currentDate\nToday's date is 2026/06/29."
        );
    }
}
