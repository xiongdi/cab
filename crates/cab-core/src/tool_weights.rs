//! Per-tool schema token-cost diagnostics.
//!
//! Tool JSON schemas sit in the cacheable request prefix and can dominate input
//! token cost for agentic workloads, yet their weight is invisible to users.
//! This module estimates each tool's token footprint and keeps the most recent
//! snapshot per agent so the dashboard can surface "which tool schemas are heavy"
//! — mirroring Reasonix's `SchemaTokenCosts`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Rough token estimate from byte length (~4 chars per token for code-heavy
/// JSON). A real tokenizer would be more precise, but a byte heuristic is
/// sufficient for relative-weight diagnostics and is allocation-free.
pub fn estimate_tokens(byte_len: usize) -> i64 {
    if byte_len == 0 {
        0
    } else {
        (byte_len / 4) as i64
    }
}

/// Estimated token cost of a single tool definition.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolSchemaCost {
    pub name: String,
    pub tokens: i64,
}

/// Best-effort tool name: Anthropic uses `tool.name`, OpenAI uses
/// `tool.function.name`; falls back to a positional label.
fn tool_name(tool: &serde_json::Value, index: usize) -> String {
    tool.get("name")
        .and_then(|v| v.as_str())
        .or_else(|| {
            tool.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("tool[{index}]"))
}

/// Estimate per-tool token costs from a request body's `tools` array.
pub fn tool_schema_costs(body: &serde_json::Value) -> Vec<ToolSchemaCost> {
    let Some(tools) = body.get("tools").and_then(|t| t.as_array()) else {
        return Vec::new();
    };
    tools
        .iter()
        .enumerate()
        .map(|(i, tool)| ToolSchemaCost {
            name: tool_name(tool, i),
            tokens: estimate_tokens(tool.to_string().len()),
        })
        .collect()
}

/// The most recent tool-schema weights observed for one agent.
#[derive(Debug, Clone, Serialize)]
pub struct ToolWeightSnapshot {
    pub agent: String,
    pub captured_at_ms: i64,
    pub total_tokens: i64,
    pub tool_count: usize,
    /// Tools sorted by descending token cost (heaviest first).
    pub tools: Vec<ToolSchemaCost>,
}

/// In-memory store of the latest [`ToolWeightSnapshot`] per agent.
#[derive(Debug, Default)]
pub struct ToolWeightTracker {
    inner: Mutex<HashMap<String, ToolWeightSnapshot>>,
}

impl ToolWeightTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the latest tool weights for `agent`. No-op when there are no
    /// tools. Tools are stored heaviest-first so the costly schemas surface.
    pub fn record(&self, agent: &str, mut tools: Vec<ToolSchemaCost>) {
        if tools.is_empty() {
            return;
        }
        tools.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
        let total_tokens = tools.iter().map(|t| t.tokens).sum();
        let tool_count = tools.len();
        let snapshot = ToolWeightSnapshot {
            agent: agent.to_string(),
            captured_at_ms: now_ms(),
            total_tokens,
            tool_count,
            tools,
        };
        if let Ok(mut map) = self.inner.lock() {
            map.insert(agent.to_string(), snapshot);
        }
    }

    /// All recorded snapshots, most-recently-captured first.
    pub fn snapshot(&self) -> Vec<ToolWeightSnapshot> {
        let Ok(map) = self.inner.lock() else {
            return Vec::new();
        };
        let mut out: Vec<_> = map.values().cloned().collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.captured_at_ms));
        out
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn costs_handle_anthropic_and_openai_shapes() {
        let body = json!({
            "tools": [
                { "name": "alpha", "input_schema": { "type": "object" } },
                { "type": "function", "function": { "name": "beta" } },
                { "description": "no name" }
            ]
        });
        let costs = tool_schema_costs(&body);
        assert_eq!(costs.len(), 3);
        assert_eq!(costs[0].name, "alpha");
        assert_eq!(costs[1].name, "beta");
        assert_eq!(costs[2].name, "tool[2]");
        assert!(costs.iter().all(|c| c.tokens >= 0));
    }

    #[test]
    fn no_tools_is_empty() {
        assert!(tool_schema_costs(&json!({ "messages": [] })).is_empty());
    }

    #[test]
    fn record_sorts_heaviest_first_and_totals() {
        let tracker = ToolWeightTracker::new();
        tracker.record(
            "claude-code",
            vec![
                ToolSchemaCost {
                    name: "light".into(),
                    tokens: 10,
                },
                ToolSchemaCost {
                    name: "heavy".into(),
                    tokens: 100,
                },
            ],
        );
        let snaps = tracker.snapshot();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].total_tokens, 110);
        assert_eq!(snaps[0].tool_count, 2);
        assert_eq!(snaps[0].tools[0].name, "heavy");
    }

    #[test]
    fn record_empty_is_noop() {
        let tracker = ToolWeightTracker::new();
        tracker.record("a", Vec::new());
        assert!(tracker.snapshot().is_empty());
    }
}
