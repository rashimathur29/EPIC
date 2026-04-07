use crate::AppState;
use tauri::{State, command, AppHandle, Manager};
use chrono::{DateTime, Utc, NaiveDateTime};
use chrono_tz::Tz;
use rusqlite::OptionalExtension;
use crate::timestamp::TimestampManager;

use crate::db::core::{Error, Result};
use crate::tracker::ActivityTracker;
use crate::tracker::aggregator::minute::MinuteEntry;

use crate::tracker::aggregator::MinuteData;
use crate::tracker::aggregator::SummaryGenerator;
use crate::tracker::storage::{StorageWriter, DbActivityPersister};
use crate::tracker::worker::{write_inactivity, make_period, InactivityReason};
use crate::tracker::screenshot::start_screenshot_tracker;
use crate::tracker::screenshot::DEFAULT_SCREENSHOT_INTERVAL_SEC;

use crate::streaming::pipeline::{Pipeline, PipelineConfig};

#[cfg(target_os = "windows")]
use crate::tracker::input::windows::get_last_break_start_time;

pub fn get_timestamps() -> (String, String, String) {
    let org_ts = TimestampManager::org_timestamp();
    let aps_ts = TimestampManager::aps_timestamp();
    let tz     = TimestampManager::get_org_timezone();
    (org_ts, aps_ts, tz)
}

// ─────────────────────────────────────────────────────────────
// HELPER: start the pipeline and store in AppState
// Called from checkin() and resume_tracking()
// ─────────────────────────────────────────────────────────────
fn start_pipeline_for_state(state: &AppState, app: &AppHandle) {
    let mut pipeline_guard = state.pipeline.lock().unwrap();

    // Don't start if already running (safety check)
    if pipeline_guard.is_some() {
        log::warn!("[PIPELINE] Already running, skipping start");
        return;
    }

    // Get the recordings directory from Tauri's app data dir
    // This is: %APPDATA%\epic on Windows, ~/Library/Application Support/epic on macOS
    let output_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            log::error!("[PIPELINE] Cannot get app data dir: {}. Recordings disabled.", e);
            return;
        }
    };

    let config = PipelineConfig {
        fps:           2,       // 5fps 
        width:         1280,
        height:        720,
        enable_stream: true,    // live stream at ws://localhost:9001
        enable_record: true,    // save .mp4 files to recordings dir
        ws_port:       9001,
        jpeg_quality:  55,
        output_dir,
        segment_secs:  300,     // new MP4 file every 5 minutes
        bitrate_kbps:  500,
    };

    *pipeline_guard = Some(Pipeline::start(config));
    log::info!("[PIPELINE] ✅ Pipeline started (stream + record)");
}

// ─────────────────────────────────────────────────────────────
// HELPER: stop the pipeline
// Called from checkout() and toggle_break() when break starts
// ─────────────────────────────────────────────────────────────
fn stop_pipeline_for_state(state: &AppState) {
    let mut pipeline_guard = state.pipeline.lock().unwrap();
    if let Some(pipeline) = pipeline_guard.take() {
        pipeline.stop();
        log::info!("[PIPELINE] ✅ Pipeline stopped");
    } else {
        log::warn!("[PIPELINE] Stop called but pipeline was not running");
    }
}

