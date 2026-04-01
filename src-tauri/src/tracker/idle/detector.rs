use std::time::Duration;

pub trait IdleDetector: Send {
    fn get_idle_time(&self) -> Duration;
    fn record_activity(&mut self);
}

pub fn create_idle_detector() -> Box<dyn IdleDetector> {
    #[cfg(target_os = "windows")]
    {
        Box::new(super::windows::WindowsIdleDetector::new())
    }
    
    #[cfg(target_os = "macos")]
    {
        Box::new(super::macos::MacIdleDetector::new())
    }
    
    #[cfg(target_os = "linux")]
    {
        Box::new(super::linux::LinuxIdleDetector::new())
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Box::new(super::fallback::FallbackIdleDetector::new())
    }
}