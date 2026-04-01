// tracker/input/mod.rs
mod monitor;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

pub use monitor::{InputMonitor, create_input_monitor};