/** Adaptive logic - 
 * Chooses HIGH / MEDIUM / LOW
 * Reacts to: CPU, Bandwidth, Packet loss
**/
use super::state::StreamProfile;

pub struct StreamController;

impl StreamController {
    pub fn new() -> Self {
        Self
    }

    pub async fn evaluate(&self) -> Option<StreamProfile> {
        // For now: no auto switching
        None
    }
}
