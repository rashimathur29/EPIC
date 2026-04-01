// tracker/input/linux.rs
use super::monitor::InputMonitor;
use crate::tracker::events::TrackEvent;
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct LinuxInputMonitor {
    event_tx: Sender<TrackEvent>,
    running: Arc<AtomicBool>,
}

impl LinuxInputMonitor {
    pub fn new(event_tx: Sender<TrackEvent>, running: Arc<AtomicBool>) -> Self {
        Self {
            event_tx,
            running,
        }
    }
}

impl InputMonitor for LinuxInputMonitor {
    fn start(&mut self) -> Result<(), String> {
        // TODO: Implement Linux input monitoring
        // This requires reading from /dev/input/event* or using X11/Wayland APIs
        eprintln!("⚠️  Linux input monitoring not yet implemented");
        eprintln!("ℹ️  Note: Requires root permissions or input group membership");
        Ok(())
    }
    
    fn stop(&mut self) -> Result<(), String> {
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

// Note: Full Linux implementation requires:
// 1. Reading from /dev/input/event* devices (requires permissions)
// 2. OR using X11 XRecord extension
// 3. OR using evdev library
// 4. Handle different display servers (X11 vs Wayland)