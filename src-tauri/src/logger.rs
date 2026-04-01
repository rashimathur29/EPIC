// src/logger.rs - Logging System
use log::LevelFilter;
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        rolling_file::{
            policy::compound::{
                roll::fixed_window::FixedWindowRoller,
                trigger::size::SizeTrigger,
                CompoundPolicy,
            },
            RollingFileAppender,
        },
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
};
use std::path::PathBuf;
use tauri::Manager; // Import Manager trait

/// Initialize professional logging system
/// 
/// Logs will be stored in:
/// - Windows: C:\Users\<user>\AppData\Local\EPIC\logs\
/// - macOS: ~/Library/Application Support/EPIC/logs/
/// - Linux: ~/.local/share/EPIC/logs/
/// 
/// Log rotation:
/// - Max file size: 10 MB
/// - Keep last 10 log files
/// - Pattern: epic.log, epic-1.log, epic-2.log, ..., epic-10.log
pub fn init_logger(app_handle: &tauri::AppHandle) -> anyhow::Result<()> {
    // Get log directory path
    let log_dir = get_log_directory(app_handle)?;
    std::fs::create_dir_all(&log_dir)?;
    
    let log_file = log_dir.join("epic.log");
    let archive_pattern = log_dir.join("epic-{}.log").to_string_lossy().to_string();
    
    // Log pattern: [2024-12-15 10:30:45.123] [INFO] [epic_tauri_lib::worker] User checked in
    let log_pattern = "[{d(%Y-%m-%d %H:%M:%S%.3f)}] [{h({l}):5.5}] [{t}] {m}{n}";
    
    // Console appender (for development)
    let console = ConsoleAppender::builder()
        .target(Target::Stdout)
        .encoder(Box::new(PatternEncoder::new(log_pattern)))
        .build();
    
    // Rolling file appender
    // Trigger: Roll when file reaches 10 MB
    let size_trigger = SizeTrigger::new(10 * 1024 * 1024); // 10 MB
    
    // Roller: Keep last 10 archived log files
    let roller = FixedWindowRoller::builder()
        .base(0)
        .build(&archive_pattern, 10)?;
    
    // Compound policy: size trigger + fixed window roller
    let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));
    
    // File appender with rotation
    let file = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(log_pattern)))
        .build(log_file, Box::new(policy))?;
    
    // Build configuration
    let config = Config::builder()
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Debug)))
                .build("console", Box::new(console)),
        )
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Info)))
                .build("file", Box::new(file)),
        )
        .build(
            Root::builder()
                .appender("console")
                .appender("file")
                .build(LevelFilter::Debug),
        )?;
    
    // Initialize logger
    log4rs::init_config(config)?;
    
    log::info!("========================================");
    log::info!("EPIC Application Started");
    log::info!("Version: {}", env!("CARGO_PKG_VERSION"));
    log::info!("Log Directory: {}", log_dir.display());
    log::info!("========================================");
    
    Ok(())
}

/// Get platform-specific log directory
fn get_log_directory(app_handle: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let app_data_dir = app_handle
        .path()
        .resolve("logs", tauri::path::BaseDirectory::AppData)
        .map_err(|e| anyhow::anyhow!("Failed to resolve log directory: {}", e))?;
    
    Ok(app_data_dir)
}

/// Utility macro for structured logging with context
#[macro_export]
macro_rules! log_with_context {
    (INFO, $module:expr, $action:expr, $($arg:tt)*) => {
        log::info!("[{}] [{}] {}", $module, $action, format!($($arg)*))
    };
    (WARN, $module:expr, $action:expr, $($arg:tt)*) => {
        log::warn!("[{}] [{}] {}", $module, $action, format!($($arg)*))
    };
    (ERROR, $module:expr, $action:expr, $($arg:tt)*) => {
        log::error!("[{}] [{}] {}", $module, $action, format!($($arg)*))
    };
    (DEBUG, $module:expr, $action:expr, $($arg:tt)*) => {
        log::debug!("[{}] [{}] {}", $module, $action, format!($($arg)*))
    };
}

/// Performance tracking helper
pub struct PerfTimer {
    name: String,
    start: std::time::Instant,
}

impl PerfTimer {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: std::time::Instant::now(),
        }
    }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        log::debug!("[PERF] {} completed in {:.2}ms", self.name, elapsed.as_secs_f64() * 1000.0);
    }
}