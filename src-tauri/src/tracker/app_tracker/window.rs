use crate::Result;

#[derive(Debug, Clone)]
pub struct ActiveWindowInfo {
    pub window_title: String,
    pub process_name: String,
}

pub fn get_current_active_window() -> Result<ActiveWindowInfo> {
    platform::get_active_window()
}

#[cfg(target_os = "windows")]
mod platform {
    pub use crate::tracker::app_tracker::windows::get_active_window;
}

#[cfg(target_os = "macos")]
mod platform {
    pub use super::macos::get_active_window;
}

#[cfg(target_os = "linux")]
mod platform {
    pub use super::linux::get_active_window;
}
