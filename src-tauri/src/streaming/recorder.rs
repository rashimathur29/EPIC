// ╔══════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/streaming/recorder.rs                         ║
// ║  H264 → MP4 Recording with 1-min duplication & lock handling ║
// ╚══════════════════════════════════════════════════════════════╝

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::thread;
use std::path::{Path, PathBuf};
use std::fs::{self, File};

use crossbeam_channel::Receiver;

use openh264::encoder::{Encoder};
use openh264::formats::YUVSource;

use muxide::api::{MuxerBuilder, VideoCodec};

use crate::streaming::capture::{RawFrame, lower_thread_priority};

// ─────────────────────────────────────────────────────────────
// CONFIG
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RecorderConfig {
    pub fps:                   u32,
    pub width:                 u32,
    pub height:                u32,
    pub output_dir:            PathBuf,
    pub segment_duration_secs: u64,
    pub bitrate_kbps:          u32,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        RecorderConfig {
            fps:                   2,
            width:                 1280,
            height:                720,
            output_dir:            PathBuf::from("recordings"),
            segment_duration_secs: 300,
            bitrate_kbps:          500,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// YUV SOURCE VIEW
// ─────────────────────────────────────────────────────────────

struct YuvSlices<'a> {
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
    w: u32,
    h: u32,
}

impl<'a> YUVSource for YuvSlices<'a> {
    fn dimensions(&self) -> (usize, usize) {
        (self.w as usize, self.h as usize)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (
            self.w as usize,
            (self.w / 2) as usize,
            (self.w / 2) as usize,
        )
    }

    fn y(&self) -> &[u8] { self.y }
    fn u(&self) -> &[u8] { self.u }
    fn v(&self) -> &[u8] { self.v }
}

// ─────────────────────────────────────────────────────────────
// FRAME BUFFERS
// ─────────────────────────────────────────────────────────────

struct FrameBuffers {
    rgba: Vec<u8>,
    yuv:  Vec<u8>,
}

impl FrameBuffers {
    fn new(w: u32, h: u32) -> Self {
        FrameBuffers {
            rgba: vec![0u8; w as usize * h as usize * 4],
            yuv:  vec![0u8; w as usize * h as usize * 3 / 2],
        }
    }
}

// ─────────────────────────────────────────────────────────────
// RGBA → YUV420
// ─────────────────────────────────────────────────────────────

fn rgba_to_yuv420_inplace(rgba: &[u8], w: u32, h: u32, yuv: &mut [u8]) {
    let w = w as usize;
    let h = h as usize;
    let y_size  = w * h;
    let uv_size = (w / 2) * (h / 2);

    let (y_plane, rest)    = yuv.split_at_mut(y_size);
    let (u_plane, v_plane) = rest.split_at_mut(uv_size);

    for row in 0..h {
        for col in 0..w {
            let src = (row * w + col) * 4;
            let r = rgba[src    ] as u16;
            let g = rgba[src + 1] as u16;
            let b = rgba[src + 2] as u16;

            y_plane[row * w + col] = ((77 * r + 150 * g + 29 * b) >> 8) as u8;

            if row % 2 == 0 && col % 2 == 0 {
                let ri = r as i16;
                let gi = g as i16;
                let bi = b as i16;
                let uv_idx = (row / 2) * (w / 2) + (col / 2);
                if uv_idx < u_plane.len() {
                    u_plane[uv_idx] = (((-43 * ri - 85 * gi + 128 * bi) >> 8) + 128).clamp(0, 255) as u8;
                    v_plane[uv_idx] = (((128 * ri - 107 * gi - 21 * bi) >> 8) + 128).clamp(0, 255) as u8;
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
// NAL helpers
// ─────────────────────────────────────────────────────────────

#[inline]
fn is_idr(nal: &[u8]) -> bool {
    if nal.len() < 5 { return false; }
    (nal[4] & 0x1F) == 5
}

// ─────────────────────────────────────────────────────────────
// Encoder creator
// ─────────────────────────────────────────────────────────────

fn make_encoder() -> anyhow::Result<Encoder> {
    Encoder::new().map_err(anyhow::Error::from)
}

// ─────────────────────────────────────────────────────────────
// Segment muxer
// ─────────────────────────────────────────────────────────────

struct SegmentMuxer {
    muxer: muxide::api::Muxer<std::fs::File>,
    path: PathBuf,
    frame_count: u32,
    pts_secs: f64,
    frame_interval_secs: f64,
}

impl SegmentMuxer {
    fn open(path: &Path, width: u32, height: u32, fps: u32) -> anyhow::Result<Self> {
        let file = File::create(path)?;

        let builder = MuxerBuilder::new(file)
            .video(VideoCodec::H264, width, height, fps as f64);

        let muxer = builder.build()?;

        Ok(SegmentMuxer {
            muxer,
            path: path.to_path_buf(),
            frame_count: 0,
            pts_secs: 0.0,
            frame_interval_secs: 1.0 / fps.max(1) as f64,
        })
    }

    fn write_frame(&mut self, data: &[u8], is_keyframe: bool, is_duplicated: bool) -> anyhow::Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        self.muxer.write_video(self.pts_secs, data, is_keyframe)?;

        self.pts_secs += self.frame_interval_secs;
        self.frame_count += 1;

        if is_duplicated {
            log::debug!("[RECORDER] Duplicated frame #{}", self.frame_count);
        }

        Ok(())
    }

    fn finalize(self) -> anyhow::Result<()> {
        self.muxer.finish()?;
        log::info!(
            "[RECORDER] ✅ Segment written: {:?} ({} frames)",
            self.path,
            self.frame_count
        );
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────
// MAIN RECORDER THREAD (with 1-min duplication)
// ─────────────────────────────────────────────────────────────

pub fn start_recorder(
    cfg: RecorderConfig,
    running: Arc<AtomicBool>,
    frame_rx: Receiver<RawFrame>,
) {
    let w = cfg.width & !1;
    let h = cfg.height & !1;

    thread::Builder::new()
        .name("epic-h264-enc".to_string())
        .spawn(move || {
            lower_thread_priority();

            if let Err(e) = fs::create_dir_all(&cfg.output_dir) {
                log::error!("[RECORDER] Cannot create output dir: {}", e);
                return;
            }

            let mut bufs = FrameBuffers::new(w, h);
            let mut current_segment: Option<SegmentMuxer> = None;
            let mut segment_start = Instant::now();
            let mut frames_in_segment = 0u32;

            let mut encoder: Option<Encoder> = None;
            let mut need_new_segment = true;

            let mut last_valid_annex_b: Option<Vec<u8>> = None;
            let mut last_real_capture_time = Instant::now();

            log::info!(
                "[RECORDER] Encode loop running → {:?} ({}s segments at {} fps, 1-min duplication)",
                cfg.output_dir, cfg.segment_duration_secs, cfg.fps
            );

            while running.load(Ordering::Relaxed) {
                let frame = match frame_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(f)  => f,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };

                // 1-minute duplication rule
                let time_since_last_real = last_real_capture_time.elapsed();
                let force_new_encode = time_since_last_real >= Duration::from_secs(60);

                if force_new_encode {
                    log::debug!("[RECORDER] Forcing new encode after 1 min");
                    last_real_capture_time = Instant::now();
                }

                // Rotate segment if needed
                if !need_new_segment
                    && current_segment.is_some()
                    && segment_start.elapsed().as_secs() >= cfg.segment_duration_secs
                {
                    need_new_segment = true;
                }

                if need_new_segment {
                    if let Some(seg) = current_segment.take() {
                        log::info!(
                            "[RECORDER] Rotating segment after {} frames",
                            frames_in_segment
                        );
                        let _ = seg.finalize();
                        thread::sleep(Duration::from_millis(50));
                    }
                    frames_in_segment = 0;
                    last_valid_annex_b = None;

                    match make_encoder() {
                        Ok(enc) => encoder = Some(enc),
                        Err(e) => {
                            log::error!("[RECORDER] Encoder failed: {}", e);
                            break;
                        }
                    }
                }

                let enc = match encoder.as_mut() {
                    Some(e) => e,
                    None => continue,
                };

                let pix_len = frame.pixels.len();
                if bufs.rgba.len() != pix_len {
                    bufs.rgba.resize(pix_len, 0);
                }
                bufs.rgba.copy_from_slice(&frame.pixels);

                rgba_to_yuv420_inplace(&bufs.rgba, w, h, &mut bufs.yuv);

                let y_size  = (w * h) as usize;
                let uv_size = (w / 2 * h / 2) as usize;
                let yuv_view = YuvSlices {
                    y: &bufs.yuv[..y_size],
                    u: &bufs.yuv[y_size..y_size + uv_size],
                    v: &bufs.yuv[y_size + uv_size..],
                    w, h,
                };

                let bitstream = match enc.encode(&yuv_view) {
                    Ok(b)  => b,
                    Err(e) => {
                        log::warn!("[RECORDER] Encode failed: {}", e);
                        continue;
                    }
                };

                let mut annex_b = Vec::new();
                for layer_idx in 0..bitstream.num_layers() {
                    if let Some(layer) = bitstream.layer(layer_idx) {
                        for nal_idx in 0..layer.nal_count() {
                            if let Some(nal) = layer.nal_unit(nal_idx) {
                                annex_b.extend_from_slice(nal);
                            }
                        }
                    }
                }

                let is_key = annex_b.chunks(5).any(|chunk| chunk.len() >= 5 && is_idr(chunk));

                if !annex_b.is_empty() {
                    last_valid_annex_b = Some(annex_b.clone());
                    last_real_capture_time = Instant::now();
                }

                if need_new_segment {
                    if annex_b.is_empty() || !is_key {
                        continue;
                    }

                    let path = new_path(&cfg.output_dir);
                    match SegmentMuxer::open(&path, w, h, cfg.fps) {
                        Ok(seg) => {
                            current_segment = Some(seg);
                            segment_start = Instant::now();
                            need_new_segment = false;
                            log::info!("[RECORDER] New segment: {:?}", path);
                        }
                        Err(e) => {
                            log::error!("[RECORDER] Segment open failed: {}", e);
                            break;
                        }
                    }
                }

                let data_to_write = if annex_b.is_empty() {
                    match last_valid_annex_b.as_ref() {
                        Some(last) => last,
                        None => continue,
                    }
                } else {
                    &annex_b
                };

                let duplicated = annex_b.is_empty();

                if let Some(seg) = current_segment.as_mut() {
                    if let Err(e) = seg.write_frame(data_to_write, is_key, duplicated) {
                        log::error!("[RECORDER] Write failed: {}", e);
                    } else {
                        frames_in_segment += 1;
                    }
                }
            }

            if let Some(seg) = current_segment.take() {
                log::info!("[RECORDER] Finalizing last segment ({} frames)...", frames_in_segment);
                let _ = seg.finalize();
            }

            log::info!("[RECORDER] Encode loop stopped");
        })
        .expect("Failed to spawn h264-enc");
}

fn new_path(dir: &Path) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    dir.join(format!("rec_{}.mp4", ts))
}