use crate::tracker::events::TrackEvent;
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool};

pub trait InputMonitor: Send {
    /// Start monitoring input events
    fn start(&mut self) -> Result<(), String>;
    
    /// Stop monitoring input events
    fn stop(&mut self) -> Result<(), String>;
    
    /// Check if monitoring is active
    fn is_running(&self) -> bool;
}

pub fn create_input_monitor(
    event_tx: Sender<TrackEvent>,
    running: Arc<AtomicBool>,
) -> Box<dyn InputMonitor> {
    #[cfg(target_os = "windows")]
    {
        Box::new(super::windows::WindowsInputMonitor::new(event_tx, running))
    }
    
    #[cfg(target_os = "macos")]
    {
        Box::new(super::macos::MacInputMonitor::new(event_tx, running))
    }
    
    #[cfg(target_os = "linux")]
    {
        Box::new(super::linux::LinuxInputMonitor::new(event_tx, running))
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        compile_error!("Unsupported platform for input monitoring");
    }
}