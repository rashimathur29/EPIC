mod detector;
mod fallback;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

pub use detector::{IdleDetector, create_idle_detector};
pub use fallback::FallbackIdleDetector;