// ╔══════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/lib.rs                                        ║
// ║  PURPOSE: declare modules + define AppState.                 ║
// ║  DO NOT put setup/main logic here.                           ║
// ║  DO NOT use `epic_tauri_lib::` — this IS epic_tauri_lib.     ║
// ╚══════════════════════════════════════════════════════════════╝

// ── existing modules (unchanged) ─────────────────────────────────────────────
pub mod commands;
pub mod db;
pub mod tracker;
pub mod logger;
pub mod api_client;
pub mod encryption_utils;
pub mod utils;
pub mod streaming;
pub mod env_config;
pub mod bypass_data;
pub mod auth_commands;
pub mod timestamp;

// ── re-exports ────────────────────────────────────────────────────────────────
pub use crate::db::core::{Error, Result};

// ── imports for AppState ──────────────────────────────────────────────────────
use std::sync::{Arc, Mutex};
use crate::db::core::DbManager;
use crate::tracker::ActivityTracker;
use crate::streaming::pipeline::Pipeline;
use tauri::AppHandle;

// ── Shared application state (injected into every Tauri command) ──────────────
pub struct AppState {
    pub db:         Arc<DbManager>,
    pub tracker:    Mutex<Option<ActivityTracker>>,
    pub pipeline:   Mutex<Option<Pipeline>>,
    pub app_handle: AppHandle,
}