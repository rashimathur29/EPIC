// ╔══════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/streaming/capture.rs                         ║
// ║  Cross-platform screen capture — single source of truth     ║
// ╚══════════════════════════════════════════════════════════════╝

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::thread;

use image::{ImageBuffer, Rgba};
use crossbeam_channel::Sender;

static IS_LOCKED: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────
// PUBLIC FRAME TYPE
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RawFrame {
    pub pixels: Arc<Vec<u8>>,
    pub width:  u32,
    pub height: u32,
    pub ts_ms:  u64,
}

// ─────────────────────────────────────────────────────────────
// INTERNAL RESULT
// ─────────────────────────────────────────────────────────────

struct Captured {
    pixels: Vec<u8>,
    width:  u32,
    height: u32,
}

// ─────────────────────────────────────────────────────────────
// CONFIG
// ─────────────────────────────────────────────────────────────

pub struct CaptureConfig {
    pub fps:    u32,
    pub width:  u32,
    pub height: u32,
}

// ─────────────────────────────────────────────────────────────
// SIMPLE BLACK LOCK FRAME (all black RGBA, no text/symbol)
// ─────────────────────────────────────────────────────────────

fn create_black_lock_frame(width: u32, height: u32) -> Vec<u8> {
    vec![0u8; (width * height * 4) as usize] // All black (R=0,G=0,B=0,A=255 implied)
}

// ─────────────────────────────────────────────────────────────
// CAPTURE ERROR CLASSIFICATION (unchanged)
// ─────────────────────────────────────────────────────────────

fn is_transient_dxgi_error(e: &str) -> bool {
    e.contains("0x80070005")
        || e.contains("0x887A0026")
        || e.contains("Access is denied")
        || e.contains("access denied")
        || e.contains("DXGI")
}

// ─────────────────────────────────────────────────────────────
// CAPTURE ATTEMPT
// ─────────────────────────────────────────────────────────────

fn try_capture() -> Result<Captured, bool> {
    #[cfg(target_os = "linux")]
    {
        let session  = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_lowercase();
        let wayland  = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        let is_wayland = session == "wayland" || !wayland.is_empty();
        if is_wayland {
            return capture_wayland_grim()
                .ok_or(false);
        }
    }

    capture_native()
}

fn capture_native() -> Result<Captured, bool> {
    use screenshots::Screen;

    thread_local! {
        static CACHED_SCREEN: std::cell::RefCell<Option<Screen>> = std::cell::RefCell::new(None);
    }

    CACHED_SCREEN.with(|cell| {
        // First: check and initialize cache (mutable borrow only if needed)
        if cell.borrow().is_none() {
            let screens = Screen::all().map_err(|e| {
                let es = e.to_string();
                log::debug!("[CAPTURE] Screen::all() failed: {}", es);
                is_transient_dxgi_error(&es)
            })?;

            let screen = screens.into_iter().next().ok_or(false)?;

            // Mutable borrow only here, short-lived
            *cell.borrow_mut() = Some(screen);
            log::debug!("[CAPTURE] Screen handle cached");
        }

        // Now immutable borrow for capture (separate borrow, safe)
        let screen_ref = cell.borrow();
        let screen = screen_ref.as_ref().unwrap();

        let result = screen.capture()
            .map(|img| Captured {
                pixels: img.rgba().to_vec(),
                width:  img.width(),
                height: img.height(),
            })
            .map_err(|e| {
                let es = e.to_string();
                log::warn!("[CAPTURE] screen.capture() failed: {}", es);
                let is_transient = is_transient_dxgi_error(&es);
                if is_transient {
                    // Clear cache on transient error (new mutable borrow, after immutable dropped)
                    drop(screen_ref); // explicitly drop immutable borrow first
                    *cell.borrow_mut() = None;
                    log::debug!("[CAPTURE] Cache cleared due to DXGI error");
                }
                is_transient
            });

        // Lock detection: black frame or transient error
        let is_error = result.is_err();
        let error_str = result.as_ref().err().map(|e| e.to_string()).unwrap_or_default();

        let is_black = if let Ok(ref cap) = result {
            cap.pixels.chunks(4).filter(|px| px[0] < 30 && px[1] < 30 && px[2] < 30).count()
                > (cap.pixels.len() / 4 * 95 / 100)
        } else {
            false
        };

        if is_black || (is_error && is_transient_dxgi_error(&error_str)) {
            if !IS_LOCKED.load(Ordering::Relaxed) {
                log::info!("[CAPTURE] Lock screen detected - sending black frame");
                IS_LOCKED.store(true, Ordering::Relaxed);
            }
        } else {
            if IS_LOCKED.load(Ordering::Relaxed) {
                log::info!("[CAPTURE] Normal capture resumed - lock mode off");
                IS_LOCKED.store(false, Ordering::Relaxed);
            }
        }

        result
    })
}

