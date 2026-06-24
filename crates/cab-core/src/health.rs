use std::collections::HashMap;
use std::sync::Mutex;

const DEFAULT_FAILURE_THRESHOLD: u32 = 3;

#[derive(Debug)]
pub struct HealthTracker {
    inner: Mutex<HashMap<String, u32>>,
    threshold: u32,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self::with_threshold(DEFAULT_FAILURE_THRESHOLD)
    }

    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            threshold: threshold.max(1),
        }
    }

    pub fn record_success(&self, provider_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(provider_id);
        }
    }

    pub fn record_failure(&self, provider_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            let count = map.entry(provider_id.to_string()).or_insert(0);
            *count += 1;
            if *count >= self.threshold {
                tracing::warn!(
                    provider_id,
                    consecutive_failures = *count,
                    threshold = self.threshold,
                    "provider marked unhealthy"
                );
            }
        }
    }

    pub fn is_healthy(&self, provider_id: &str) -> bool {
        self.inner
            .lock()
            .map(|map| {
                map.get(provider_id)
                    .map(|c| *c < self.threshold)
                    .unwrap_or(true)
            })
            .unwrap_or(true)
    }
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_until_threshold_reached() {
        let tracker = HealthTracker::with_threshold(3);
        assert!(tracker.is_healthy("p1"));
        tracker.record_failure("p1");
        assert!(tracker.is_healthy("p1"));
        tracker.record_failure("p1");
        assert!(tracker.is_healthy("p1"));
        tracker.record_failure("p1");
        assert!(!tracker.is_healthy("p1"));
    }

    #[test]
    fn success_resets_counter() {
        let tracker = HealthTracker::with_threshold(2);
        tracker.record_failure("p1");
        tracker.record_failure("p1");
        assert!(!tracker.is_healthy("p1"));
        tracker.record_success("p1");
        assert!(tracker.is_healthy("p1"));
    }

    #[test]
    fn unknown_provider_is_healthy() {
        let tracker = HealthTracker::new();
        assert!(tracker.is_healthy("unknown"));
    }

    #[test]
    fn providers_tracked_independently() {
        let tracker = HealthTracker::with_threshold(2);
        tracker.record_failure("p1");
        tracker.record_failure("p1");
        assert!(!tracker.is_healthy("p1"));
        assert!(tracker.is_healthy("p2"));
    }
}
