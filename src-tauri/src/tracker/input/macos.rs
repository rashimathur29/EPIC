use super::monitor::InputMonitor;
use crate::tracker::events::TrackEvent;
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct MacInputMonitor {
    event_tx: Sender<TrackEvent>,
    running: Arc<AtomicBool>,
}

impl MacInputMonitor {
    pub fn new(event_tx: Sender<TrackEvent>, running: Arc<AtomicBool>) -> Self {
        Self {
            event_tx,
            running,
        }
    }
}

impl InputMonitor for MacInputMonitor {
    fn start(&mut self) -> Result<(), String> {
        eprintln!("⚠️  macOS input monitoring not yet implemented");
        Ok(())
    }
    
    fn stop(&mut self) -> Result<(), String> {
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}