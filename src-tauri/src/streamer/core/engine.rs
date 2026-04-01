/** Main orchestrator
    * Starts / stops streaming, Wires capture → encoder → transport, Owns async task 
    * “Brainstem of the streamer”
**/
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::RwLock;

use super::state::{StreamingState, StreamProfile};
use super::scheduler::FrameScheduler;
use super::controller::StreamController;

pub struct StreamerEngine {
    running: Arc<AtomicBool>,
    state: Arc<RwLock<StreamingState>>,
    scheduler: FrameScheduler,
    controller: StreamController,
}

impl StreamerEngine {
    pub fn new(profile: StreamProfile) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            state: Arc::new(RwLock::new(StreamingState::new(profile))),
            scheduler: FrameScheduler::new(profile.fps()),
            controller: StreamController::new(),
        }
    }

    pub async fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        println!("📡 Streamer started");

        while self.running.load(Ordering::Relaxed) {
            // FPS control
            self.scheduler.wait_for_next_frame().await;

            // Dummy frame work
            println!("📸 Captured frame");

            println!("🎞 Encoded frame");

            println!("📤 Sent frame");

            // Adaptive hook
            if let Some(new_profile) = self.controller.evaluate().await {
                let mut state = self.state.write().await;
                if state.profile != new_profile {
                    println!("🔁 Profile changed to {:?}", new_profile);
                    state.set_profile(new_profile);
                    self.scheduler.update_fps(new_profile.fps());
                }
            }
        }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
