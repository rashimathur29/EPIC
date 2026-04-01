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
use std::time::{Duration as StdDuration, Instant};
use chrono::{Local, Utc, DateTime};
use crate::tracker::config::{IDLE_ACTIVITY_THRESHOLD, IDLE_BREAK_THRESHOLD};
use crate::tracker::audio::mic_detector::MicrophoneDetector;
use crate::timestamp::TimestampManager;

// Import the input monitor
use crate::tracker::input::{InputMonitor, create_input_monitor};

pub fn get_timestamps() -> (String, String, String) {
    let org_ts = TimestampManager::org_timestamp();
    let aps_ts = TimestampManager::aps_timestamp();
    let tz     = TimestampManager::get_org_timezone();
    (org_ts, aps_ts, tz)
}
pub struct ActivityTracker {
    _config: TrackerConfig,
    running: Arc<AtomicBool>,
    event_tx: Sender<TrackEvent>,
    _storage_writer: StorageWriter,
    _idle_detector: Arc<Mutex<Box<dyn IdleDetector>>>,
    input_monitor: Arc<Mutex<Box<dyn InputMonitor>>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}



impl ActivityTracker {

    pub fn new(
        db: Arc<DbManager>,
        config: Option<TrackerConfig>,
        idle_detector: Option<Box<dyn IdleDetector>>,
    ) -> crate::Result<Self> {
        let config = config.unwrap_or_default();
        
        let idle_detector = idle_detector.unwrap_or_else(|| create_idle_detector());
        let idle_detector = Arc::new(Mutex::new(idle_detector));
        
        let persister = DbActivityPersister::new(Arc::clone(&db));
        let storage_writer = StorageWriter::new(persister);
        
        let (event_tx, event_rx) = unbounded();
        let running = Arc::new(AtomicBool::new(true));
        
        // Create input monitor (system-level hooks)
        let input_monitor = create_input_monitor(event_tx.clone(), Arc::clone(&running));
        let input_monitor = Arc::new(Mutex::new(input_monitor));
        
        // Start the input monitor
        {
            let mut monitor = input_monitor.lock().unwrap();
            monitor.start()
                .map_err(|e| crate::Error::WorkerError(format!("Failed to start input monitor: {}", e)))?;
        }

        // NEW: Start unified inactivity tracker (180 seconds)
        start_unified_inactivity_tracker(
            Arc::clone(&running),
            Arc::clone(&idle_detector),
            Arc::clone(&db),
        );
        
        log::info!("[TRACKER] System-level input monitoring started");
        
        let worker_config = config.clone();
        let worker_storage = storage_writer.clone();
        let worker_idle = Arc::clone(&idle_detector);
        let worker_running = Arc::clone(&running);
        let db_weak = Arc::downgrade(&db);
        
        let handle = thread::spawn(move || {
            Self::run_worker(
                event_rx,
                worker_storage,
                worker_idle,
                worker_running,
                worker_config,
                db_weak,
            );
        });
        
        Ok(Self {
            _config: config,
            running,
            event_tx,
            _storage_writer: storage_writer,
            _idle_detector: idle_detector,
            input_monitor,
            worker_handle: Some(handle),
        })
    }
    
