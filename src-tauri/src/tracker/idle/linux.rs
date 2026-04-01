use std::time::Duration;

pub struct LinuxIdleDetector {
    last_activity: std::time::Instant,
}

impl LinuxIdleDetector {
    pub fn new() -> Self {
        Self {
            last_activity: std::time::Instant::now(),
        }
    }
}

impl super::detector::IdleDetector for LinuxIdleDetector {
    fn get_idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }
    
    fn record_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
    }
}