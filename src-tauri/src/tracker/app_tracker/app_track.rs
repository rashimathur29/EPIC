use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use chrono::{Utc, Local};

use crate::db::core::DbManager;
use crate::tracker::app_tracker::window::{get_current_active_window, ActiveWindowInfo};

pub fn start_active_window_tracker(
    db: Arc<DbManager>,
    running: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut last_info: Option<ActiveWindowInfo> = None;
        let mut start_time = Utc::now();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        while running.load(Ordering::Relaxed) {

            // 1. Get current active window
            let current = match get_current_active_window() {
                Ok(info) => info,
                Err(_) => {
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            match &last_info {
                // 2. First time
                None => {
                    last_info = Some(current);
                    start_time = Utc::now();
                }

                Some(prev) => {
                    // 3. If window/app changed
                    if prev.window_title != current.window_title
                        || prev.process_name != current.process_name
                    {
                        let end_time = Utc::now();
                        let duration_sec = end_time
                            .signed_duration_since(start_time)
                            .num_seconds()
                            .max(0);

                        // 4. Save previous window record
                        let conn = db.conn.lock().unwrap();
                        let utc_now = Utc::now().to_rfc3339();
                        let tz = Local::now().format("%Z").to_string();
                        let _ = conn.execute(
                                    "INSERT INTO active_window
                                    (window_title, process_name, start_time, end_time, duration_sec, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
                                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                                    rusqlite::params![
                                        prev.window_title,
                                        prev.process_name,
                                        start_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                                        end_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                                        duration_sec,
                                        now, // created_at
                                        now, // updated_at
                                        utc_now,
                                        utc_now,
                                        tz
                                    ],
                                );

                        // 5. Start tracking new window
                        last_info = Some(current);
                        start_time = end_time;
                    }
                }
            }

            // 6. Sleep (important for CPU)
            thread::sleep(Duration::from_secs(5));
        }

        // 7. Final save when tracking stops
        if let Some(prev) = last_info {
            let end_time = Utc::now();
            let duration_sec = end_time
                .signed_duration_since(start_time)
                .num_seconds()
                .max(0);

            let conn = db.conn.lock().unwrap();
            
            let utc_now = Utc::now().to_rfc3339();
            let tz = Local::now().format("%Z").to_string();
            let _ = conn.execute(
                        "INSERT INTO active_window
                        (window_title, process_name, start_time, end_time, duration_sec, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        rusqlite::params![
                            prev.window_title,
                            prev.process_name,
                            start_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                            end_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                            duration_sec,
                            now, // created_at
                            now, // updated_at
                            utc_now,
                            utc_now,
                            tz
                        ],
                    );

        }
    });
}
