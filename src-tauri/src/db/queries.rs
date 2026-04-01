// db/queries.rs — small set of DB operations
use crate::db::core::DbManager;
use crate::timestamp::TimestampManager;

use rusqlite::{OptionalExtension, params};
use serde_json;

impl DbManager {
    
    pub fn get_timestamps(&self) -> (String, String, String) {
        let org_ts = TimestampManager::org_timestamp();
        let aps_ts = TimestampManager::aps_timestamp();
        let tz     = TimestampManager::get_org_timezone();
        (org_ts, aps_ts, tz)
    }

    // ------------------------------------------------------
    // CHECK-IN LOGIC
    // ------------------------------------------------------
    pub fn log_checkin(&self) -> rusqlite::Result<String> {

        let (org_ts, aps_ts, tz) = self.get_timestamps();
        
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO user_checkin (checkin_time, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![org_ts, org_ts, org_ts, aps_ts, aps_ts, tz],
        )?;
        Ok(org_ts)
    }

    // ------------------------------------------------------
    // BREAK TOGGLE
    // ------------------------------------------------------
    pub fn save_break_toggle(
        &self,
        break_time: &str,
        reason: Option<&str>
    ) -> rusqlite::Result<()> {

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, breakout_time 
            FROM user_breaks 
            ORDER BY id DESC 
            LIMIT 1"
        )?;

        let last = stmt.query_row([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        }).optional()?;

        let (org_ts, aps_ts, tz) = self.get_timestamps();

        match last {
            Some((id, None)) => {
                // CLOSE BREAK
                conn.execute(
                    "UPDATE user_breaks
                    SET breakout_time = ?1, updated_at = ?2, apsupdatedatetime = ?3
                    WHERE id = ?4",
                    params![break_time, org_ts, aps_ts, id],   
                )?;
            }
            _ => {
                // START BREAK
                let r = reason.unwrap_or("manual");
                conn.execute(
                    "INSERT INTO user_breaks
                    (breakin_time, breakout_time, break_duration, reason, 
                    created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
                    VALUES (?1, NULL, NULL, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![break_time, r, org_ts, org_ts, aps_ts, aps_ts, tz],   
                )?;
            }
        }

        Ok(())
    }

    // ------------------------------------------------------
    // MINUTE ACTIVITY INSERT
    // ------------------------------------------------------
    pub fn insert_minute_activity(
        &self,
        minute_start: &str,
        minute_end: &str,
        keystrokes: i32,
        mouse_moves: i32,
        mouse_clicks: i32,
        idle_secs: i32,
    ) -> rusqlite::Result<()> {

        let (org_ts, aps_ts, tz) = self.get_timestamps();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO user_activity_minute
             (minute_start, minute_end, keystroke_count, mouse_move_count, mouse_click_count, idle_seconds, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                minute_start,
                minute_end,
                keystrokes,
                mouse_moves,
                mouse_clicks,
                idle_secs,
                org_ts,
                org_ts,
                aps_ts,
                aps_ts,
                tz
            ],
        )?;

        Ok(())
    }

    // ------------------------------------------------------
    // FINAL SUMMARY — USING COUNT LISTS
    // ------------------------------------------------------
    pub fn insert_summary_with_count_lists(
        &self,
        start_time: &str,
        end_time: &str,
        keystroke_count_list: &Vec<i32>,
        mouse_movement_count_list: &Vec<i32>,
        mouse_click_count_list: &Vec<i32>,
        total_idle_seconds: i32,
    ) -> crate::db::core::Result<()> {

        let (org_ts, aps_ts, tz) = self.get_timestamps();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO user_activity_summary
             (start_time, end_time,
              keystroke_list, mouse_movement_list, mouse_click_list,
              total_idle_seconds,
              created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                start_time,
                end_time,
                serde_json::to_string(keystroke_count_list)?,
                serde_json::to_string(mouse_movement_count_list)?,
                serde_json::to_string(mouse_click_count_list)?,
                total_idle_seconds,
                org_ts,
                org_ts,
                aps_ts,
                aps_ts,
                tz
            ],
        )?;

        Ok(())
    }

      // Add this method for deleting minutes
    pub fn delete_minutes_by_ids(&self, ids: &[i64]) -> rusqlite::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        
        // Create placeholders
        let placeholders: String = ids.iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "DELETE FROM user_activity_minute WHERE id IN ({})",
            placeholders
        );
        
        let mut stmt = conn.prepare(&sql)?;
        
        // Bind parameters
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        
        stmt.execute(rusqlite::params_from_iter(params))?;
        
        Ok(())
    }
    
    // Helper to get last inserted ID
    pub fn get_last_insert_id(&self) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT last_insert_rowid()",
            [],
            |row| row.get(0),
        )
    }
    
}
