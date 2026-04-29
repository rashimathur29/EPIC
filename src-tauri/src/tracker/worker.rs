use crate::db::core::DbManager;
use crate::tracker::{
    config::TrackerConfig,
    events::TrackEvent,
    aggregator::{MinuteAggregator, SummaryGenerator},
    storage::{StorageWriter, DbActivityPersister},
    idle::{create_idle_detector, IdleDetector},
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::thread;
use tauri::Emitter;
use std::time::{Duration as StdDuration, Instant};
use chrono::{Utc, DateTime};
use crate::tracker::config::{IDLE_ACTIVITY_THRESHOLD, IDLE_BREAK_THRESHOLD};
use crate::tracker::audio::mic_detector::MicrophoneDetector;
use crate::timestamp::TimestampManager;
use crate::tracker::input::{InputMonitor, create_input_monitor};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStats {
    pub idle_seconds:      i64,
    pub break_seconds:     i64,
    pub keystroke_count:   i64,
    pub mouse_click_count: i64,
    pub mouse_move_count:  i64,
}

impl SessionStats {
    pub fn zero() -> Self {
        Self {
            idle_seconds: 0, break_seconds: 0,
            keystroke_count: 0, mouse_click_count: 0, mouse_move_count: 0,
        }
    }
}

fn emit_stats(app: &tauri::AppHandle, stats: &SessionStats) {
    if let Err(e) = app.emit("session-stats-update", stats) {
        log::warn!("[STATS] Emit failed: {}", e);
    }
}

pub fn get_timestamps() -> (String, String, String) {
    (
        TimestampManager::org_timestamp(),
        TimestampManager::aps_timestamp(),
        TimestampManager::get_org_timezone(),
    )
}

// ─────────────────────────────────────────────────────────────
// INACTIVITY STATE
//
// Single shared struct that tracks ONE inactivity period at a time.
// All writes to user_inactivity go through write_inactivity().
// write_inactivity() has a dedup guard — it checks if the exact same
// (start_time, end_time) row already exists before inserting.
//
// This means even if the code accidentally calls write_inactivity
// twice for the same period, the DB gets only ONE row.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InactivityReason {
    /// No keyboard / mouse input for X seconds
    Idle,
    /// OS screen-lock event received (Windows: WTS_SESSION_LOCK)
    ScreenLock,
    /// App was closed / crashed and restarted (startup recovery)
    SystemOffline,
}

impl InactivityReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            InactivityReason::Idle          => "Unavailability",
            InactivityReason::ScreenLock    => "Screen locked",
            InactivityReason::SystemOffline => "System offline / lock / sleep",
        }
    }
}

#[derive(Debug, Default)]
pub struct InactivityState {
    pub active:    bool,
    pub start:     Option<DateTime<Utc>>,
    pub confirmed: bool,
    pub reason:    Option<InactivityReason>,
    /// True once write_inactivity() has inserted this period.
    /// Prevents the startup path from writing the same period again.
    pub written:   bool,
}

impl InactivityState {
    /// Open a new inactivity period.
    /// If one is already open, only upgrade Idle → ScreenLock (never open twice).
    pub fn begin(&mut self, when: DateTime<Utc>, reason: InactivityReason) {
        if self.active {
            // Upgrade reason if we get a stronger signal
            if reason == InactivityReason::ScreenLock
                && self.reason != Some(InactivityReason::ScreenLock)
            {
                self.reason = Some(InactivityReason::ScreenLock);
                log::debug!("[INACTIVITY] Reason upgraded to ScreenLock");
            }
            return; // already tracking — don't reset start time
        }
        *self = InactivityState {
            active:    true,
            start:     Some(when),
            confirmed: false,
            reason:    Some(reason.clone()),
            written:   false,
        };
        log::info!("[INACTIVITY] Period opened at {} ({:?})", when, reason);
    }

    pub fn confirm(&mut self) {
        if !self.confirmed {
            self.confirmed = true;
            log::info!("[INACTIVITY] Period confirmed (threshold reached)");
        }
    }

    /// Close the period and return data for a DB write, or None if:
    ///  - not active
    ///  - never confirmed (too short)
    ///  - already written
    pub fn close(&mut self, now: DateTime<Utc>) -> Option<PeriodData> {
        if !self.active {
            return None;
        }

        let start     = self.start.unwrap_or(now);
        let confirmed = self.confirmed;
        let written   = self.written;
        let reason    = self.reason.clone();

        // Always reset state
        *self = InactivityState::default();

        if !confirmed {
            log::debug!("[INACTIVITY] Period closed — never confirmed, not recording");
            return None;
        }
        if written {
            log::debug!("[INACTIVITY] Period closed — already written, skipping");
            return None;
        }

        let dur = now.signed_duration_since(start).num_seconds().max(0);
        Some(PeriodData { start, end: now, duration_sec: dur, reason })
    }
}

