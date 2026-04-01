// db/core.rs — Encrypted Database Manager
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use rand::rngs::OsRng;
use rand::RngCore;
use base64::{engine::general_purpose, Engine as _};
use tauri::{path::BaseDirectory, Manager};
use keyring::Entry;
use thiserror::Error;
use serde::Serialize;

#[derive(Error, Debug, Serialize)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Worker error: {0}")]
    WorkerError(String),

    #[error("Keyring error: {0}")]
    Keyring(String),
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self { Error::Database(err.to_string()) }
}
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self { Error::Serialization(err.to_string()) }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct DbManager {
    pub conn: Mutex<Connection>,
}

// ─────────────────────────────────────────────────────────────
// Path helpers
// ─────────────────────────────────────────────────────────────
pub fn get_db_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .resolve("database", BaseDirectory::AppData)
        .expect("Unable to resolve app data directory");
    std::fs::create_dir_all(&dir).ok();
    dir.join("epic_secure.db")
}

// ─────────────────────────────────────────────────────────────
// Keyring helpers
// ─────────────────────────────────────────────────────────────
pub fn get_key_from_keyring(service: &str, username: &str) -> Result<Option<String>> {
    let entry = Entry::new(service, username).map_err(|e| Error::Keyring(e.to_string()))?;
    match entry.get_password() {
        Ok(pw)                       => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e)                       => Err(Error::Keyring(e.to_string())),
    }
}

pub fn generate_and_store_key(service: &str, username: &str) -> Result<String> {
    let mut key_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut key_bytes);
    let key_b64 = general_purpose::STANDARD.encode(&key_bytes);
    let entry = Entry::new(service, username).map_err(|e| Error::Keyring(e.to_string()))?;
    entry.set_password(&key_b64).map_err(|e| Error::Keyring(e.to_string()))?;
    Ok(key_b64)
}

// ─────────────────────────────────────────────────────────────
// DbManager
// ─────────────────────────────────────────────────────────────
impl DbManager {
    pub fn open_or_create(app: &tauri::AppHandle) -> Result<Self> {
        let db_path = get_db_path(app);
        let key_b64 = match get_key_from_keyring("epic-tauri", "database-key")? {
            Some(k) => k,
            None    => generate_and_store_key("epic-tauri", "database-key")?,
        };
        let manager = Self::open_encrypted(&db_path, &key_b64)?;
        manager.initialize_schema()?;
        Ok(manager)
    }