// ─────────────────────────────────────────────────────────────
// HELPER: is the user currently on a break?
// ─────────────────────────────────────────────────────────────
fn is_currently_on_break(state: &AppState) -> bool {
    let db = &state.db;
    let conn = db.conn.lock().unwrap();

    let result: Option<Option<String>> = conn.query_row(
        "SELECT breakout_time FROM user_breaks ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).optional().unwrap_or(None);

    // Some(None) = row exists AND breakout_time is NULL = ON BREAK
    // Some(Some(_)) = row exists with breakout_time = NOT on break
    // None = no rows at all = NOT on break
    matches!(result, Some(None))
}

// ─────────────────────────────────────────────────────────────
// CHECK-IN
// Insert DB entry
// Start background tracking worker
// ─────────────────────────────────────────────────────────────

#[command]
pub fn checkin(state: State<AppState>, app_handle: AppHandle) -> Result<String> {
    log::info!("[CHECKIN] Check-in request received");

    let db = state.inner().db.clone();
    let (org_ts, aps_ts, _) = get_timestamps();

    // 1. Prevent double check-in
    {
        let guard = state.inner().tracker.lock().unwrap();
        if guard.is_some() {
            log::warn!("[CHECKIN] Failed - already checked in");
            return Err(Error::WorkerError("Already checked in. Please check out first.".into()));
        }
    }

    // 2. Insert check-in row
    let checkin_time = db
        .log_checkin()
        .map_err(|e| {
            log::error!("[CHECKIN] Database error: {}", e);
            Error::Database(e.to_string())
        })?;

    log::info!("[CHECKIN] Database entry created at {}", checkin_time);

    // 3. Update last_active_time (non-fatal if fails)
    {
        let conn = db.conn.lock().unwrap();
        if let Err(e) = conn.execute(
            "UPDATE user_checkin 
             SET last_active_time = ?1, updated_at = ?1, apsupdatedatetime = ?2
             WHERE checkin_time = ?3",
            rusqlite::params![org_ts, aps_ts, checkin_time],
        ) {
            log::warn!("[CHECKIN] Failed to update last_active_time: {}", e);
        }
    }

    // 4. Start main activity tracker
    let mut guard = state.inner().tracker.lock().unwrap();

    match ActivityTracker::new(db.clone(), None, None) {
        Ok(tracker) => {
            *guard = Some(tracker);
            log::info!("[CHECKIN] ✅ Activity tracker started");

            // 5. Start screenshot tracker
            let tracker = guard.as_ref().unwrap();
            let running      = tracker.running_flag();
            let idle_detector = tracker.idle_detector();

            start_screenshot_tracker(
                app_handle.clone(),
                db.clone(),
                running,
                idle_detector,
                Some(DEFAULT_SCREENSHOT_INTERVAL_SEC),
            );

            log::info!("[CHECKIN] Screenshot tracker started");
        }
        Err(e) => {
            log::error!("[CHECKIN] CRITICAL: Failed to start activity tracker: {}", e);

            // Rollback check-in entry
            let _ = db.conn.lock().unwrap().execute(
                "DELETE FROM user_checkin WHERE checkin_time = ?1",
                rusqlite::params![checkin_time],
            );

            return Err(Error::WorkerError(format!(
                "Failed to start activity tracking: {}\n\n\
                 Possible causes:\n\
                 • Antivirus/security blocking input hooks\n\
                 • Another app using system hooks (TeamViewer, macro tools, etc.)\n\
                 • Insufficient permissions (try run as admin)\n\
                 Technical: {}",
                e, e
            )));
        }
    }

    // drop the tracker guard before starting pipeline (pipeline.lock() must not be taken while tracker.lock() is held)
    drop(guard);

    // ── 6. Start streaming pipeline ───────────────────────────
    // START: screen capture + live stream + video recording (Called AFTER tracker starts so if tracker fails, pipeline never starts)
    start_pipeline_for_state(state.inner(), &app_handle);

    log::info!("[CHECKIN] ✅ Check-in complete at {}", checkin_time);
    Ok(checkin_time)
}

/// CHECK-OUT - Stop tracker 

#[command]
pub fn checkout(state: State<AppState>) -> Result<String> {
    log::info!("[CHECKOUT] Check-out request received");

    let db = state.inner().db.clone();
    let (org_ts, aps_ts, _) = get_timestamps();    

    // 1. Stop activity tracker
    {
        let mut guard = state.inner().tracker.lock().unwrap();

        if let Some(mut tracker) = guard.take() {
            log::info!("[CHECKOUT] Stopping activity tracker...");
            tracker.stop().map_err(|e| {
                log::error!("[CHECKOUT] Failed to stop tracker: {}", e);
                Error::WorkerError(e.to_string())
            })?;
            log::info!("[CHECKOUT] Activity tracker stopped");
        } else {
            log::warn!("[CHECKOUT] Not checked in");
            return Err(Error::WorkerError("Not checked in. Nothing to check out.".into()));
        }
    }

    // 2. Update check-out timestamp in DB
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE user_checkin
             SET checkout_time = ?1, updated_at = ?1, apsupdatedatetime = ?2
             WHERE id = (SELECT MAX(id) FROM user_checkin)",
            rusqlite::params![org_ts, aps_ts],
        )
        .map_err(|e| {
            log::error!("[CHECKOUT] DB update error: {}", e);
            Error::Database(e.to_string())
        })?;

    // ── 3. Stop streaming pipeline ────────────────────────────
    // STOP: ends screen capture, WebSocket stream, finalizes last .mp4
    stop_pipeline_for_state(state.inner());

    log::info!("[CHECKOUT] ✅ Check-out complete at {}", org_ts);
    Ok(org_ts)
}