// ─────────────────────────────────────────────────────────────
// THE ONE CAPTURE THREAD (with lock handling)
// ─────────────────────────────────────────────────────────────

pub fn start_capture(
    cfg:       CaptureConfig,
    stream_tx: Option<Sender<RawFrame>>,
    record_tx: Option<Sender<RawFrame>>,
    running:   Arc<AtomicBool>,  
) {
    let w = cfg.width  & !1;
    let h = cfg.height & !1;

    let running2 = running.clone();

    thread::Builder::new()
        .name("epic-capture".to_string())
        .spawn(move || {
            lower_thread_priority();

            let interval      = Duration::from_millis(1000 / cfg.fps.max(1) as u64);
            let t_start       = Instant::now();
            let mut fail_streak    = 0u32;
            let mut transient_wait = Duration::from_millis(500);
            let mut black_checked  = false;

            log::info!("[CAPTURE] Started — {}fps {}×{}", cfg.fps, w, h);
            log_os_notes();

            while running2.load(Ordering::Relaxed) {
                let t0 = Instant::now();

                let raw = match try_capture() {
                    Ok(r) => {
                        fail_streak = 0;
                        transient_wait = Duration::from_millis(500);
                        r
                    }
                    Err(true) => {
                        // Transient error (likely lock) — send black frame instead of waiting long
                        log::debug!("[CAPTURE] Transient error during lock — sending black frame");

                        let black_pixels = vec![0u8; (w * h * 4) as usize]; // plain black

                        let frame = RawFrame {
                            pixels: Arc::new(black_pixels),
                            width:  w,
                            height: h,
                            ts_ms:  t_start.elapsed().as_millis() as u64,
                        };

                        if let Some(tx) = &stream_tx {
                            let _ = tx.try_send(frame.clone());
                        }
                        if let Some(tx) = &record_tx {
                            let _ = tx.try_send(frame);
                        }

                        // Short sleep only (keep 2 fps rhythm)
                        thread::sleep(Duration::from_millis(50)); // small delay to avoid spam

                        // Reset backoff (will try fresh capture next loop)
                        transient_wait = Duration::from_millis(500);
                        continue;
                    }
                    Err(false) => {
                        fail_streak += 1;
                        if fail_streak == 5 {
                            log::warn!("[CAPTURE] 5 consecutive failures...");
                        }
                        let wait = Duration::from_millis(500 * fail_streak.min(10) as u64);
                        thread::sleep(wait);
                        continue;
                    }
                };

                if !black_checked {
                    black_checked = true;
                    check_macos_black_frame(&raw.pixels);
                }

                let pixels: Vec<u8> = if raw.width == w && raw.height == h {
                    raw.pixels
                } else {
                    match ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(raw.width, raw.height, raw.pixels) {
                        Some(buf) => image::DynamicImage::ImageRgba8(buf)
                            .resize_exact(w, h, image::imageops::FilterType::Nearest)
                            .to_rgba8()
                            .into_raw(),
                        None => {
                            log::warn!("[CAPTURE] Bad buffer ({}×{})", raw.width, raw.height);
                            continue;
                        }
                    }
                };

                // Lock mode: override with black frame
                let final_pixels = if IS_LOCKED.load(Ordering::Relaxed) {
                    log::debug!("[CAPTURE] Sending black lock frame");
                    create_black_lock_frame(w, h)
                } else {
                    pixels
                };

                let frame = RawFrame {
                    pixels: Arc::new(final_pixels),
                    width:  w,
                    height: h,
                    ts_ms:  t_start.elapsed().as_millis() as u64,
                };

                if let Some(tx) = &stream_tx {
                    let _ = tx.try_send(frame.clone());
                }
                if let Some(tx) = &record_tx {
                    let _ = tx.try_send(frame);
                }

                let spent = t0.elapsed();
                if spent < interval {
                    thread::sleep(interval - spent);
                }
            }

            log::info!("[CAPTURE] Thread stopped");
        })
        .expect("Failed to spawn capture thread");
}