    fn run_worker(
        event_rx: Receiver<TrackEvent>,
        storage: StorageWriter,
        idle_detector: Arc<Mutex<Box<dyn IdleDetector>>>,
        running: Arc<AtomicBool>,
        config: TrackerConfig,
        db_weak: std::sync::Weak<DbManager>,
    ) {
        let mut aggregator = MinuteAggregator::new(config.summary_window_minutes);

        let idle_activity_threshold = IDLE_ACTIVITY_THRESHOLD; // e.g. 6s
        let break_threshold = IDLE_BREAK_THRESHOLD;           // e.g. 5 min

        let mut last_idle_check = Instant::now();
        let idle_check_interval = StdDuration::from_secs(1);

        let mut currently_idle = false;
        let mut idle_start_time: Option<DateTime<Utc>> = None;
        let mut break_confirmed = false;

        let recv_timeout = StdDuration::from_millis(100);
        let (org_ts, aps_ts, tz) = get_timestamps();

        log::info!(
            "[WORKER] Worker thread started (idle={}s, break={}s)",
            idle_activity_threshold.as_secs(),
            break_threshold.as_secs()
        );

        while running.load(Ordering::Acquire) {
            let mut had_activity = false;

            // -------------------- EVENT PROCESSING --------------------
            loop {
                match event_rx.recv_timeout(recv_timeout) {
                    Ok(event) => match event {
                        TrackEvent::Key | TrackEvent::MouseMove | TrackEvent::MouseClick => {
                            aggregator.add_event(event);
                            had_activity = true;
                        }
                        TrackEvent::IdleTick => {
                            aggregator.add_event(event);
                        }
                    },
                    Err(_) => break,
                }
            }

            // -------------------- REAL ACTIVITY --------------------
            if had_activity {
                if let Ok(mut detector) = idle_detector.lock() {
                    detector.record_activity();
                }

                /*if currently_idle {
                    log::debug!("[IDLE] User became ACTIVE");
                }*/
            }

            // -------------------- IDLE CHECK (1s) --------------------
            if last_idle_check.elapsed() >= idle_check_interval {
                last_idle_check = Instant::now();

                if let Ok(detector) = idle_detector.lock() {
                    let idle_time = detector.get_idle_time();

                    // -------- USER IS IDLE --------
                    if idle_time >= idle_activity_threshold {
                        if !currently_idle {
                            let now = Utc::now();
                            currently_idle = true;
                            idle_start_time = Some(now);
                            break_confirmed = false;

                            /*log::info!(
                                "[IDLE] User became IDLE at {} ({}s since last activity)",
                                now,
                                idle_time.as_secs()
                            );*/
                        }

                        // Count idle seconds
                        aggregator.add_event(TrackEvent::IdleTick);

                        // -------- BREAK CONFIRMATION (5 min rule) --------
                        if !break_confirmed && idle_time >= break_threshold {
                            break_confirmed = true;

                            if let Some(start) = idle_start_time {
                                log::info!(
                                    "[IDLE] Unavailability confirmed ({} min). Started at {}",
                                    break_threshold.as_secs() / 60,
                                    start
                                );
                            }
                        }
                    }
                    // -------- USER BECAME ACTIVE AGAIN --------
                    else {
                        if currently_idle {
                            let active_time = Utc::now();

                            if let Some(start_time) = idle_start_time {
                                let mic_in_use = MicrophoneDetector::is_microphone_active();
                                let mic_flag = if mic_in_use { 1 } else { 0 };
                                let duration = active_time.signed_duration_since(start_time);
                                let duration_seconds = duration.num_seconds().max(0);
                                

                                if break_confirmed {
                                    let start_str = TimestampManager::convert_to_org_time(start_time);
                                    let end_str   = TimestampManager::convert_to_org_time(active_time);

                                    if let Some(db) = db_weak.upgrade() {
                                        let conn = db.conn.lock().unwrap();
                                        let utc_now = Utc::now().to_rfc3339();
                                        let tz = Local::now().format("%Z").to_string();
                                        let _ = conn.execute(
                                            "INSERT INTO user_inactivity
                                            (inactive_start_time, inactive_end_time, inactivity_by, duration, is_microphone_in_use, created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
                                            VALUES (?1, ?2, 'Unavailability', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                                            rusqlite::params![
                                                start_str,
                                                end_str,
                                                duration_seconds,
                                                mic_flag,
                                                org_ts,
                                                org_ts,
                                                aps_ts,
                                                aps_ts,
                                                tz
                                            ],
                                        );

                                        log::info!(
                                            "[IDLE] Logged unavailability: {} seconds ({} → {})",
                                            duration_seconds,
                                            start_str,
                                            end_str
                                        );
                                    }
                                }
                            }
                        }

                        // Reset idle state
                        currently_idle = false;
                        idle_start_time = None;
                        break_confirmed = false;
                    }
                }
            }

            // -------------------- MINUTE FLUSH --------------------
            if aggregator.should_flush() {
                if let Some(minute_data) = aggregator.flush() {
                    log::info!(
                        "[MINUTE] Minute {}: keys={}, moves={}, clicks={}, idle={}s",
                        minute_data.minute_start,
                        minute_data.keystroke_count,
                        minute_data.mouse_move_count,
                        minute_data.mouse_click_count,
                        minute_data.idle_seconds
                    );

                    match storage.insert_minute(minute_data.clone()) {
                        Ok(id) => {
                            aggregator.store_minute_with_id(id, minute_data);

                            let recent_entries = aggregator.get_recent_for_summary();
                            if recent_entries.len() >= config.summary_window_minutes {
                                if let Some(summary) =
                                    SummaryGenerator::generate(&recent_entries)
                                {
                                    let minute_ids = summary.minute_ids.clone();

                                    log::info!(
                                        "[SUMMARY] Generating summary from {} minutes",
                                        recent_entries.len()
                                    );

                                    if storage.insert_summary(summary).is_ok() {
                                        if storage
                                            .delete_minutes(minute_ids.clone())
                                            .is_ok()
                                        {
                                            aggregator
                                                .clear_processed_minutes(&minute_ids);
                                            log::info!(
                                                "[SUMMARY] Cleaned up {} minute records",
                                                minute_ids.len()
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "[MINUTE] Failed to insert minute data: {}",
                                e
                            );
                        }
                    }
                }
            }
        }

        log::info!("[WORKER] Worker thread stopped");
    }

    
    // These methods are now obsolete since we use system hooks
    // But keep them for backward compatibility
    pub fn record_key(&self) -> crate::Result<()> {
        // System hooks handle this automatically
        Ok(())
    }
    
    pub fn record_mouse_move(&self) -> crate::Result<()> {
        // System hooks handle this automatically
        Ok(())
    }
    
    pub fn record_mouse_click(&self) -> crate::Result<()> {
        // System hooks handle this automatically
        Ok(())
    }
    
    pub fn stop(&mut self) -> crate::Result<()> {
        log::info!("[TRACKER] Stopping tracker...");
        
        // Stop input monitor first
        {
            let mut monitor = self.input_monitor.lock().unwrap();
            monitor.stop()
                .map_err(|e| {
                    log::error!("[TRACKER] Failed to stop input monitor: {}", e);
                    crate::Error::WorkerError(format!("Failed to stop input monitor: {}", e))
                })?;
        }
        
        // Then stop worker thread
        self.running.store(false, Ordering::Relaxed);
        
        if let Some(handle) = self.worker_handle.take() {
            handle.join()
                .map_err(|e| {
                    log::error!("[TRACKER] Worker thread join failed: {:?}", e);
                    crate::Error::WorkerError(format!("Join failed: {:?}", e))
                })?;
        }
        
        log::info!("[TRACKER] ✅ Tracker stopped successfully");
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

// ===================================================================
// NEW: Unified Inactivity Tracker (≥ 180 seconds) - Same on all OS
// ===================================================================
fn start_unified_inactivity_tracker(
    running: Arc<AtomicBool>,
    idle_detector: Arc<Mutex<Box<dyn IdleDetector>>>,
    db: Arc<DbManager>,
) {
    thread::spawn(move || {
        const THRESHOLD_SEC: u64 = 180;   // Change to 90 if you want 90 seconds

        let mut inactivity_start: Option<DateTime<Utc>> = None;

        while running.load(Ordering::Relaxed) {
            let idle = idle_detector.lock().unwrap().get_idle_time();

            if idle.as_secs() >= THRESHOLD_SEC {
                if inactivity_start.is_none() {
                    inactivity_start = Some(Utc::now());
                }
            } else if let Some(start) = inactivity_start.take() {
                let end = Utc::now();
                let duration_sec = (end - start).num_seconds().max(0) as u64;

                if duration_sec >= THRESHOLD_SEC {
                    let _ = save_inactivity_record(&db, start, end, duration_sec);
                }
            }

            thread::sleep(StdDuration::from_secs(5));
        }
    });
}

fn save_inactivity_record(
    db: &Arc<DbManager>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    duration_sec: u64,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn.lock().unwrap();
    
    let (org_ts, aps_ts, tz) = get_timestamps();
    let start_time = TimestampManager::convert_to_org_time(start);
    let end_time   = TimestampManager::convert_to_org_time(end);

    conn.execute(
        "INSERT INTO user_inactivity 
         (inactive_start_time, inactive_end_time, inactivity_by, duration, 
          created_at, updated_at, apscreatedatetime, apsupdatedatetime, timezone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            start_time, 
            end_time,  
            "inactive_or_locked",
            duration_sec as i64,
            org_ts,
            org_ts,
            aps_ts,
            aps_ts, 
            tz
        ],
    )?;
    Ok(())
}