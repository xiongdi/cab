//! Session-sticky routing for upstream prefix-cache stability.
//!
//! Upstreams (Anthropic / OpenAI / DeepSeek) bill cached input tokens at a
//! steep discount, but the cache only hits when consecutive requests share the
//! same provider *and* a byte-stable prefix. CAB forwards request bodies
//! unchanged, so the prefix is preserved — the remaining risk is the router
//! moving a live conversation to a *different* provider/model between turns
//! (re-scoring as the request profile grows, a key hitting its rate limit,
//! etc.), which cold-starts the upstream cache every time.
//!
//! [`SessionAffinity`] pins a conversation (identified by [`session_key`]) to
//! the provider+model it first resolved to, so later turns keep hitting the
//! same warm cache until the pin expires or the pinned target becomes
//! unusable.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a session stays pinned after its last request.
pub const DEFAULT_TTL_SECS: u64 = 30 * 60;
/// Maximum number of distinct sessions tracked at once (oldest evicted first).
pub const DEFAULT_CAPACITY: usize = 1024;

/// The provider+model a session is pinned to.
#[derive(Debug, Clone)]
pub struct SessionPin {
    pub provider_id: String,
    pub model_name: String,
    last_seen: Instant,
}

/// In-memory, TTL- and capacity-bounded map of session fingerprint → pinned route.
#[derive(Debug)]
pub struct SessionAffinity {
    inner: Mutex<HashMap<u64, SessionPin>>,
    ttl: Duration,
    capacity: usize,
}

impl SessionAffinity {
    pub fn new() -> Self {
        Self::with_config(DEFAULT_TTL_SECS, DEFAULT_CAPACITY)
    }

    pub fn with_config(ttl_secs: u64, capacity: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs.max(1)),
            capacity: capacity.max(1),
        }
    }

    /// Return the still-valid pin for `key`, refreshing its activity timestamp.
    /// Expired pins are dropped lazily and reported as a miss.
    pub fn get(&self, key: u64) -> Option<SessionPin> {
        let mut map = self.inner.lock().ok()?;
        let now = Instant::now();
        if let Some(pin) = map.get(&key)
            && now.duration_since(pin.last_seen) > self.ttl
        {
            map.remove(&key);
            return None;
        }
        let pin = map.get_mut(&key)?;
        pin.last_seen = now;
        Some(pin.clone())
    }

    /// Pin `key` to `provider_id`+`model_name`, evicting expired and overflow entries.
    pub fn set(&self, key: u64, provider_id: String, model_name: String) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        let now = Instant::now();
        map.retain(|_, pin| now.duration_since(pin.last_seen) <= self.ttl);
        if map.len() >= self.capacity
            && !map.contains_key(&key)
            && let Some(oldest) = map
                .iter()
                .min_by_key(|(_, pin)| pin.last_seen)
                .map(|(k, _)| *k)
        {
            map.remove(&oldest);
        }
        map.insert(
            key,
            SessionPin {
                provider_id,
                model_name,
                last_seen: now,
            },
        );
    }

    /// Forget a single session (e.g. after the pinned target turns out unusable).
    pub fn forget(&self, key: u64) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&key);
        }
    }
}

impl Default for SessionAffinity {
    fn default() -> Self {
        Self::new()
    }
}

/// Hashes of the request's cache-bearing prefix regions (the system prompt and
/// the tool schemas). Comparing these across turns of one session attributes a
/// cache miss to the exact region that changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefixShape {
    pub system_hash: u64,
    pub tools_hash: u64,
}

/// Compute the prefix shape of a request body. `tools` are hashed in a
/// deterministic, order-independent way so that mere reordering by the client
/// is not reported as a change (it would not break the cache once shaping
/// sorts them).
pub fn prefix_shape(body: &serde_json::Value) -> PrefixShape {
    let mut system_hasher = std::collections::hash_map::DefaultHasher::new();
    if let Some(system) = body.get("system") {
        system.to_string().hash(&mut system_hasher);
    }
    // `instructions` is the openai-responses equivalent of `system`.
    if let Some(instructions) = body.get("instructions") {
        "\0instructions\0".hash(&mut system_hasher);
        instructions.to_string().hash(&mut system_hasher);
    }

    let mut tools_hash: u64 = 0;
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        // XOR of per-tool hashes → independent of array order.
        for tool in tools {
            let mut tool_hasher = std::collections::hash_map::DefaultHasher::new();
            tool.to_string().hash(&mut tool_hasher);
            tools_hash ^= tool_hasher.finish();
        }
    }

    PrefixShape {
        system_hash: system_hasher.finish(),
        tools_hash,
    }
}

