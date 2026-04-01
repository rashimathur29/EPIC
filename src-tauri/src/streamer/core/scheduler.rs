/** FPS governor
 * Enforces: 10 FPS → 100ms/6 FPS → 166ms/3 FPS → 333ms
 * Prevents over-capture
 * Drops frames if late
 * This is critical for CPU safety.
 * **/
use tokio::time::{sleep, Duration, Instant};

pub struct FrameScheduler {
    frame_interval: Duration,
    last_frame: Instant,
}

impl FrameScheduler {
    pub fn new(fps: u64) -> Self {
        Self {
            frame_interval: Duration::from_millis(1000 / fps),
            last_frame: Instant::now(),
        }
    }

    pub async fn wait_for_next_frame(&mut self) {
        let elapsed = self.last_frame.elapsed();

        if elapsed < self.frame_interval {
            sleep(self.frame_interval - elapsed).await;
        }

        self.last_frame = Instant::now();
    }

    pub fn update_fps(&mut self, fps: u64) {
        self.frame_interval = Duration::from_millis(1000 / fps);
    }
}