// ─────────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────────

/// Returns when user last had activity (last minute_end, or checkin_time as fallback)
fn last_activity_time(
    conn:         &std::sync::MutexGuard<rusqlite::Connection>,
    checkin_time: &str,
) -> DateTime<Utc> {
    let s: Option<String> = conn.query_row(
        "SELECT minute_end FROM user_activity_minute
         WHERE created_at >= ?1 ORDER BY id DESC LIMIT 1",
        [checkin_time], |r| r.get(0),
    ).optional().unwrap_or(None).flatten();

    parse_utc(s.as_deref().unwrap_or(checkin_time))
        .unwrap_or_else(|_| Utc::now())
}

fn parse_utc(s: &str) -> crate::Result<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
        .map_err(|e| crate::Error::Database(format!("Parse error: {}", e)))
}

fn daily_focus_seconds(
    conn:            &std::sync::MutexGuard<rusqlite::Connection>,
    today_start_str: &str,
    now:             DateTime<Utc>,
) -> i64 {
    let mut total: i64 = 0;
    if let Ok(mut stmt) = conn.prepare(
        "SELECT checkin_time FROM user_checkin WHERE checkin_time >= ?1"
    ) {
        if let Ok(mut rows) = stmt.query([today_start_str]) {
            while let Ok(Some(row)) = rows.next() {
                let s: String = row.get(0).unwrap_or_default();
                if let Ok(dt) = parse_utc(&s) {
                    total += now.signed_duration_since(dt).num_seconds().max(0);
                }
            }
        }
    }
    let mut breaks: i64 = 0;
    if let Ok(mut stmt) = conn.prepare(
        "SELECT COALESCE(break_duration,0) FROM user_breaks WHERE created_at >= ?1"
    ) {
        if let Ok(mut rows) = stmt.query([today_start_str]) {
            while let Ok(Some(row)) = rows.next() {
                breaks += row.get::<_, i64>(0).unwrap_or(0);
            }
        }
    }
    (total - breaks).max(0)
}

///
/// GET STATUS
/// - Check if currently checked in
///

#[command]
pub fn get_status(state: State<AppState>) -> Result<bool> {
    let guard = state.inner().tracker.lock().unwrap();
    let is_active = guard.is_some();
    log::debug!("[STATUS] Current status: {}", if is_active { "CHECKED_IN" } else { "CHECKED_OUT" });
    Ok(is_active)
}

///
/// RECORD KEY PRESSED 
///

#[command]
pub fn record_key(state: State<AppState>) -> Result<()> {
    let guard = state.inner().tracker.lock().unwrap();
    if let Some(t) = &*guard {
        t.record_key()
            .map_err(|e| Error::WorkerError(e.to_string()))?;
    }
    Ok(())
}

/// RECORD MOUSE MOVE 

#[command]
pub fn record_mouse_move(state: State<AppState>) -> Result<()> {
    let guard = state.inner().tracker.lock().unwrap();
    if let Some(t) = &*guard {
        t.record_mouse_move()
            .map_err(|e| Error::WorkerError(e.to_string()))?;
    }
    Ok(())
}

/// RECORD MOUSE CLICK 

#[command]
pub fn record_mouse_click(state: State<AppState>) -> Result<()> {
    let guard = state.inner().tracker.lock().unwrap();
    if let Some(t) = &*guard {
        t.record_mouse_click()
            .map_err(|e| Error::WorkerError(e.to_string()))?;
    }
    Ok(())
}

/// TOGGLE BREAK

