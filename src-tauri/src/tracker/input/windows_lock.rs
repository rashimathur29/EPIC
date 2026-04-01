#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crossbeam_channel::Sender;
use windows::Win32::Foundation::*;
use windows::Win32::System::WindowsProgramming::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result as WinResult;

use crate::tracker::events::TrackEvent;
use crate::db::core::DbManager;

// Global (ugly but necessary for Windows callback)
static mut LOCK_STATE_SENDER: Option<Sender<bool>> = None; // true = locked, false = unlocked

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_WTSSESSION_CHANGE {
        match wparam.0 as u32 {
            WTS_SESSION_LOCK => {
                log::info!("[WINDOWS-LOCK] Screen locked (Win+L or policy)");
                if let Some(tx) = &LOCK_STATE_SENDER {
                    let _ = tx.send(true);
                }
            }
            WTS_SESSION_UNLOCK => {
                log::info!("[WINDOWS-LOCK] Screen unlocked");
                if let Some(tx) = &LOCK_STATE_SENDER {
                    let _ = tx.send(false);
                }
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub fn start_windows_lock_monitor(
    event_tx: Sender<TrackEvent>,
    running: Arc<AtomicBool>,
    db: Arc<DbManager>,
    inactivity_start: Arc<Mutex<Option<DateTime<Utc>>>>,
) {
    thread::spawn(move || {
        unsafe {
            // Create message-only window
            let class_name = w!("MessageOnlyWindowClass");
            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: class_name,
                ..Default::default()
            };

            RegisterClassW(&wnd_class).unwrap();

            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE,
                class_name,
                None,
                WS_DISABLED,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            ).unwrap();

            // Register for session notifications
            let result = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_ALL_SESSIONS);
            if result.is_err() {
                log::error!("[WINDOWS-LOCK] WTSRegisterSessionNotification failed: {:?}", result.err());
                return;
            }

            // Channel to receive lock/unlock from callback
            let (tx, rx) = crossbeam_channel::unbounded::<bool>();
            LOCK_STATE_SENDER = Some(tx);

            log::info!("[WINDOWS-LOCK] Lock monitoring started");

            while running.load(Ordering::Relaxed) {
                let mut msg = MSG::default();
                if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Check channel for lock/unlock events
                while let Ok(is_locked) = rx.try_recv() {
                    let mut start_guard = inactivity_start.lock().unwrap();

                    if is_locked {
                        // Screen just locked → start period
                        if start_guard.is_none() {
                            *start_guard = Some(Utc::now());
                            log::info!("[LOCK] Windows lock detected → starting inactivity");
                        }
                    } else {
                        // Screen unlocked → end period
                        if let Some(start) = *start_guard {
                            let end = Utc::now();
                            let duration_sec = (end - start).num_seconds().max(0) as u64;

                            if duration_sec >= 90 {
                                if let Err(e) = save_lock_period(&db, start, end, duration_sec) {
                                    log::error!("[LOCK] Failed to save lock period: {}", e);
                                } else {
                                    log::info!("[LOCK] Saved lock period: {} seconds", duration_sec);
                                }
                            }

                            *start_guard = None;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(50));
            }

            let _ = WTSUnRegisterSessionNotification(hwnd);
            log::info!("[WINDOWS-LOCK] Monitoring stopped");
        }
    });
}