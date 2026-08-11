use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_FAILURE_THRESHOLD: u32 = 3;
/// After a provider trips the breaker, wait this long before allowing a probe
/// request through again (half-open state). A successful probe resets the
/// counter; another failure re-trips and restarts the cooldown.
const RECOVERY_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug)]
struct ProviderHealth {
    /// Consecutive failures since the last success (or probe pass).
    consecutive_failures: u32,
    /// When the last failure was recorded — drives the half-open recovery.
    last_failure_at: Instant,
}

#[derive(Debug)]
pub struct HealthTracker {
    inner: Mutex<HashMap<String, ProviderHealth>>,
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
            let entry = map
                .entry(provider_id.to_string())
                .or_insert(ProviderHealth {
                    consecutive_failures: 0,
                    last_failure_at: Instant::now(),
                });
            entry.consecutive_failures += 1;
            entry.last_failure_at = Instant::now();
            if entry.consecutive_failures >= self.threshold {
                tracing::warn!(
                    provider_id,
                    consecutive_failures = entry.consecutive_failures,
                    threshold = self.threshold,
                    "provider marked unhealthy"
                );
            }
        }
    }

    /// A provider is healthy while below the failure threshold, or once the
    /// recovery cooldown has elapsed since the last failure (half-open — lets a
    /// probe request through so the provider can recover after an upstream blip).
    pub fn is_healthy(&self, provider_id: &str) -> bool {
        self.inner
            .lock()
            .map(|map| {
                map.get(provider_id)
                    .map(|h| {
                        h.consecutive_failures < self.threshold
                            || h.last_failure_at.elapsed() >= RECOVERY_COOLDOWN
                    })
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

    #[test]
    fn tripped_provider_recovers_after_cooldown() {
        // Override the module-level cooldown to a tiny value so the test doesn't
        // wait 30s: temporarily swap the constant via the timer used internally.
        // HealthTracker uses a fixed cooldown; emulate elapsed time by tripping,
        // then verify the half-open window lets a probe through.
        let tracker = HealthTracker::with_threshold(2);
        tracker.record_failure("p1");
        tracker.record_failure("p1");
        assert!(!tracker.is_healthy("p1"));

        // Force the stored last_failure_at into the past so the cooldown reads
        // as expired (half-open). Accessing the private map directly is fine in
        // the unit test module.
        if let Ok(mut map) = tracker.inner.lock()
            && let Some(h) = map.get_mut("p1")
        {
            h.last_failure_at = Instant::now() - std::time::Duration::from_secs(31);
        }
        assert!(tracker.is_healthy("p1"), "cooldown elapsed → probe allowed");

        // A successful probe resets the breaker entirely.
        tracker.record_success("p1");
        assert!(tracker.is_healthy("p1"));

        // A failed probe re-trips and the cooldown restarts (still unhealthy
        // right away).
        tracker.record_failure("p1");
        tracker.record_failure("p1");
        assert!(!tracker.is_healthy("p1"));
    }
}
