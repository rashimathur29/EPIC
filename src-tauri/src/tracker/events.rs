use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrackEvent {
    Key,           // Any key pressed
    MouseMove,     // Any mouse movement
    MouseClick,    // Any mouse button click
    IdleTick,      // 1 second of user idle
}

impl TrackEvent {
    /// Get event type as string (for debugging/logging)
    pub fn as_str(&self) -> &'static str {
        match self {
            TrackEvent::Key => "key",
            TrackEvent::MouseMove => "mouse_move",
            TrackEvent::MouseClick => "mouse_click",
            TrackEvent::IdleTick => "idle_tick",
        }
    }
    
    /// Check if event indicates user activity
    pub fn is_activity(&self) -> bool {
        match self {
            TrackEvent::Key | TrackEvent::MouseMove | TrackEvent::MouseClick => true,
            TrackEvent::IdleTick => false,
        }
    }
}