use super::monitor::InputMonitor;
use crate::tracker::events::TrackEvent;
use evtx::EvtxParser;
use std::path::Path;
use chrono::{DateTime, Utc};
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Global state for hook callbacks (needed because Windows hooks require static callbacks)
static mut GLOBAL_EVENT_TX: Option<Sender<TrackEvent>> = None;
static mut GLOBAL_RUNNING: Option<Arc<AtomicBool>> = None;

// Atomic throttling timestamps (thread-safe, no leaks)
static LAST_MOUSE_MOVE_MS: AtomicU64 = AtomicU64::new(0);
static LAST_KEY_MS: AtomicU64 = AtomicU64::new(0);

// Statistics for monitoring (detect issues)
static TOTAL_EVENTS_RECEIVED: AtomicU64 = AtomicU64::new(0);
static TOTAL_EVENTS_THROTTLED: AtomicU64 = AtomicU64::new(0);
static TOTAL_EVENTS_SENT: AtomicU64 = AtomicU64::new(0);

// Throttle configuration (adjust these for performance vs accuracy)
const MOUSE_MOVE_THROTTLE_MS: u64 = 100;  // 10 events/sec max
const KEY_THROTTLE_MS: u64 = 50;          // 20 events/sec max (handles key repeat)
const MAX_HOOK_RETRIES: u32 = 3;          // Retry hook installation on failure
const HOOK_RETRY_DELAY_MS: u64 = 500;    // Wait 500ms between retries

pub struct WindowsInputMonitor {
    event_tx: Sender<TrackEvent>,
    running: Arc<AtomicBool>,
    hook_thread: Option<thread::JoinHandle<()>>,
}

impl WindowsInputMonitor {
    pub fn new(event_tx: Sender<TrackEvent>, running: Arc<AtomicBool>) -> Self {
        Self {
            event_tx,
            running,
            hook_thread: None,
        }
    }
    
    /// Get throttling statistics for monitoring
    pub fn get_stats() -> (u64, u64, u64) {
        let received = TOTAL_EVENTS_RECEIVED.load(Ordering::Relaxed);
        let throttled = TOTAL_EVENTS_THROTTLED.load(Ordering::Relaxed);
        let sent = TOTAL_EVENTS_SENT.load(Ordering::Relaxed);
        (received, throttled, sent)
    }
    
    /// Reset statistics
    pub fn reset_stats() {
        TOTAL_EVENTS_RECEIVED.store(0, Ordering::Relaxed);
        TOTAL_EVENTS_THROTTLED.store(0, Ordering::Relaxed);
        TOTAL_EVENTS_SENT.store(0, Ordering::Relaxed);
    }
}

