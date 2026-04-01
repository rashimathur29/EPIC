use std::time::Duration;

pub struct WindowsIdleDetector {
    last_activity: std::time::Instant,
}

impl WindowsIdleDetector {
    pub fn new() -> Self {
        Self {
            last_activity: std::time::Instant::now(),
        }
    }
}

impl super::detector::IdleDetector for WindowsIdleDetector {
    fn get_idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }
    
    fn record_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
    }
}