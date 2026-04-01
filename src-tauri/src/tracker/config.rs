use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default threshold for logging long idle as "unavailability" break
pub const IDLE_BREAK_THRESHOLD: Duration = Duration::from_secs(120); // 5 minutes

/// Default threshold for considering user "idle" (no activity)
pub const IDLE_ACTIVITY_THRESHOLD: Duration = Duration::from_secs(6); // 6 seconds

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
    
    #[serde(default = "default_summary_window")]
    pub summary_window_minutes: usize,
}

fn default_flush_interval() -> u64 { 60 }
fn default_summary_window() -> usize { 10 }

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: default_flush_interval(),
            summary_window_minutes: default_summary_window(),
        }
    }
}