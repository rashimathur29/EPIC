// ╔══════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/streaming/streamer.rs                        ║
// ║  MJPEG over WebSocket — NO screen capture code here         ║
// ╚══════════════════════════════════════════════════════════════╝
//
// THIS FILE DOES NOT CAPTURE THE SCREEN.
// It only receives RawFrame from the shared capture thread (capture.rs)
// and converts RGBA pixels → JPEG → WebSocket binary message.
//
// DATA FLOW:
//   capture.rs → RawFrame (Arc<pixels>) → [channel] → this file
//   this file  → RGBA strip alpha → resize if needed → JPEG encode
//              → WebSocket binary message → browser
//
// JPEG encode per frame:
//   Input:  1280×720 RGBA = 3.5MB raw pixels (shared Arc, NOT a copy)
//   Output: ~50-100KB JPEG (quality 55%)
//   CPU:    ~2-5ms per frame on modern hardware

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::thread;
use std::net::TcpListener;

use image::codecs::jpeg::JpegEncoder;
use crossbeam_channel::Receiver;
use tungstenite::{accept, Message};

use crate::streaming::capture::{RawFrame, lower_thread_priority};

// ─────────────────────────────────────────────────────────────
// CONFIG
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct StreamerConfig {
    pub jpeg_quality: u8,   // 1-100. 55 = good balance.
    pub ws_port:      u16,  // Browser connects to ws://localhost:{ws_port}
}

impl Default for StreamerConfig {
    fn default() -> Self {
        StreamerConfig { jpeg_quality: 55, ws_port: 9001 }
    }
}

// ─────────────────────────────────────────────────────────────
// STREAMER THREAD
//
// Receives RawFrame from capture.rs channel.
// Does NOT call capture_screen(). Never has.
// ─────────────────────────────────────────────────────────────

pub fn start_streamer(
    cfg:      StreamerConfig,
    running:  Arc<AtomicBool>,
    frame_rx: Receiver<RawFrame>,
) {
    // ── WebSocket listener ────────────────────────────────────
    let addr     = format!("0.0.0.0:{}", cfg.ws_port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l)  => { log::info!("[STREAM] ws://localhost:{}", cfg.ws_port); l }
        Err(e) => {
            log::error!("[STREAM] Cannot bind port {}: {}. Is it already in use?", cfg.ws_port, e);
            return;
        }
    };
    listener.set_nonblocking(true).ok();

    // Shared list of connected browser clients
    type Clients = Arc<Mutex<Vec<tungstenite::WebSocket<std::net::TcpStream>>>>;
    let clients:        Clients = Arc::new(Mutex::new(Vec::new()));
    let clients_accept: Clients = Arc::clone(&clients);

    // ── Accept thread (browser connects here) ─────────────────
    thread::Builder::new()
        .name("epic-ws-accept".to_string())
        .spawn(move || {
            for result in listener.incoming() {
                match result {
                    Ok(tcp) => {
                        // Timeouts: prevent hangs from port scanners or frozen clients
                        let _ = tcp.set_read_timeout(Some(Duration::from_secs(5)));
                        let _ = tcp.set_write_timeout(Some(Duration::from_secs(5)));
                        let _ = tcp.set_nodelay(true);
                        if let Ok(ws) = accept(tcp) {
                            log::info!("[STREAM] Viewer connected");
                            if let Ok(mut list) = clients_accept.lock() {
                                list.push(ws);
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {}
                }
            }
        })
        .expect("Failed to spawn ws-accept");

    // ── Encode + broadcast thread ─────────────────────────────
    thread::Builder::new()
        .name("epic-jpeg-broadcast".to_string())
        .spawn(move || {
            lower_thread_priority();
            let quality = cfg.jpeg_quality;

            while running.load(Ordering::Relaxed) {
                let frame = match frame_rx.recv_timeout(Duration::from_millis(300)) {
                    Ok(f)  => f,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };

                // ── RGBA → RGB ────────────────────────────────────────
                // JPEG has no alpha channel. Strip the 4th byte from every pixel.
                // chunks_exact(4) = iterate 4 bytes at a time [R,G,B,A]
                // flat_map picks first 3 bytes [R,G,B], discards A.
                //
                // NOTE: frame.pixels is an Arc<Vec<u8>>.
                // We deref once via `*frame.pixels` to get &Vec<u8>,
                // then iterate. NO pixel data is copied here — we read from
                // the Arc directly and write the RGB result into a new Vec.
                // The RGBA source stays shared between stream and record threads.
                let rgb_vec: Vec<u8> = frame.pixels
                    .chunks_exact(4)
                    .flat_map(|px| [px[0], px[1], px[2]])
                    .collect();

                let rgb_img = match image::RgbImage::from_raw(frame.width, frame.height, rgb_vec) {
                    Some(img) => img,
                    None => { log::warn!("[STREAM] Bad RGB dims"); continue; }
                };

                // ── JPEG encode ───────────────────────────────────────
                let mut jpeg = Vec::with_capacity(100_000);
                {
                    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, quality);
                    if enc.encode_image(&rgb_img).is_err() { continue; }
                }

                // ── Broadcast to all viewers ──────────────────────────
                let msg = Message::Binary(jpeg);
                if let Ok(mut list) = clients.lock() {
                    let mut dead: Vec<usize> = Vec::new();
                    for (i, ws) in list.iter_mut().enumerate() {
                        match ws.send(msg.clone()) {
                            Ok(_) => {}
                            Err(_) => dead.push(i),
                        }
                    }
                    // Remove in reverse so indices stay valid
                    for i in dead.into_iter().rev() {
                        log::info!("[STREAM] Viewer disconnected");
                        list.swap_remove(i);
                    }
                }
            }

            log::info!("[STREAM] Broadcast thread stopped");
        })
        .expect("Failed to spawn jpeg-broadcast");
}