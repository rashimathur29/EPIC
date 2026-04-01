use std::time::{Duration, Instant};

pub struct FallbackIdleDetector {
    last_activity: Instant,
}

impl FallbackIdleDetector {
    pub fn new() -> Self {
        Self {
            last_activity: Instant::now(),
        }
    }
}

impl super::detector::IdleDetector for FallbackIdleDetector {
    fn get_idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }
    
    fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }
}