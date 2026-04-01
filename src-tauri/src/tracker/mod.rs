// Module declarations
pub mod config;
pub mod events;
pub mod worker;
pub mod screenshot;

pub mod aggregator;
pub mod storage;
pub mod idle;
pub mod input;
pub mod audio;
pub mod app_tracker;

// Re-exports for convenience
pub use config::TrackerConfig;
pub use events::TrackEvent;
pub use worker::ActivityTracker;