impl InputMonitor for WindowsInputMonitor {
    fn start(&mut self) -> Result<(), String> {
        if self.is_running() {
            log::warn!("[HOOKS] Input monitor already running");
            return Err("Input monitor already running".to_string());
        }
        
        // Clean up any existing thread
        if self.hook_thread.is_some() {
            log::warn!("[HOOKS] Cleaning up existing hook thread");
            self.hook_thread = None;
        }

        let event_tx = self.event_tx.clone();
        let running = Arc::clone(&self.running);
        
        // Reset throttle timestamps and statistics
        LAST_MOUSE_MOVE_MS.store(0, Ordering::Relaxed);
        LAST_KEY_MS.store(0, Ordering::Relaxed);
        Self::reset_stats();
        
        log::info!("[HOOKS] Starting Windows input hooks with retry logic...");
        
        // Channel to receive hook installation result
        let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
        
        let handle = thread::spawn(move || {
            // Set running flag after thread starts
            running.store(true, Ordering::SeqCst);
            // Store in global state (required for Windows hook callbacks)
            unsafe {
                GLOBAL_EVENT_TX = Some(event_tx);
                GLOBAL_RUNNING = Some(running.clone());
            }
            
            unsafe {
                let mut retry_count = 0;
                let mut kb_hook = HHOOK::default();
                let mut mouse_hook = HHOOK::default();
                
                // Retry loop for hook installation
                while retry_count < MAX_HOOK_RETRIES {
                    // Install keyboard hook with retry
                    kb_hook = match SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(keyboard_hook_callback),
                        None,
                        0,
                    ) {
                        Ok(hook) => {
                            log::info!("[HOOKS] Keyboard hook installed successfully");
                            hook
                        }
                        Err(e) => {
                            retry_count += 1;
                            log::error!("[HOOKS] Failed to install keyboard hook (attempt {}/{}): {:?}", 
                                       retry_count, MAX_HOOK_RETRIES, e);
                            
                            if retry_count < MAX_HOOK_RETRIES {
                                log::info!("[HOOKS] Retrying in {}ms...", HOOK_RETRY_DELAY_MS);
                                thread::sleep(Duration::from_millis(HOOK_RETRY_DELAY_MS));
                                continue;
                            } else {
                                log::error!("[HOOKS] CRITICAL: Maximum retries reached for keyboard hook");
                                let _ = result_tx.send(Err("Failed to install keyboard hook after 3 attempts. Another application may be using system hooks.".to_string()));
                                GLOBAL_EVENT_TX = None;
                                GLOBAL_RUNNING = None;
                                return;
                            }
                        }
                    };
                    
                    // Install mouse hook with retry
                    mouse_hook = match SetWindowsHookExW(
                        WH_MOUSE_LL,
                        Some(mouse_hook_callback),
                        None,
                        0,
                    ) {
                        Ok(hook) => {
                            log::info!("[HOOKS] Mouse hook installed successfully");
                            hook
                        }
                        Err(e) => {
                            retry_count += 1;
                            log::error!("[HOOKS] Failed to install mouse hook (attempt {}/{}): {:?}", 
                                       retry_count, MAX_HOOK_RETRIES, e);
                            
                            // Clean up keyboard hook before retrying
                            if !kb_hook.is_invalid() {
                                let _ = UnhookWindowsHookEx(kb_hook);
                            }
                            
                            if retry_count < MAX_HOOK_RETRIES {
                                log::info!("[HOOKS] Retrying in {}ms...", HOOK_RETRY_DELAY_MS);
                                thread::sleep(Duration::from_millis(HOOK_RETRY_DELAY_MS));
                                continue;
                            } else {
                                log::error!("[HOOKS] CRITICAL: Maximum retries reached for mouse hook");
                                let _ = result_tx.send(Err("Failed to install mouse hook after 3 attempts. Another application may be using system hooks.".to_string()));
                                GLOBAL_EVENT_TX = None;
                                GLOBAL_RUNNING = None;
                                return;
                            }
                        }
                    };
                    
                    // Both hooks installed successfully
                    break;
                }

                if kb_hook.is_invalid() || mouse_hook.is_invalid() {
                    log::error!("[HOOKS] CRITICAL: Invalid hook handles after installation");
                    let _ = result_tx.send(Err("Invalid hook handles. System may not support low-level hooks.".to_string()));
                    GLOBAL_EVENT_TX = None;
                    GLOBAL_RUNNING = None;
                    return;
                }

                log::info!("[HOOKS] ✅ All hooks installed (throttle: mouse={}ms, key={}ms)", 
                         MOUSE_MOVE_THROTTLE_MS, KEY_THROTTLE_MS);
                log::info!("[HOOKS] Event handling: Key+SysKey, Mouse moves/clicks/wheel");
                
                // Signal success to main thread
                let _ = result_tx.send(Ok(()));
                
                // Simple and efficient message loop
                let mut msg = MSG::default();
                let mut idle_iterations = 0;
                let mut stats_log_interval = 0;
                
                while running.load(Ordering::Relaxed) {
                    // Non-blocking peek for messages
                    if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        idle_iterations = 0;
                        
                        if msg.message == WM_QUIT {
                            log::info!("[HOOKS] WM_QUIT received, stopping message loop");
                            break;
                        }
                        
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    } else {
                        // No messages - sleep to save CPU
                        idle_iterations += 1;
                        
                        if idle_iterations > 100 {
                            // Been idle for a while, sleep longer
                            thread::sleep(Duration::from_millis(10));
                        } else {
                            // Active period, shorter sleep
                            thread::sleep(Duration::from_millis(1));
                        }
                        
                        // Log statistics every ~60 seconds (60000 iterations @ 1ms)
                        stats_log_interval += 1;
                        if stats_log_interval >= 60000 {
                            stats_log_interval = 0;
                            let (received, throttled, sent) = Self::get_stats();
                            if received > 0 {
                                let throttle_pct = (throttled as f64 / received as f64 * 100.0) as u32;
                                log::debug!("[HOOKS] Stats: received={}, sent={}, throttled={}% ({})", 
                                          received, sent, throttle_pct, throttled);
                            }
                        }
                    }
                }
                
                log::info!("[HOOKS] Message loop exited, cleaning up...");
                
                // Log final statistics
                let (received, throttled, sent) = Self::get_stats();
                if received > 0 {
                    let throttle_pct = (throttled as f64 / received as f64 * 100.0) as u32;
                    log::info!("[HOOKS] Final stats: received={}, sent={}, throttled={}%", 
                              received, sent, throttle_pct);
                }
                
                // Uninstall hooks
                if !kb_hook.is_invalid() {
                    match UnhookWindowsHookEx(kb_hook) {
                        Ok(_) => log::debug!("[HOOKS] Keyboard hook removed"),
                        Err(e) => log::warn!("[HOOKS] Failed to remove keyboard hook: {:?}", e),
                    }
                }
                if !mouse_hook.is_invalid() {
                    match UnhookWindowsHookEx(mouse_hook) {
                        Ok(_) => log::debug!("[HOOKS] Mouse hook removed"),
                        Err(e) => log::warn!("[HOOKS] Failed to remove mouse hook: {:?}", e),
                    }
                }
                
                // Clear global state (prevent memory leaks)
                GLOBAL_EVENT_TX = None;
                GLOBAL_RUNNING = None;
                
                log::info!("[HOOKS] ✅ Cleanup complete");
            }
        });
        
        self.hook_thread = Some(handle);
        
        // Wait for hook installation result (with timeout)
        match result_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                log::info!("[HOOKS] Hook installation confirmed");
                Ok(())
            }
            Ok(Err(e)) => {
                log::error!("[HOOKS] Hook installation failed: {}", e);
                self.running.store(false, Ordering::SeqCst);
                self.hook_thread = None;
                Err(e)
            }
            Err(_) => {
                log::error!("[HOOKS] Hook installation timeout");
                self.running.store(false, Ordering::SeqCst);
                self.hook_thread = None;
                Err("Hook installation timeout after 5 seconds".to_string())
            }
        }
    }
    
    fn stop(&mut self) -> Result<(), String> {
        if !self.is_running() {
            return Ok(());
        }
        
        log::info!("[HOOKS] Stopping Windows input monitor...");
        
        // Signal stop
        self.running.store(false, Ordering::SeqCst);
        
        // Wake up message loop
        unsafe {
            PostQuitMessage(0);
        }
        
        // Wait for thread to finish (with timeout)
        if let Some(handle) = self.hook_thread.take() {
            match handle.join() {
                Ok(_) => log::info!("[HOOKS] ✅ Hook thread stopped cleanly"),
                Err(e) => log::warn!("[HOOKS] Hook thread join error: {:?}", e),
            }
        }
        
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) && self.hook_thread.is_some()
    }
}