fn log_os_notes() {
    #[cfg(target_os = "windows")]
    log::info!(
        "[CAPTURE] Windows mode (DXGI). \
         UAC-elevated windows appear black (OS restriction). \
         After sleep/lock, capture recovers automatically within ~2 seconds."
    );

    #[cfg(target_os = "macos")]
    log::info!(
        "[CAPTURE] macOS mode (CoreGraphics). \
         Needs Screen Recording permission. \
         If frames are black: System Settings → Privacy → Screen Recording → enable this app."
    );

    #[cfg(target_os = "linux")]
    {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        if !wayland.is_empty() || session.to_lowercase() == "wayland" {
            log::info!("[CAPTURE] Linux Wayland mode — using grim.");
        } else {
            log::info!("[CAPTURE] Linux X11 mode — using XCB.");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// WAYLAND CAPTURE (Linux only)
// ─────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn capture_wayland_grim() -> Option<Captured> {
    use std::process::Command;
    static WARNED_NO_GRIM:   std::sync::Once = std::sync::Once::new();
    static WARNED_NO_PORTAL: std::sync::Once = std::sync::Once::new();

    let tmp = std::env::temp_dir().join("epic_cap.png");
    let result = Command::new("grim")
        .args(["-t", "png", "-l", "0", tmp.to_str()?])
        .output();

    match result {
        Err(_) => {
            WARNED_NO_GRIM.call_once(|| log::error!(
                "[CAPTURE] Wayland: `grim` not found.\n\
                 Fix: sudo apt install grim   (Ubuntu/Debian)\n\
                 Fix: sudo dnf install grim   (Fedora)\n\
                 Fix: sudo pacman -S grim     (Arch)"
            ));
            return None;
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("compositor doesn't support") || stderr.contains("portal") {
                WARNED_NO_PORTAL.call_once(|| log::error!(
                    "[CAPTURE] GNOME Wayland: install xdg-desktop-portal-gnome then re-login."
                ));
            } else {
                WARNED_NO_GRIM.call_once(|| log::error!("[CAPTURE] grim failed: {}", stderr.trim()));
            }
            return None;
        }
        Ok(_) => {}
    }

    let img = match image::open(&tmp) {
        Ok(i)  => i.to_rgba8(),
        Err(e) => { log::warn!("[CAPTURE] grim PNG load failed: {}", e); let _ = std::fs::remove_file(&tmp); return None; }
    };
    let (w, h) = (img.width(), img.height());
    let pixels = img.into_raw();
    let _ = std::fs::remove_file(&tmp);
    Some(Captured { pixels, width: w, height: h })
}

#[cfg(not(target_os = "linux"))]
fn capture_wayland_grim() -> Option<Captured> { None }

// ─────────────────────────────────────────────────────────────
// FREEBSD / UNIX FALLBACK
// ─────────────────────────────────────────────────────────────

#[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
fn capture_unix_x11_fallback() -> Option<Captured> {
    use std::process::Command;
    static WARNED: std::sync::Once = std::sync::Once::new();
    let tmp = std::env::temp_dir().join("epic_cap_fb.png");
    let ok = Command::new("scrot").args(["-z", tmp.to_str()?]).status()
        .map(|s| s.success()).unwrap_or(false);
    if !ok {
        let ok2 = Command::new("import").args(["-window", "root", tmp.to_str()?]).status()
            .map(|s| s.success()).unwrap_or(false);
        if !ok2 {
            WARNED.call_once(|| log::error!(
                "[CAPTURE] FreeBSD/Unix: install scrot or ImageMagick for screen capture."
            ));
            return None;
        }
    }
    let img = image::open(&tmp).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let _ = std::fs::remove_file(&tmp);
    Some(Captured { pixels: img.into_raw(), width: w, height: h })
}

// ─────────────────────────────────────────────────────────────
// MACOS BLACK FRAME DETECTION
// ─────────────────────────────────────────────────────────────

fn check_macos_black_frame(pixels: &[u8]) -> bool {
    #[cfg(not(target_os = "macos"))]
    return false;

    #[cfg(target_os = "macos")]
    {
        let sample = (10 * 10 * 4).min(pixels.len());
        let all_black = pixels[..sample].chunks_exact(4)
            .all(|px| px[0] < 5 && px[1] < 5 && px[2] < 5);
        if all_black {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| log::error!(
                "[CAPTURE] macOS: frames are black — Screen Recording permission not granted.\n\
                 Fix: System Settings → Privacy & Security → Screen Recording → enable this app\n\
                 Then restart the app."
            ));
        }
        all_black
    }
}

// ─────────────────────────────────────────────────────────────
// THREAD PRIORITY
// ─────────────────────────────────────────────────────────────

pub fn lower_thread_priority() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" { fn pthread_set_qos_class_self_np(q: u32, p: i32) -> i32; }
        pthread_set_qos_class_self_np(0x09, 0);
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    unsafe {
        extern "C" { fn nice(inc: libc::c_int) -> libc::c_int; }
        nice(10);
    }
}