/// Tracks the last-seen [`PrefixShape`] per session so the gateway can log *why*
/// a prompt-cache miss likely happened (system vs. tools changed).
#[derive(Debug)]
pub struct PrefixShapeTracker {
    inner: Mutex<HashMap<u64, PrefixShape>>,
    capacity: usize,
}

impl PrefixShapeTracker {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            capacity: capacity.max(1),
        }
    }

    /// Record the current shape for `key`; return the regions that changed since
    /// the previous turn (empty on first sight or when nothing changed).
    pub fn record(&self, key: u64, shape: PrefixShape) -> Vec<&'static str> {
        let Ok(mut map) = self.inner.lock() else {
            return Vec::new();
        };
        let mut reasons = Vec::new();
        if let Some(prev) = map.get(&key) {
            if prev.system_hash != shape.system_hash {
                reasons.push("system");
            }
            if prev.tools_hash != shape.tools_hash {
                reasons.push("tools");
            }
        }
        if map.len() >= self.capacity && !map.contains_key(&key) {
            // Cheap bound: clear when full. Diagnostics tolerate occasional resets.
            map.clear();
        }
        map.insert(key, shape);
        reasons
    }
}

impl Default for PrefixShapeTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a stable session fingerprint from a request body's cacheable prefix.
///
/// Hashes only the region that stays byte-stable across every turn of a
/// conversation — the agent id, the system prompt, and the first message /
/// `instructions` field. Later turns append to the log but never rewrite this
/// prefix, so the same conversation yields the same key from turn one onward.
///
/// Returns `None` when there is no usable prefix to key on, so callers skip
/// affinity rather than collapsing unrelated requests onto one pin.
pub fn session_key(agent: &str, body: &serde_json::Value) -> Option<u64> {
    let mut parts: Vec<String> = Vec::new();

    // Anthropic Messages + OpenAI Responses share a top-level `system`.
    if let Some(system) = body.get("system") {
        parts.push(system.to_string());
    }
    // OpenAI Responses API uses `instructions` for the system-equivalent prefix.
    if let Some(instructions) = body.get("instructions") {
        parts.push(instructions.to_string());
    }
    // First message is stable across turns; later messages grow and must be excluded.
    if let Some(first) = body
        .get("messages")
        .and_then(|m| m.as_array())
        .and_then(|m| m.first())
    {
        parts.push(first.to_string());
    }

    if parts.is_empty() {
        return None;
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    agent.hash(&mut hasher);
    for part in &parts {
        part.hash(&mut hasher);
    }
    Some(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_is_stable_across_growing_conversation() {
        let turn1 = json!({
            "system": "you are helpful",
            "messages": [{"role": "user", "content": "hello"}]
        });
        let turn2 = json!({
            "system": "you are helpful",
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi"},
                {"role": "user", "content": "next"}
            ]
        });
        assert_eq!(session_key("codex", &turn1), session_key("codex", &turn2));
    }

    #[test]
    fn key_differs_by_agent_and_prefix() {
        let body = json!({"messages": [{"role": "user", "content": "hello"}]});
        let other = json!({"messages": [{"role": "user", "content": "different"}]});
        assert_ne!(session_key("codex", &body), session_key("claude", &body));
        assert_ne!(session_key("codex", &body), session_key("codex", &other));
    }

    #[test]
    fn empty_body_has_no_key() {
        assert!(session_key("codex", &json!({})).is_none());
    }

    #[test]
    fn get_set_and_expiry() {
        let aff = SessionAffinity::with_config(DEFAULT_TTL_SECS, DEFAULT_CAPACITY);
        assert!(aff.get(1).is_none());
        aff.set(1, "p1".into(), "p1/model".into());
        let pin = aff.get(1).expect("pin present");
        assert_eq!(pin.provider_id, "p1");
        assert_eq!(pin.model_name, "p1/model");

        let expired = SessionAffinity::with_config(1, DEFAULT_CAPACITY);
        expired.set(2, "p2".into(), "p2/model".into());
        // Force the stored pin to look stale, then confirm it is dropped.
        {
            let mut map = expired.inner.lock().unwrap();
            map.get_mut(&2).unwrap().last_seen = Instant::now() - Duration::from_secs(10);
        }
        assert!(expired.get(2).is_none());
    }

    #[test]
    fn capacity_evicts_oldest() {
        let aff = SessionAffinity::with_config(DEFAULT_TTL_SECS, 2);
        aff.set(1, "p1".into(), "m1".into());
        std::thread::sleep(Duration::from_millis(5));
        aff.set(2, "p2".into(), "m2".into());
        std::thread::sleep(Duration::from_millis(5));
        aff.set(3, "p3".into(), "m3".into());
        // Oldest (key 1) evicted once capacity exceeded.
        assert!(aff.get(1).is_none());
        assert!(aff.get(2).is_some());
        assert!(aff.get(3).is_some());
    }
}