pub struct PeriodData {
    pub start:        DateTime<Utc>,
    pub end:          DateTime<Utc>,
    pub duration_sec: i64,
    pub reason:       Option<InactivityReason>,
}

/// Shared handle so commands.rs can read/close the period on startup.
pub type SharedInactivityState = Arc<Mutex<InactivityState>>;

// ─────────────────────────────────────────────────────────────
// ACTIVITY TRACKER
// ─────────────────────────────────────────────────────────────

pub struct ActivityTracker {
    _config:         TrackerConfig,
    running:         Arc<AtomicBool>,
    event_tx:        Sender<TrackEvent>,
    _storage_writer: StorageWriter,
    _idle_detector:  Arc<Mutex<Box<dyn IdleDetector>>>,
    input_monitor:   Arc<Mutex<Box<dyn InputMonitor>>>,
    worker_handle:   Option<thread::JoinHandle<()>>,
    /// Exposed to commands.rs for startup-path recovery only.
    pub inactivity:  SharedInactivityState,
}

impl ActivityTracker {
    pub fn new(
        db:            Arc<DbManager>,
        config:        Option<TrackerConfig>,
        idle_detector: Option<Box<dyn IdleDetector>>,
        app_handle:    tauri::AppHandle, 
    ) -> crate::Result<Self> {
        let config        = config.unwrap_or_default();
        let idle_detector = idle_detector.unwrap_or_else(|| create_idle_detector());
        let idle_detector = Arc::new(Mutex::new(idle_detector));
        let persister     = DbActivityPersister::new(Arc::clone(&db));
        let storage       = StorageWriter::new(persister);
        let (tx, rx)      = unbounded::<TrackEvent>();
        let running       = Arc::new(AtomicBool::new(true));
        let inactivity: SharedInactivityState =
            Arc::new(Mutex::new(InactivityState::default()));

        // Start input monitor
        let input_monitor = create_input_monitor(tx.clone(), Arc::clone(&running));
        let input_monitor = Arc::new(Mutex::new(input_monitor));
        {
            let mut m = input_monitor.lock().unwrap();
            m.start().map_err(|e| crate::Error::WorkerError(
                format!("Failed to start input monitor: {}", e)
            ))?;
        }

        // Spawn worker thread
        let worker_handle = {
            let cfg       = config.clone();
            let storage2  = storage.clone();
            let idle2     = Arc::clone(&idle_detector);
            let running2  = Arc::clone(&running);
            let db_weak   = Arc::downgrade(&db);
            let inact2    = Arc::clone(&inactivity);

            thread::spawn(move || {
                run_worker(rx, storage2, idle2, running2, cfg, db_weak, inact2, app_handle);
            })
        };

        log::info!("[TRACKER] Started");

        Ok(Self {
            _config: config,
            running,
            event_tx: tx,
            _storage_writer: storage,
            _idle_detector: idle_detector,
            input_monitor,
            worker_handle: Some(worker_handle),
            inactivity,
        })
    }

    pub fn record_key(&self)         -> crate::Result<()> { Ok(()) }
    pub fn record_mouse_move(&self)  -> crate::Result<()> { Ok(()) }
    pub fn record_mouse_click(&self) -> crate::Result<()> { Ok(()) }