    pub fn open_encrypted(db_path: &Path, key_b64: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(&format!(
            "PRAGMA key = '{}';
             PRAGMA cipher_page_size = 4096;
             PRAGMA kdf_iter = 64000;
             PRAGMA cipher_hmac_algorithm = HMAC_SHA1;
             PRAGMA cipher_kdf_algorithm = PBKDF2_HMAC_SHA1;",
            key_b64
        ))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    // ─────────────────────────────────────────────────────────
    // Schema
    //
    // IMPORTANT NOTES:
    //   • user_details.email has a UNIQUE constraint — this enables the
    //     "INSERT … ON CONFLICT(email) DO UPDATE" upsert in auth_commands.rs.
    //     If you add a migration for existing installs, run:
    //       CREATE UNIQUE INDEX IF NOT EXISTS idx_user_details_email
    //         ON user_details(email);
    //
    //   • user_session is always a single row (id=1, CHECK enforced).
    //     Deleting that row = logged out.
    //     Absence of a matching row in user_details = kicked.
    // ─────────────────────────────────────────────────────────
    pub fn initialize_schema(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            -- ── Config ────────────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS config (
                key   TEXT PRIMARY KEY,
                value INTEGER
            );

            -- ── org_config ─────────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS org_config (
                org_id           TEXT PRIMARY KEY,
                org_name         TEXT NOT NULL DEFAULT '',
                domain           TEXT NOT NULL DEFAULT '',
                sso_enabled      INTEGER NOT NULL DEFAULT 0,
                logo_url         TEXT,               
                plan_type        TEXT,               
                start_date       TEXT,               
                end_date         TEXT,               
                deployment       TEXT,               
                api_base_url     TEXT,               
                timezone         TEXT NOT NULL DEFAULT 'Asia/Kolkata',
                is_active        INTEGER NOT NULL DEFAULT 1,  -- 0=inactive, 1=active 
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT
            );

            -- ── Per-minute keyboard / mouse activity ───────────────────────────
            CREATE TABLE IF NOT EXISTS user_activity_minute (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                minute_start      TEXT,
                minute_end        TEXT,
                keystroke_count   INTEGER,
                mouse_move_count  INTEGER,
                mouse_click_count INTEGER,
                idle_seconds      INTEGER,
                created_at        TEXT,
                updated_at        TEXT,
                apscreatedatetime  TEXT,
                apsupdatedatetime  TEXT,
                timezone          TEXT
            );

            -- ── Inactivity / idle periods ──────────────────────────────────────
            CREATE TABLE IF NOT EXISTS user_inactivity (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                inactive_start_time   TEXT NOT NULL,
                inactive_end_time     TEXT NOT NULL,
                inactivity_by         TEXT NOT NULL,
                is_microphone_in_use  INTEGER,
                duration              INTEGER NOT NULL,
                created_at            TEXT NOT NULL,
                updated_at            TEXT,
                apscreatedatetime      TEXT,
                apsupdatedatetime      TEXT,
                timezone              TEXT
            );

            -- ── Daily check-in / check-out ─────────────────────────────────────
            CREATE TABLE IF NOT EXISTS user_checkin (
                id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                checkin_time       TEXT,
                checkout_time      TEXT,
                breaks             TEXT,
                break_duration     TEXT,
                total_elapsed_time INTEGER,
                last_active_time   TEXT,
                created_at         TEXT,
                updated_at         TEXT,
                apscreatedatetime   TEXT,
                apsupdatedatetime   TEXT,
                timezone           TEXT
            );

            -- ── Break log ──────────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS user_breaks (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                breakin_time     TEXT,
                breakout_time    TEXT,
                break_duration   INTEGER,
                reason           TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            -- ── 10-minute summaries ────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS user_activity_summary (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time          TEXT,
                end_time            TEXT,
                keystroke_list      TEXT,
                mouse_movement_list TEXT,
                mouse_click_list    TEXT,
                total_idle_seconds  INTEGER,
                created_at          TEXT,
                updated_at          TEXT,
                apscreatedatetime    TEXT,
                apsupdatedatetime    TEXT,
                timezone            TEXT
            );

            -- ── Call tracking ──────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS call_log (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                call_start_time  TEXT,
                call_end_time    TEXT,
                duration         TEXT,
                type             TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            -- ── Active window / app tracking ───────────────────────────────────
            CREATE TABLE IF NOT EXISTS active_window (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                window_title     TEXT NOT NULL,
                process_name     TEXT NOT NULL,
                start_time       TEXT NOT NULL,
                end_time         TEXT NOT NULL,
                duration_sec     INTEGER NOT NULL,
                created_at       TEXT NOT NULL,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT NOT NULL
            );

            -- ── App-level settings ─────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS user_settings (
                id      INTEGER PRIMARY KEY CHECK(id = 1),
                email   TEXT,
                version TEXT,
                uuid    TEXT
            );

            -- ── Scheduled job tracking ─────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS job_scheduled (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                last_datetime TEXT
            );

            -- ── User profile / details ─────────────────────────────────────────
            -- email is UNIQUE so auth_commands.rs can use ON CONFLICT upsert.
            -- The "kicked" check queries this table: if the row is absent while
            -- a user_session row exists, the user has been removed externally.
            CREATE TABLE IF NOT EXISTS user_details (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id      TEXT,
                email        TEXT UNIQUE,          -- ← UNIQUE for upsert + kicked check
                first_name   TEXT,
                middle_name  TEXT,
                last_name    TEXT,
                org_id       TEXT,
                shift_timing TEXT,
                timezone     TEXT,
                designation  TEXT,
                phone_number TEXT,
                valid_upto   TEXT
            );

            -- Index to speed up the kicked check (email + user_id lookup)
            CREATE INDEX IF NOT EXISTS idx_user_details_email_uid
                ON user_details(email, user_id);

            -- ── Network / connection log ───────────────────────────────────────
            CREATE TABLE IF NOT EXISTS connection (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                ip_address       TEXT,
                connection_type  TEXT,
                isp              TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            -- ── Location ───────────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS location_details (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                location         TEXT,
                latitude         TEXT,
                longitude        TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            -- ── Screenshots ────────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS screenshots (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                screenshot       TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            -- ── Video filenames ────────────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS video_filename (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                video_name TEXT,
                timestamp  TEXT,
                created_at TEXT
            );

            -- ── Touch / gesture events ─────────────────────────────────────────
            CREATE TABLE IF NOT EXISTS touch_events (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time       TEXT,
                end_time         TEXT,
                touch_list       TEXT,
                created_at       TEXT,
                updated_at       TEXT,
                apscreatedatetime TEXT,
                apsupdatedatetime TEXT,
                timezone         TEXT
            );

            
            CREATE TABLE IF NOT EXISTS user_session (
                id             INTEGER PRIMARY KEY CHECK(id = 1),
                user_id        TEXT NOT NULL DEFAULT '',
                email          TEXT NOT NULL DEFAULT '',
                first_name     TEXT,
                last_name      TEXT,
                org_id         TEXT,
                token          TEXT,
                is_first_login INTEGER NOT NULL DEFAULT 0,
                created_at     TEXT,
                updated_at     TEXT
            );
            "#,
        )?;
        Ok(())
    }
}