use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

use chrono::Local;
use chrono::Utc;
use rand::Rng;
use screenshots::Screen;
use tauri::{AppHandle, Manager};

use crate::db::core::DbManager;
use crate::tracker::idle::IdleDetector;
use crate::tracker::config::IDLE_BREAK_THRESHOLD;
use crate::timestamp::TimestampManager;

pub const DEFAULT_SCREENSHOT_INTERVAL_SEC: u64 = 1800; // 30 min
pub const MIN_INTERVAL_SEC: u64 = 300; // 5 min safety

pub fn start_screenshot_tracker(
    app_handle: AppHandle,
    db: Arc<DbManager>,
    running: Arc<AtomicBool>,
    idle_detector: Arc<std::sync::Mutex<Box<dyn IdleDetector>>>,
    interval_sec: Option<u64>,
) {
    let interval_sec = interval_sec.unwrap_or(DEFAULT_SCREENSHOT_INTERVAL_SEC);
    let effective_interval = interval_sec.max(MIN_INTERVAL_SEC);

    log::info!(
        "[SCREENSHOT] Tracker started — interval ≈ {} minutes (randomized)",
        effective_interval / 60
    );

    thread::spawn(move || {
        let mut rng = rand::thread_rng();

        // 🔒 Track next window start to avoid duplicates
        let mut next_window_start_ts: i64 = 0;

        while running.load(Ordering::Relaxed) {
            let now = Utc::now().timestamp();
            let window_seconds = effective_interval as i64;

            // 1️⃣ Calculate next window start (STRICT forward movement)
            if next_window_start_ts == 0 {
                next_window_start_ts = now - (now % window_seconds) + window_seconds;
            } else {
                next_window_start_ts += window_seconds;
            }

            let window_start_ts = next_window_start_ts;
            let window_end_ts = window_start_ts + window_seconds;

            // 2️⃣ Pick random time inside this window
            let random_offset = rng.gen_range(0..window_seconds);
            let capture_ts = window_start_ts + random_offset;

            let sleep_seconds = capture_ts - Utc::now().timestamp();
            if sleep_seconds > 0 {
                thread::sleep(Duration::from_secs(sleep_seconds as u64));
            }

            if !running.load(Ordering::Relaxed) {
                break;
            }

            // 3️⃣ Idle check (skip only, do not break timeline)
            let idle_seconds = if let Ok(detector) = idle_detector.lock() {
                detector.get_idle_time().as_secs()
            } else {
                0
            };

            if idle_seconds >= IDLE_BREAK_THRESHOLD.as_secs() {
                log::debug!(
                    "[SCREENSHOT] User away (idle {}s) — skipping this window",
                    idle_seconds
                );
                continue;
            }

            // 4️⃣ Capture primary screen
            let screens = match Screen::all() {
                Ok(s) if !s.is_empty() => s,
                _ => {
                    log::warn!("[SCREENSHOT] No screens found");
                    continue;
                }
            };

            let screen = &screens[0];

            let image = match screen.capture() {
                Ok(img) => img,
                Err(e) => {
                    log::error!("[SCREENSHOT] Capture failed: {}", e);
                    continue;
                }
            };

            // 5️⃣ Build file path
            let timestamp = Local::now();
            let filename = format!("ss_{}.png", timestamp.format("%Y-%m-%d_%H-%M-%S"));

            let app_data_dir = match app_handle.path().app_data_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    log::error!("[SCREENSHOT] Failed to resolve app data dir: {}", e);
                    continue;
                }
            };

            let ss_dir = app_data_dir.join("screenshots");
            if std::fs::create_dir_all(&ss_dir).is_err() {
                log::error!("[SCREENSHOT] Failed to create screenshots dir");
                continue;
            }

            let full_path = ss_dir.join(&filename);

            let buffer = match image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                image.width(),
                image.height(),
                image.rgba().to_vec(),
            ) {
                Some(buf) => buf,
                None => {
                    log::error!("[SCREENSHOT] Failed to build image buffer");
                    continue;
                }
            };

            if buffer.save(&full_path).is_err() {
                log::error!("[SCREENSHOT] Failed to save file");
                continue;
            }

            // 6️⃣ DB insert
            let org_ts = TimestampManager::org_timestamp();
            let aps_ts = TimestampManager::aps_timestamp();
            let tz     = TimestampManager::get_org_timezone();

            if let Ok(conn) = db.conn.lock() {
                let _ = conn.execute(
                    "INSERT INTO screenshots 
                    (screenshot, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        filename,
                        org_ts,
                        org_ts,
                        aps_ts,
                        aps_ts,
                        tz
                    ],
                );
            }

            log::info!("[SCREENSHOT] Captured → {}", filename);

            // 7️⃣ Sleep until window end (hard guarantee no duplicate)
            let now_after = Utc::now().timestamp();
            let remaining = window_end_ts - now_after;

            if remaining > 0 {
                thread::sleep(Duration::from_secs(remaining as u64));
            }
        }

        log::info!("[SCREENSHOT] Tracker thread stopped");
    });
}