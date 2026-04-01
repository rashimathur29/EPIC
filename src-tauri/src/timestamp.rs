// ========================================================
// src-tauri/src/timestamp.rs
// Centralized, cached, dynamic timestamp management
// ========================================================

use chrono::{Utc, DateTime};
use chrono_tz::Tz;
use std::sync::Mutex;
use crate::db::core::DbManager;

/// Global cached timezone for the current session
/// Loaded once after org validation / login
static ORG_TIMEZONE: Mutex<String> = Mutex::new(String::new());

pub struct TimestampManager;

impl TimestampManager {

    /// Call this **once** after successful organization validation or login
    /// This is the only place where we hit the database for timezone
    pub fn init(db: &DbManager) {
        let tz = db.get_org_timezone();   // one-time DB read
        *ORG_TIMEZONE.lock().unwrap() = tz.clone();
        log::info!("[TIMESTAMP] Org timezone initialized: {}", tz);
    }

    /// Fast access to current org timezone (no DB hit)
    pub fn get_org_timezone() -> String {
        let guard = ORG_TIMEZONE.lock().unwrap();
        if guard.is_empty() {
            log::warn!("[TIMESTAMP] Timezone not initialized, using default");
            "Asia/Kolkata".to_string()
        } else {
            guard.clone()
        }
    }

    pub fn org_timestamp() -> String {
        let tz_str = Self::get_org_timezone();
        
        let tz: Tz = tz_str.parse().unwrap_or_else(|_| {
            log::warn!("[TIMESTAMP] Unknown timezone '{}', falling back to UTC", tz_str);
            chrono_tz::UTC
        });

        Utc::now()
            .with_timezone(&tz)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    /// Returns current timestamp in **UTC ISO format** for APS fields
    /// Format: "2026-03-26T09:05:22.123456+00:00"
    /// This should be used for apscreatedatetime and apsupdatedatetime
    pub fn aps_timestamp() -> String {
        Utc::now().to_rfc3339()
    }

    /// Convenience method for created_at / updated_at (same as org_timestamp)
    pub fn created_at() -> String {
        Self::org_timestamp()
    }

    /// Reset cache (useful on logout)
    pub fn reset() {
        *ORG_TIMEZONE.lock().unwrap() = String::new();
        log::info!("[TIMESTAMP] Timezone cache reset");
    }

    pub fn convert_to_org_time(dt: DateTime<Utc>) -> String {
        let tz_str = Self::get_org_timezone();
        let tz: Tz = tz_str.parse().unwrap_or(chrono_tz::UTC);
        dt.with_timezone(&tz)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
    
}

// Helper trait to make DBManager cleaner
pub trait TimestampExt {
    fn get_org_timezone(&self) -> String;
}

impl TimestampExt for DbManager {
    fn get_org_timezone(&self) -> String {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(timezone, 'Asia/Kolkata') FROM org_config LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap_or_else(|_| "Asia/Kolkata".to_string())
    }
}