#[command]
pub fn toggle_break(state: State<AppState>, reason: Option<String>, app_handle: AppHandle) -> Result<String> {
    log::info!("[BREAK] Toggle break (reason: {:?})", reason);

    let db = state.inner().db.clone();
    let (org_ts, _, _) = get_timestamps();

    // ── Read current break state BEFORE toggling ──────────────
    let was_on_break = is_currently_on_break(state.inner());
    log::info!("[BREAK] Current state: {}", if was_on_break { "ON BREAK → ending break" } else { "ACTIVE → starting break" });
    // ─────────────────────────────────────────────────────────

    db.save_break_toggle(&org_ts, reason.as_deref())
        .map_err(|e| {
            log::error!("[BREAK] DB error: {}", e);
            Error::Database(e.to_string())
        })?;

    if was_on_break {
        // Break just ENDED → user is back → restart pipeline
        log::info!("[BREAK] Break ended — restarting streaming pipeline");
        start_pipeline_for_state(state.inner(), &app_handle);
    } else {
        log::info!("[BREAK] Break started — stopping streaming pipeline");
        stop_pipeline_for_state(state.inner());
    }
    

    log::info!("[BREAK] ✅ Break toggled at {}", org_ts);
    Ok(org_ts)
}

///
/// GET HOOK STATISTICS
/// - Returns event statistics for monitoring
///

#[command]
pub fn get_hook_stats() -> Result<serde_json::Value> {
    #[cfg(target_os = "windows")]
    {
        use crate::tracker::input::windows::WindowsInputMonitor;
        let (received, throttled, sent) = WindowsInputMonitor::get_stats();
        
        let throttle_pct = if received > 0 {
            (throttled as f64 / received as f64 * 100.0) as u32
        } else {
            0
        };
        
        let stats = serde_json::json!({
            "received": received,
            "sent": sent,
            "throttled": throttled,
            "throttle_percentage": throttle_pct
        });
        
        log::debug!("[STATS] Hook statistics: {:?}", stats);
        Ok(stats)
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        Ok(serde_json::json!({
            "error": "Hook statistics only available on Windows"
        }))
    }
}

#[command]
pub fn get_startup_status(state: State<AppState>) -> crate::Result<serde_json::Value> {
    log::info!("[STARTUP] Checking session…");

    let db = state.db.clone();
    let (org_ts, _aps_ts, _tz) = get_timestamps();
    let now = Utc::now();
    let today_start = now.date_naive()
        .and_hms_opt(0, 0, 0).unwrap()
        .format("%Y-%m-%d 00:00:00").to_string();

    let conn = db.conn.lock().unwrap();

    let checkin_opt: Option<String> = conn.query_row(
        "SELECT checkin_time FROM user_checkin
         WHERE checkout_time IS NULL ORDER BY id DESC LIMIT 1",
        [], |r| r.get(0),
    ).optional()?;

    // Is tracker alive right now?
    let tracker_running = state.tracker.lock().unwrap().is_some();

    let Some(checkin_time) = checkin_opt else {
        // No open session
        let focus = daily_focus_seconds(&conn, &today_start, now);
        return Ok(serde_json::json!({
            "has_active_session":  false,
            "checkin_time":        "",
            "daily_focus_seconds": focus,
            "offline_minutes":     0,
            "message":             "Ready to check in"
        }));
    };

    // ── Tracker IS running — worker handles inactivity, we do nothing ──
    if tracker_running {
        conn.execute(
            "UPDATE user_checkin SET last_active_time=?1 WHERE checkin_time=?2",
            [&org_ts, &checkin_time],
        ).ok();

        let focus = daily_focus_seconds(&conn, &today_start, now);
        return Ok(serde_json::json!({
            "has_active_session":  true,
            "checkin_time":        checkin_time,
            "daily_focus_seconds": focus,
            "offline_minutes":     0,
            "message":             "Session active"
        }));
    }

    // ── Tracker NOT running — app was closed/restarted ─────────────────
    // Find when activity actually stopped (last minute written)
    let offline_start: DateTime<Utc> = {
        #[cfg(target_os = "windows")]
        {
            get_last_break_start_time()
                .unwrap_or_else(|| last_activity_time(&conn, &checkin_time))
        }
        #[cfg(not(target_os = "windows"))]
        {
            last_activity_time(&conn, &checkin_time)
        }
    };

    let offline_sec  = now.signed_duration_since(offline_start).num_seconds().max(0);
    let offline_mins = (offline_sec as f64 / 60.0).round() as u64;

    // Write ONE inactivity record for the gap — dedup guard prevents duplicates
    if offline_sec > 60 {
        // Release conn lock before write_inactivity (which takes its own lock)
        drop(conn);

        let period = make_period(offline_start, now, Some(InactivityReason::SystemOffline));
        write_inactivity(&db, &period);

        let conn2 = db.conn.lock().unwrap();
        conn2.execute(
            "UPDATE user_checkin SET last_active_time=?1 WHERE checkin_time=?2",
            [&org_ts, &checkin_time],
        ).ok();
        let focus = daily_focus_seconds(&conn2, &today_start, now);

        return Ok(serde_json::json!({
            "has_active_session":  true,
            "checkin_time":        checkin_time,
            "daily_focus_seconds": focus,
            "offline_minutes":     offline_mins,
            "message": format!("System was offline for {} minutes — marked as break", offline_mins)
        }));
    }

    // Short gap (< 60s) — not worth recording
    conn.execute(
        "UPDATE user_checkin SET last_active_time=?1 WHERE checkin_time=?2",
        [&org_ts, &checkin_time],
    ).ok();
    let focus = daily_focus_seconds(&conn, &today_start, now);
    Ok(serde_json::json!({
        "has_active_session":  true,
        "checkin_time":        checkin_time,
        "daily_focus_seconds": focus,
        "offline_minutes":     offline_mins,
        "message":             "Session resumed"
    }))
}

