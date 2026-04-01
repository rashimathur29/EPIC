// ╔══════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/streaming/pipeline.rs                        ║
// ║  THE ONLY PUBLIC ENTRY POINT for streaming + recording      ║
// ╚══════════════════════════════════════════════════════════════╝
//
// THIS IS THE ONLY FILE your app code should call.
// It wires everything together correctly so double-capture
// is structurally impossible.
//
// THREAD MAP after Pipeline::start():
//
//   ┌─────────────────────────────────────────────┐
//   │  Thread: "epic-capture"  (capture.rs)        │
//   │  • calls Screen::capture() — the ONLY place  │
//   │  • resizes to target resolution once         │
//   │  • wraps pixels in Arc<Vec<u8>>              │
//   │  • sends RawFrame to TWO channels:           │
//   │      stream_tx ──────────────────────────┐   │
//   │      record_tx ──────────────────────┐   │   │
//   └──────────────────────────────────────│───│───┘
//                                          │   │
//         ┌─────────────────────────────────┘   │
//         ▼                                     │
//   ┌─────────────────────────────┐             │
//   │  Thread: "epic-h264-enc"    │             │
//   │  (recorder.rs)              │             │
//   │  • copies Arc→Vec (1 copy)  │             │
//   │  • RGBA→YUV420 in-place     │             │
//   │  • H264 encode              │             │
//   │  • writes to .mp4 file      │             │
//   └─────────────────────────────┘             │
//                                               │
//         ┌─────────────────────────────────────┘
//         ▼
//   ┌──────────────────────────────────────┐
//   │  Thread: "epic-jpeg-broadcast"       │
//   │  (streamer.rs)                       │
//   │  • reads Arc pixels (0 copies)       │
//   │  • RGBA→RGB (strip alpha)            │
//   │  • JPEG encode                       │
//   │  • WebSocket broadcast               │
//   └──────────────────────────────────────┘
//
// COPY COUNT PER FRAME:
//   Capture → Arc<pixels>: 0 copies (original allocation)
//   Stream  path:           0 copies (reads Arc directly)
//   Record  path:           1 copy   (Arc→Vec, unavoidable for H264 encoder)
//   Total:                  1 copy per frame = theoretical minimum

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::thread;
use std::path::PathBuf;

use tauri::Manager;

use crossbeam_channel::bounded;
use once_cell::sync::Lazy;
use std::sync::Mutex;

use crate::streaming::capture::{start_capture, CaptureConfig, RawFrame};
use crate::streaming::streamer::{start_streamer, StreamerConfig};
use crate::streaming::recorder::{start_recorder, RecorderConfig};

// ─────────────────────────────────────────────────────────────
// CONFIG
// ─────────────────────────────────────────────────────────────