impl Drop for WindowsInputMonitor {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

// Get current time in milliseconds (no allocation, no leaks)
#[inline]
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

// Keyboard hook callback with enhanced logging and error handling
unsafe extern "system" fn keyboard_hook_callback(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Always call next hook, even if we error
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    
    TOTAL_EVENTS_RECEIVED.fetch_add(1, Ordering::Relaxed);
    
    // Handle both regular keys and system keys (Alt, Ctrl, etc.)
    let is_key_down = wparam.0 == WM_KEYDOWN as usize 
                   || wparam.0 == WM_SYSKEYDOWN as usize;
    
    if is_key_down {
        // THROTTLE: Prevent key repeat spam
        let now_ms = current_time_ms();
        let last_ms = LAST_KEY_MS.load(Ordering::Relaxed);
        
        if now_ms.saturating_sub(last_ms) >= KEY_THROTTLE_MS {
            if let Some(tx) = &GLOBAL_EVENT_TX {
                // Use try_send to never block the hook
                match tx.try_send(TrackEvent::Key) {
                    Ok(_) => {
                        TOTAL_EVENTS_SENT.fetch_add(1, Ordering::Relaxed);
                        LAST_KEY_MS.store(now_ms, Ordering::Relaxed);
                        log::trace!("[HOOKS] Key event sent");
                    }
                    Err(e) => {
                        log::warn!("[HOOKS] Failed to send key event: {:?}", e);
                    }
                }
            }
        } else {
            TOTAL_EVENTS_THROTTLED.fetch_add(1, Ordering::Relaxed);
            log::trace!("[HOOKS] Key event throttled");
        }
    }
    
    CallNextHookEx(None, code, wparam, lparam)
}

// Mouse hook callback with enhanced logging and more event types
unsafe extern "system" fn mouse_hook_callback(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Always call next hook
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    
    TOTAL_EVENTS_RECEIVED.fetch_add(1, Ordering::Relaxed);
    
    if let Some(tx) = &GLOBAL_EVENT_TX {
        match wparam.0 as u32 {
            WM_MOUSEMOVE => {
                // CRITICAL THROTTLE: Only send mouse move every 100ms
                let now_ms = current_time_ms();
                let last_ms = LAST_MOUSE_MOVE_MS.load(Ordering::Relaxed);
                
                if now_ms.saturating_sub(last_ms) >= MOUSE_MOVE_THROTTLE_MS {
                    match tx.try_send(TrackEvent::MouseMove) {
                        Ok(_) => {
                            TOTAL_EVENTS_SENT.fetch_add(1, Ordering::Relaxed);
                            LAST_MOUSE_MOVE_MS.store(now_ms, Ordering::Relaxed);
                            log::trace!("[HOOKS] Mouse move event sent");
                        }
                        Err(e) => {
                            log::warn!("[HOOKS] Failed to send mouse move: {:?}", e);
                        }
                    }
                } else {
                    TOTAL_EVENTS_THROTTLED.fetch_add(1, Ordering::Relaxed);
                    log::trace!("[HOOKS] Mouse move throttled");
                }
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | 
            WM_XBUTTONDOWN => {
                // Always send clicks (they're infrequent)
                match tx.try_send(TrackEvent::MouseClick) {
                    Ok(_) => {
                        TOTAL_EVENTS_SENT.fetch_add(1, Ordering::Relaxed);
                        log::trace!("[HOOKS] Mouse click event sent");
                    }
                    Err(e) => {
                        log::warn!("[HOOKS] Failed to send mouse click: {:?}", e);
                    }
                }
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                // Send scroll as activity indicator
                match tx.try_send(TrackEvent::MouseClick) {
                    Ok(_) => {
                        TOTAL_EVENTS_SENT.fetch_add(1, Ordering::Relaxed);
                        log::trace!("[HOOKS] Mouse wheel event sent");
                    }
                    Err(e) => {
                        log::warn!("[HOOKS] Failed to send mouse wheel: {:?}", e);
                    }
                }
            }
            _ => {
                log::trace!("[HOOKS] Unhandled mouse event: {}", wparam.0);
            }
        }
    }
    
    CallNextHookEx(None, code, wparam, lparam)
}

pub fn get_last_break_start_time() -> Option<DateTime<Utc>> {
    let path = Path::new(r"C:\Windows\System32\winevt\Logs\System.evtx");
    if !path.exists() {
        log::warn!("[WINDOWS_EVENT] System.evtx not found");
        return None;
    }

    let mut parser = match EvtxParser::from_path(path) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[WINDOWS_EVENT] Failed to open System.evtx: {}", e);
            return None;
        }
    };

    let mut last_break: Option<DateTime<Utc>> = None;

    for record in parser.records_json_value() {
        if let Ok(rec) = record {
            let json_value = rec.data;  // <-- field, no ()

            if let Some(event) = json_value.get("Event") {
                if let Some(system) = event.get("System") {
                    let event_id: u16 = if let Some(id_val) = system.get("EventID") {
                        match id_val {
                            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0) as u16,
                            serde_json::Value::Object(obj) => obj
                                .get("#text")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u16)
                                .unwrap_or(0),
                            _ => 0,
                        }
                    } else {
                        0
                    };

                    if let Some(time_created) = system.get("TimeCreated") {
                        if let Some(sys_time) = time_created.get("@SystemTime") {
                            if let Some(time_str) = sys_time.as_str() {
                                if let Ok(parsed) = DateTime::parse_from_rfc3339(time_str) {
                                    let utc_time = parsed.with_timezone(&Utc);

                                    // Lock (4800) or sleep entry (42)
                                    if event_id == 4800 || event_id == 42 {
                                        if last_break.is_none() || utc_time > last_break.unwrap() {
                                            last_break = Some(utc_time);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    last_break
}