// Helper to avoid code duplication
fn fallback_to_db_last_minute(conn: &std::sync::MutexGuard<rusqlite::Connection>, checkin_time: &str) -> crate::Result<DateTime<Utc>> {
    let last_minute_end: Option<String> = conn.query_row(
        "SELECT minute_end FROM user_activity_minute WHERE created_at >= ?1 ORDER BY id DESC LIMIT 1",
        [checkin_time],
        |row| row.get(0),
    ).optional()?.flatten();

    let offline_start_str = last_minute_end.unwrap_or_else(|| checkin_time.to_owned());

    let naive = NaiveDateTime::parse_from_str(&offline_start_str, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| crate::Error::Database(format!("Parse error: {}", e)))?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

// Extracted daily focus calculation
fn calculate_daily_focus(conn: &std::sync::MutexGuard<rusqlite::Connection>, today_start_str: &str, now: DateTime<Utc>) -> i64 {
    let mut total_session_seconds: i64 = 0;
    let mut stmt = conn.prepare("SELECT checkin_time FROM user_checkin WHERE checkin_time >= ?1").unwrap();
    let mut rows = stmt.query([today_start_str]).unwrap();

    while let Some(row) = rows.next().unwrap() {
        let checkin_str: String = row.get(0).unwrap();
        let checkin_naive = NaiveDateTime::parse_from_str(&checkin_str, "%Y-%m-%d %H:%M:%S").unwrap();
        let checkin_dt = DateTime::<Utc>::from_naive_utc_and_offset(checkin_naive, Utc);
        total_session_seconds += now.signed_duration_since(checkin_dt).num_seconds().max(0);
    }

    let mut break_seconds: i64 = 0;
    let mut stmt_breaks = conn.prepare("SELECT break_duration FROM user_breaks WHERE created_at >= ?1").unwrap();
    let mut rows_breaks = stmt_breaks.query([today_start_str]).unwrap();

    while let Some(row) = rows_breaks.next().unwrap() {
        let secs: i64 = row.get(0).unwrap();
        break_seconds += secs;
    }

    (total_session_seconds - break_seconds).max(0)
}

#[command]
pub fn check_auth_state(state: State<AppState>) -> crate::Result<serde_json::Value> {
    let conn = state.db.conn.lock().unwrap();
 
    // Check for stored email in user_settings
    let email: Option<String> = conn.query_row(
        "SELECT email FROM user_settings WHERE id = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .unwrap_or(None)
    .flatten()
    .filter(|e: &String| !e.trim().is_empty());
 
    // Check for stored org_id in user_details
    let org_id: Option<String> = conn.query_row(
        "SELECT org_id FROM user_details ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .unwrap_or(None)
    .flatten()
    .filter(|o: &String| !o.trim().is_empty());
 
    log::info!(
        "[AUTH_STATE] email={} org_id={}",
        email.as_deref().unwrap_or("none"),
        org_id.as_deref().unwrap_or("none")
    );
 
    let redirect = match (email.as_ref(), org_id.as_ref()) {
        (Some(_), Some(_)) => "index",    // fully logged in
        (None,    Some(_)) => "login",    // org set, no user
        _                  => "find_org", // nothing stored
    };
 
    Ok(serde_json::json!({ "redirect": redirect }))
}

#[command]
pub fn get_total_active_seconds(state: State<AppState>) -> Result<i64> {
    let conn = state.db.conn.lock().unwrap();

    let (_, _, tz_str) = get_timestamps();
    let tz: Tz = tz_str.parse()
                    .map_err(|_| Error::Database("Invalid timezone".into()))?;

    // Get current open session's checkin_time
    let checkin_time: String = conn.query_row(
        "SELECT checkin_time FROM user_checkin WHERE checkout_time IS NULL ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| Error::Database(e.to_string()))?;

    let checkin_naive = NaiveDateTime::parse_from_str(&checkin_time, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| Error::Database(e.to_string()))?;

    let checkin_local = chrono::TimeZone::from_local_datetime(&tz, &checkin_naive)
                                        .single()
                                        .ok_or_else(|| Error::Database("Invalid local datetime".into()))?;
    let now_local = chrono::Utc::now().with_timezone(&tz);  
    let total_session_seconds = now_local
                                    .signed_duration_since(checkin_local)
                                    .num_seconds()
                                    .max(0);                      

    //let checkin_dt = DateTime::<Utc>::from_naive_utc_and_offset(checkin_naive, Utc);

    //let now = Utc::now();
    //let total_session_seconds = now.signed_duration_since(checkin_dt).num_seconds().max(0);

    // Sum break durations for this session only (filter by created_at >= checkin_time)
    let mut total_break_seconds: i64 = 0;
    let mut stmt_breaks = conn.prepare(
        "SELECT COALESCE(break_duration, 0) FROM user_breaks WHERE created_at >= ?1"
    )?;
    let mut rows_breaks = stmt_breaks.query([&checkin_time])?;

    while let Some(row) = rows_breaks.next()? {
        let secs: i64 = row.get(0)?;
        total_break_seconds += secs;
    }

    let active_seconds = total_session_seconds - total_break_seconds;
    Ok(active_seconds.max(0))
}

#[command]
pub fn resume_tracking(state: State<AppState>, app_handle: AppHandle) -> Result<String> {
    log::info!("[RESUME] Attempting to resume");

    let mut guard = state.inner().tracker.lock().unwrap();

    if guard.is_some() {
        log::info!("[RESUME] Tracker already running");
        return Ok("Tracker already active".to_string());
    }

    guard.take(); // drop any stale tracker

    let db = state.db.clone();

    match ActivityTracker::new(db, None, None) {
        Ok(tracker) => {
            *guard = Some(tracker);
            log::info!("[RESUME] ✅ Activity tracker resumed");

            // drop guard before starting pipeline (avoid nested lock)
            drop(guard);

            // ── Restart streaming pipeline ────────────────────
            start_pipeline_for_state(state.inner(), &app_handle);
            // ─────────────────────────────────────────────────

            Ok("Tracker and pipeline resumed".to_string())
        }
        Err(e) => {
            log::error!("[RESUME] Failed: {}", e);
            Err(Error::WorkerError(format!(
                "Failed to resume: {}\n\
                 Common causes:\n\
                 • Antivirus blocking input hooks\n\
                 • Conflicting apps (macro tools, remote desktop)\n\
                 Technical: {}",
                e, e
            )))
        }
    }
}

#[command]
pub fn prepare_exit(state: State<AppState>) -> Result<String> {
    log::info!("[PREPARE_EXIT] User closing app — generating partial summary from remaining minutes");

    let db = state.db.clone();
    let conn = db.conn.lock().unwrap();

    // Get all unflushed minutes from today (or since last summary)
    let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today_start_str = DateTime::<Utc>::from_naive_utc_and_offset(today_start, Utc)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let mut stmt = conn.prepare(
        "SELECT 
            minute_start, minute_end, 
            keystroke_count, mouse_move_count, mouse_click_count, idle_seconds
         FROM user_activity_minute 
         WHERE created_at >= ?1 
         ORDER BY id ASC"
    )?;

    let rows = stmt.query_map([&today_start_str], |row| {
        Ok(MinuteEntry {
            id: row.get(0)?,
            data: MinuteData {
                minute_start: row.get(1)?,
                minute_end: row.get(2)?,
                keystroke_count: row.get(3)?,
                mouse_move_count: row.get(4)?,
                mouse_click_count: row.get(5)?,
                idle_seconds: row.get(6)?,
            },
        })
    })?;

    let mut recent_minutes: Vec<MinuteEntry> = Vec::new();

    for row in rows {
        recent_minutes.push(row?);
    }

    if !recent_minutes.is_empty() {
        if let Some(partial_summary) = SummaryGenerator::generate(&recent_minutes) {
            let storage = StorageWriter::new(DbActivityPersister::new(state.db.clone()));
            if storage.insert_summary(partial_summary).is_ok() {
                log::info!(
                    "[PREPARE_EXIT] Saved partial summary from {} minutes on app close",
                    recent_minutes.len()
                );
            }
        }
    } else {
        log::info!("[PREPARE_EXIT] No recent minutes to summarize on close");
    }

    // Optional: Clean shutdown of tracker
    let mut guard = state.tracker.lock().unwrap();
    if let Some(tracker) = guard.as_mut() {
        let _ = tracker.stop();
    }

    Ok("App closing — partial summary saved".to_string())
}

#[command]
pub fn get_session_stats(state: State<AppState>) -> crate::Result<serde_json::Value> {
    let conn = state.db.conn.lock().unwrap();

    // ── Find checkin_time of the CURRENT open session ─────────────────
    let checkin_time: Option<String> = conn.query_row(
        "SELECT checkin_time FROM user_checkin
         WHERE checkout_time IS NULL
         ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).optional()?;

    let Some(checkin_time) = checkin_time else {
        // Not checked in — return zeroes
        return Ok(serde_json::json!({
            "idle_seconds":       0,
            "break_seconds":      0,
            "keystroke_count":    0,
            "mouse_click_count":  0,
            "mouse_move_count":   0,
        }));
    };

    // ── Idle seconds: confirmed inactivity (no input) ─────────────────
    // Reason = 'Unavailability' means the idle detector fired (no keyboard/mouse).
    // We exclude 'Screen locked' and 'System offline' because those are
    // different concepts — they don't appear in the idle_time bar.
    let idle_seconds: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration), 0)
         FROM user_inactivity
         WHERE inactivity_by = 'Unavailability'
           AND inactive_start_time >= ?1",
        [&checkin_time],
        |row| row.get(0),
    ).unwrap_or(0);

    // ── Break seconds: deliberate breaks (user clicked Pause/Break) ───
    // Only count breaks that have BOTH breakin_time AND breakout_time
    // (completed breaks). Ongoing break is not counted yet.
    let break_seconds: i64 = conn.query_row(
        "SELECT COALESCE(SUM(
            CAST(
                (strftime('%s', breakout_time) - strftime('%s', breakin_time))
                AS INTEGER
            )
         ), 0)
         FROM user_breaks
         WHERE breakin_time >= ?1
           AND breakout_time IS NOT NULL",
        [&checkin_time],
        |row| row.get(0),
    ).unwrap_or(0);

    // ── Input counts: keystrokes + clicks from activity minutes ───────
    // Sum from user_activity_minute for this session
    let (keystroke_count, mouse_click_count, mouse_move_count): (i64, i64, i64) = conn.query_row(
        "SELECT
            COALESCE(SUM(keystroke_count),   0),
            COALESCE(SUM(mouse_click_count), 0),
            COALESCE(SUM(mouse_move_count),  0)
         FROM user_activity_minute
         WHERE created_at >= ?1",
        [&checkin_time],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap_or((0, 0, 0));

    Ok(serde_json::json!({
        "idle_seconds":      idle_seconds,
        "break_seconds":     break_seconds,
        "keystroke_count":   keystroke_count,
        "mouse_click_count": mouse_click_count,
        "mouse_move_count":  mouse_move_count,
    }))
}