pub struct PipelineConfig {
    /// Frames per second for BOTH stream and record (same capture feeds both)
    pub fps:            u32,
    pub width:          u32,
    pub height:         u32,
    pub enable_stream:  bool,
    pub enable_record:  bool,
    // Streamer settings
    pub ws_port:        u16,
    pub jpeg_quality:   u8,
    // Recorder settings
    pub output_dir:     PathBuf,
    pub segment_secs:   u64,
    pub bitrate_kbps:   u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        PipelineConfig {
            fps:           2,
            width:         1280,
            height:        720,
            enable_stream: true,
            enable_record: true,
            ws_port:       9001,
            jpeg_quality:  55,
            output_dir:    PathBuf::from("recordings"),
            segment_secs:  300,
            bitrate_kbps:  500,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// PIPELINE HANDLE
// ─────────────────────────────────────────────────────────────

pub struct Pipeline {
    // The capture thread checks this flag. When false → thread exits.
    running: Arc<AtomicBool>,
}

impl Pipeline {
    pub fn start(cfg: PipelineConfig) -> Self {
        // ── Build channels ────────────────────────────────────────
        // bounded(N): channel holds at most N frames.
        // If consumer is slow → frames are dropped (low CPU, bounded RAM).

        let running = Arc::new(AtomicBool::new(true));

        let stream_tx = if cfg.enable_stream {
            let (tx, rx) = bounded::<RawFrame>(2);
            start_streamer(
                StreamerConfig {
                    jpeg_quality: cfg.jpeg_quality,
                    ws_port:      cfg.ws_port,
                },
                // streamer gets its own AtomicBool derived from the same source
                // When capture stops sending, frame_rx disconnects → streamer exits
                //Arc::new(AtomicBool::new(true)),
                running.clone(),                
                rx,
            );
            Some(tx)
        } else { None };

        let record_tx = if cfg.enable_record {
            let (tx, rx) = bounded::<RawFrame>(3);
            start_recorder(
                RecorderConfig {
                    fps:                   cfg.fps,
                    width:                 cfg.width,
                    height:                cfg.height,
                    output_dir:            cfg.output_dir.clone(),
                    segment_duration_secs: cfg.segment_secs,
                    bitrate_kbps:          cfg.bitrate_kbps,
                },
                //Arc::new(AtomicBool::new(true)),
                running.clone(),
                rx,
            );
            Some(tx)
        } else { None };

        // ── Start THE ONE capture thread ──────────────────────────
        // This is the only call to start_capture() in the codebase.
        // It calls Screen::capture() in a loop and feeds both channels.
        start_capture(
            CaptureConfig {
                fps:    cfg.fps,
                width:  cfg.width,
                height: cfg.height,
            },
            stream_tx,
            record_tx,
            running.clone(),
        );

        log::info!(
            "[PIPELINE] Started — {}fps {}×{} stream={} record={}",
            cfg.fps, cfg.width, cfg.height,
            cfg.enable_stream, cfg.enable_record
        );

        Pipeline { running }
    }

    pub fn stop(&self) {
        // Setting this false causes the capture thread to exit its loop.
        // When it exits, both channels (stream_tx, record_tx) are dropped.
        // Dropped Sender → Receiver gets Disconnected error → both encoder
        // threads also exit their loops gracefully.
        self.running.store(false, Ordering::Relaxed);
        log::info!("[PIPELINE] Stopping...");
        // Give encoder threads time to finalize the last MP4 segment
        thread::sleep(Duration::from_secs(4));
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) { if self.is_running() { self.stop(); } }
}

// ─────────────────────────────────────────────────────────────
// GLOBAL PIPELINE STATE
// Allows Tauri commands to access the pipeline from any command handler
// ─────────────────────────────────────────────────────────────

static PIPELINE: Lazy<Mutex<Option<Pipeline>>> = Lazy::new(|| Mutex::new(None));

// ─────────────────────────────────────────────────────────────
// TAURI COMMANDS
// ─────────────────────────────────────────────────────────────

/// JS: await invoke('start_pipeline', { fps: 2, enableStream: true, enableRecord: true })
#[tauri::command]
pub fn start_pipeline(
    fps:           Option<u32>,
    enable_stream: Option<bool>,
    enable_record: Option<bool>,
    app:           tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let mut guard = PIPELINE.lock().map_err(|e| e.to_string())?;

    if guard.is_some() {
        return Ok(serde_json::json!({
            "status": "already_running",
            "stream_url": "ws://localhost:9001"
        }));
    }

    let output_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("AppDataDir error: {}", e))?
        .join("recordings");

    let fps           = fps.unwrap_or(2).clamp(1, 30);
    let enable_stream = enable_stream.unwrap_or(true);
    let enable_record = enable_record.unwrap_or(true);

    let pipeline = Pipeline::start(PipelineConfig {
        fps,
        enable_stream,
        enable_record,
        output_dir:   output_dir.clone(),
        ..Default::default()
    });

    *guard = Some(pipeline);

    Ok(serde_json::json!({
        "status":         "started",
        "fps":            fps,
        "stream_enabled": enable_stream,
        "record_enabled": enable_record,
        "stream_url":     "ws://localhost:9001",
        "recordings_dir": output_dir.display().to_string(),
    }))
}

/// JS: await invoke('stop_pipeline')
#[tauri::command]
pub fn stop_pipeline() -> serde_json::Value {
    let mut guard = PIPELINE.lock().unwrap();
    match guard.take() {
        Some(p) => { p.stop(); serde_json::json!({ "status": "stopped" }) }
        None    => serde_json::json!({ "status": "was_not_running" }),
    }
}

/// JS: const s = await invoke('pipeline_status')
#[tauri::command]
pub fn pipeline_status() -> serde_json::Value {
    let guard = PIPELINE.lock().unwrap();
    serde_json::json!({
        "running":    guard.as_ref().map(|p| p.is_running()).unwrap_or(false),
        "stream_url": "ws://localhost:9001",
    })
}