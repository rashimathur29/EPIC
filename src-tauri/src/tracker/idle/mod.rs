use std::time::Duration;
use user_idle::UserIdle;

pub trait IdleDetector: Send + Sync {
    fn get_idle_time(&self) -> Duration;
    fn record_activity(&mut self);
}

pub struct SystemIdleDetector;

impl IdleDetector for SystemIdleDetector {
    fn get_idle_time(&self) -> Duration {
        match UserIdle::get_time() {
            Ok(user_idle) => user_idle.duration(),   // ← Fixed here
            Err(e) => {
                log::warn!("[IDLE] Failed to get idle time: {:?}", e);
                Duration::from_secs(0)
            }
        }
    }

    fn record_activity(&mut self) {
        // No manual reset needed
    }
}

pub fn create_idle_detector() -> Box<dyn IdleDetector> {
    Box::new(SystemIdleDetector)
}