    pub fn stop(&mut self) -> crate::Result<()> {
        log::info!("[TRACKER] Stopping…");
        {
            let mut m = self.input_monitor.lock().unwrap();
            m.stop().map_err(|e| crate::Error::WorkerError(
                format!("Failed to stop input monitor: {}", e)
            ))?;
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        log::info!("[TRACKER] ✅ Stopped");
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn running_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn idle_detector(&self) -> Arc<Mutex<Box<dyn IdleDetector>>> {
        Arc::clone(&self._idle_detector)
    }

    pub fn input_monitor(&self) -> Arc<Mutex<Box<dyn InputMonitor>>> {
        Arc::clone(&self.input_monitor)
    }
}

impl Drop for ActivityTracker {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

// ─────────────────────────────────────────────────────────────
// WORKER
// ─────────────────────────────────────────────────────────────

fn run_worker(
    event_rx:   Receiver<TrackEvent>,
    storage:    StorageWriter,
    idle_det:   Arc<Mutex<Box<dyn IdleDetector>>>,
    running:    Arc<AtomicBool>,
    config:     TrackerConfig,
    db_weak:    std::sync::Weak<DbManager>,
    inactivity: SharedInactivityState,
    app:        tauri::AppHandle,   // ← new parameter
) {
    let mut aggregator          = MinuteAggregator::new(config.summary_window_minutes);
    let mut last_idle_check     = Instant::now();
    let idle_check_interval     = StdDuration::from_secs(1);
    let recv_timeout            = StdDuration::from_millis(100);
    let idle_activity_threshold = IDLE_ACTIVITY_THRESHOLD;
    let break_threshold         = IDLE_BREAK_THRESHOLD;

    // Track last emitted values — only emit when something changes
    let mut last_emitted = SessionStats::zero();
    // Track cumulative idle for this session (incremented each second)
    let mut session_idle_sec: i64 = 0;
    // Keystroke/click counts from current minute (reset on minute flush)
    let mut total_keystrokes:  i64 = 0;
    let mut total_clicks:      i64 = 0;
    let mut total_moves:       i64 = 0;

    log::info!("[WORKER] Started — idle={}s break={}s",
        idle_activity_threshold.as_secs(), break_threshold.as_secs());

    while running.load(Ordering::Acquire) {

        // ── Drain event queue ─────────────────────────────────────────
        let mut had_activity    = false;
        let mut new_keystrokes  = 0i64;
        let mut new_clicks      = 0i64;
        let mut new_moves       = 0i64;

        loop {
            match event_rx.recv_timeout(recv_timeout) {
                Ok(ev) => {
                    match ev {
                        TrackEvent::Key        => { had_activity = true; new_keystrokes += 1; }
                        TrackEvent::MouseClick => { had_activity = true; new_clicks     += 1; }
                        TrackEvent::MouseMove  => { had_activity = true; new_moves      += 1; }
                        TrackEvent::IdleTick   => {}
                    }
                    aggregator.add_event(ev);
                }
                Err(_) => break,
            }
        }

        if had_activity {
            total_keystrokes += new_keystrokes;
            total_clicks     += new_clicks;
            total_moves      += new_moves;

            if let Ok(mut det) = idle_det.lock() {
                det.record_activity();
            }
        }

        // ── Idle check (1 Hz) ─────────────────────────────────────────
        if last_idle_check.elapsed() >= idle_check_interval {
            last_idle_check = Instant::now();

            let idle_time = idle_det.lock()
                .map(|d| d.get_idle_time())
                .unwrap_or(StdDuration::ZERO);

            let user_is_idle = idle_time >= idle_activity_threshold;

            if user_is_idle {
                let inferred_start = Utc::now()
                    - chrono::Duration::seconds(idle_time.as_secs() as i64);

                let mut state = inactivity.lock().unwrap();
                state.begin(inferred_start, InactivityReason::Idle);

                if idle_time >= break_threshold {
                    state.confirm();
                }
                drop(state);

                aggregator.add_event(TrackEvent::IdleTick);

                // Increment our local idle counter (1 second per check)
                session_idle_sec += 1;

            } else if had_activity {
                // User became active — close any open inactivity period
                let now = Utc::now();
                let period = inactivity.lock().unwrap().close(now);
                if let Some(p) = period {
                    if let Some(db) = db_weak.upgrade() {
                        write_inactivity(&db, &p);
                    }
                }
            }

            // ── Emit stats update ONLY if values changed ──────────────
            // This is the key: we compare before emitting.
            // No change = no event = zero CPU for JS.
            let break_sec = fetch_break_seconds_fast(&db_weak);

            let current = SessionStats {
                idle_seconds:      session_idle_sec,
                break_seconds:     break_sec,
                keystroke_count:   total_keystrokes,
                mouse_click_count: total_clicks,
                mouse_move_count:  total_moves,
            };

            let changed =
                current.idle_seconds      != last_emitted.idle_seconds      ||
                current.break_seconds     != last_emitted.break_seconds      ||
                current.keystroke_count   != last_emitted.keystroke_count    ||
                current.mouse_click_count != last_emitted.mouse_click_count;
                // Note: mouse_move_count excluded — moves are too frequent,
                // not worth triggering a UI re-render every second

            if changed {
                emit_stats(&app, &current);
                last_emitted = SessionStats {
                    idle_seconds:      current.idle_seconds,
                    break_seconds:     current.break_seconds,
                    keystroke_count:   current.keystroke_count,
                    mouse_click_count: current.mouse_click_count,
                    mouse_move_count:  current.mouse_move_count,
                };
            }
        }

        // ── Minute flush ──────────────────────────────────────────────
        if aggregator.should_flush() {
            if let Some(minute) = aggregator.flush() {
                log::info!(
                    "[MINUTE] {} keys={} moves={} clicks={} idle={}s",
                    minute.minute_start,
                    minute.keystroke_count,
                    minute.mouse_move_count,
                    minute.mouse_click_count,
                    minute.idle_seconds
                );

                if let Ok(id) = storage.insert_minute(minute.clone()) {
                    aggregator.store_minute_with_id(id, minute);
                    let recent = aggregator.get_recent_for_summary();
                    if recent.len() >= config.summary_window_minutes {
                        if let Some(summary) = SummaryGenerator::generate(&recent) {
                            let ids = summary.minute_ids.clone();
                            if storage.insert_summary(summary).is_ok()
                                && storage.delete_minutes(ids.clone()).is_ok()
                            {
                                aggregator.clear_processed_minutes(&ids);
                            }
                        }
                    }
                }
            }
        }
    } // while

    // ── Shutdown ──────────────────────────────────────────────────────
    let now = Utc::now();
    let period = inactivity.lock().unwrap().close(now);
    if let Some(p) = period {
        if let Some(db) = db_weak.upgrade() {
            write_inactivity(&db, &p);
        }
    }

    log::info!("[WORKER] Stopped");
}

// ─────────────────────────────────────────────────────────────
// DB WRITE — the ONE place that inserts into user_inactivity
//
// Has a dedup guard: checks (start_time, end_time) before inserting.
// Even if called twice with identical data, only 1 row is written.
// ─────────────────────────────────────────────────────────────

pub fn write_inactivity(db: &Arc<DbManager>, period: &PeriodData) {
    if period.duration_sec <= 0 {
        return;
    }

    let reason_str = period.reason.as_ref()
        .map(|r| r.as_str())
        .unwrap_or("System offline / lock / sleep");

    let (org_ts, aps_ts, tz) = get_timestamps();
    let start_str = TimestampManager::convert_to_org_time(period.start);
    let end_str   = TimestampManager::convert_to_org_time(period.end);

    let conn = match db.conn.lock() {
        Ok(c)  => c,
        Err(e) => { log::error!("[INACTIVITY] Lock error: {}", e); return; }
    };

    // ── Dedup guard: if exact same period exists, skip ────────────────
    let already_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM user_inactivity
         WHERE inactive_start_time = ?1
           AND ABS(strftime('%s', inactive_end_time) - strftime('%s', ?2)) < 60",
        rusqlite::params![start_str, end_str],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if already_exists {
        log::warn!(
            "[INACTIVITY] Dedup: skipping {} → {} — row already exists",
            start_str, end_str
        );
        return;
    }

    let mic = MicrophoneDetector::is_microphone_active() as i32;

    match conn.execute(
        "INSERT INTO user_inactivity
         (inactive_start_time, inactive_end_time, inactivity_by, duration,
          is_microphone_in_use,
          created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            start_str, end_str, reason_str, period.duration_sec,
            mic,
            org_ts, org_ts, aps_ts, aps_ts, tz
        ],
    ) {
        Ok(_) => log::info!(
            "[INACTIVITY] ✅ '{}' {} → {} ({}s)",
            reason_str, start_str, end_str, period.duration_sec
        ),
        Err(e) => log::error!("[INACTIVITY] Write failed: {}", e),
    }
}

// ─────────────────────────────────────────────────────────────
// PUBLIC HELPER — used by commands.rs startup path
// ─────────────────────────────────────────────────────────────

pub fn make_period(
    start:  DateTime<Utc>,
    end:    DateTime<Utc>,
    reason: Option<InactivityReason>,
) -> PeriodData {
    PeriodData {
        start,
        end,
        duration_sec: end.signed_duration_since(start).num_seconds().max(0),
        reason,
    }
}

fn fetch_break_seconds_fast(db_weak: &std::sync::Weak<DbManager>) -> i64 {
    let Some(db) = db_weak.upgrade() else { return 0; };
    let Ok(conn) = db.conn.lock() else { return 0; };

    // Get the current session's checkin_time first
    let checkin: Option<String> = conn.query_row(
        "SELECT checkin_time FROM user_checkin
         WHERE checkout_time IS NULL ORDER BY id DESC LIMIT 1",
        [], |r| r.get(0),
    ).unwrap_or(None);

    let Some(checkin) = checkin else { return 0; };

    conn.query_row(
        // strftime arithmetic is handled entirely in SQLite — no Rust math
        "SELECT COALESCE(SUM(
            strftime('%s', breakout_time) - strftime('%s', breakin_time)
         ), 0)
         FROM user_breaks
         WHERE breakin_time  >= ?1
           AND breakout_time IS NOT NULL",
        [&checkin],
        |r| r.get::<_, i64>(0),
    ).unwrap_or(0)
}