// ─────────────────────────────────────────────────────────────
// get_daily_progress — returns the complete daily progress data for the ring + bars on first load and on resume.
// ─────────────────────────────────────────────────────────────

#[command]
pub fn get_daily_progress(state: State<AppState>) -> crate::Result<serde_json::Value> {
    use chrono::{Utc, NaiveDateTime, DateTime};
    use crate::timestamp::TimestampManager;

    let conn = state.db.conn.lock().unwrap();
    let tz_str = TimestampManager::get_org_timezone();
    let now_utc = Utc::now();

    // Today start in org timezone
    let tz: chrono_tz::Tz = tz_str.parse().unwrap_or(chrono_tz::UTC);
    let now_local  = now_utc.with_timezone(&tz);
    let today_str  = now_local.format("%Y-%m-%d 00:00:00").to_string();

    // ── Active seconds (all sessions today) ──────────────────────────
    let mut active_sec: i64 = 0;
    {
        let mut stmt = conn.prepare(
            "SELECT checkin_time, COALESCE(checkout_time, ?1) as end_t
             FROM user_checkin
             WHERE DATE(checkin_time) = DATE(?2)"
        ).unwrap();

        let now_str = now_local.format("%Y-%m-%d %H:%M:%S").to_string();
        let today_date = now_local.format("%Y-%m-%d").to_string();

        let rows = stmt.query_map(
            rusqlite::params![now_str, today_date],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        ).unwrap();

        for row in rows.flatten() {
            let (ci, co) = row;
            if let (Ok(ci_dt), Ok(co_dt)) = (
                NaiveDateTime::parse_from_str(&ci, "%Y-%m-%d %H:%M:%S"),
                NaiveDateTime::parse_from_str(&co, "%Y-%m-%d %H:%M:%S"),
            ) {
                active_sec += (co_dt - ci_dt).num_seconds().max(0);
            }
        }
    }

    // ── Idle seconds today (Unavailability only) ──────────────────────
    let idle_sec: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration), 0)
         FROM user_inactivity
         WHERE inactivity_by = 'Unavailability'
           AND DATE(inactive_start_time) = DATE(?1)",
        [&today_str],
        |row| row.get(0),
    ).unwrap_or(0);

    // ── Break seconds today ───────────────────────────────────────────
    let break_sec: i64 = conn.query_row(
        "SELECT COALESCE(SUM(
            CAST(
                (strftime('%s', breakout_time) - strftime('%s', breakin_time))
                AS INTEGER
            )
         ), 0)
         FROM user_breaks
         WHERE DATE(breakin_time) = DATE(?1)
           AND breakout_time IS NOT NULL",
        [&today_str],
        |row| row.get(0),
    ).unwrap_or(0);

    // ── Keystroke / click counts today ────────────────────────────────
    let (keystrokes, clicks): (i64, i64) = conn.query_row(
        "SELECT
            COALESCE(SUM(keystroke_count),   0),
            COALESCE(SUM(mouse_click_count), 0)
         FROM user_activity_minute
         WHERE DATE(created_at) = DATE(?1)",
        [&today_str],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap_or((0, 0));

    // ── Yesterday active seconds (for the ring stat) ──────────────────
    let yesterday_str = (now_local - chrono::Duration::days(1))
        .format("%Y-%m-%d").to_string();

    let yesterday_sec: i64 = conn.query_row(
        "SELECT COALESCE(SUM(
            CAST(
                (strftime('%s', COALESCE(checkout_time, checkin_time))
                 - strftime('%s', checkin_time))
                AS INTEGER
            )
         ), 0)
         FROM user_checkin
         WHERE DATE(checkin_time) = DATE(?1)",
        [&yesterday_str],
        |row| row.get(0),
    ).unwrap_or(0);

    // Subtract breaks from active time for net active
    let net_active = (active_sec - break_sec - idle_sec).max(0);

    Ok(serde_json::json!({
        "active_seconds":    net_active,
        "idle_seconds":      idle_sec,
        "break_seconds":     break_sec,
        "keystroke_count":   keystrokes,
        "mouse_click_count": clicks,
        "yesterday_minutes": yesterday_sec / 60,
    }))
}