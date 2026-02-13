//! Terminal stability detection - determines when content has settled

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Tracks terminal content stability
#[derive(Debug, Clone)]
pub struct StabilityState {
    /// Content fingerprint (hash)
    pub fingerprint: u64,
    /// Consecutive stable checks
    pub stable_count: u32,
    /// Last check timestamp
    pub last_check: std::time::Instant,
}

impl Default for StabilityState {
    fn default() -> Self {
        Self {
            fingerprint: 0,
            stable_count: 0,
            last_check: std::time::Instant::now(),
        }
    }
}

/// Detects when terminal content has stabilized
pub struct StabilityDetector {
    states: HashMap<String, StabilityState>,
    threshold: u32,
}

impl StabilityDetector {
    pub fn new(threshold: u32) -> Self {
        Self {
            states: HashMap::new(),
            threshold,
        }
    }

    /// Check if content is stable
    pub fn is_stable(&mut self, agent_id: &str, content: &str) -> bool {
        let fingerprint = Self::hash_content(content);
        let state = self.states.entry(agent_id.to_string()).or_default();

        if state.fingerprint == fingerprint {
            state.stable_count += 1;
        } else {
            state.fingerprint = fingerprint;
            state.stable_count = 1;
        }
        state.last_check = std::time::Instant::now();

        state.stable_count >= self.threshold
    }

    /// Clear state for an agent
    pub fn clear(&mut self, agent_id: &str) {
        self.states.remove(agent_id);
    }

    fn hash_content(content: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}
