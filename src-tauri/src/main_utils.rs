use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use std::io::{self, Write};
use winreg::enums::*;
use winreg::RegKey;
use fs2::FileExt;
use tokio::time::{self, Duration};
use reqwest;
use serde_json;
use chrono::{DateTime, Utc, NaiveDate};
use std::cmp::Ordering;

// Constants
const BASE_PATH: &str = r"C:\AIPRUS\EPIC-kafka\AppData";
const VERSION_FILE_PATH: &str = "version.txt";

pub fn get_paths() -> HashMap<String, PathBuf> {
    let base_path = PathBuf::from(BASE_PATH);
    let mut paths = HashMap::new();
    paths.insert("db".to_string(), base_path.join("user_activity.db"));
    paths.insert("log".to_string(), base_path.join("activity.log"));
    paths.insert("mail".to_string(), base_path.join("email_detail.txt"));
    paths.insert("unique".to_string(), base_path.join("unique.txt"));
    paths.insert("lock".to_string(), base_path.join("app.lock"));
    paths.insert("last_cleanup".to_string(), base_path.join("last_cleanup_timestamp.txt"));
    paths.insert("user_id".to_string(), base_path.join("id.txt"));
    paths
}

pub fn enable_autostart(app_name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let (key, _) = hkcu.create_subkey(path)?;
    let exe_path = std::env::current_exe()?;
    key.set_value(app_name, &format!("\"{}\"", exe_path.display()))?;
    log::info!("Autostart enabled for {}", app_name);
    Ok(true)
}

pub fn enforce_single_instance(lock_file_path: &PathBuf) -> Result<fs::File, Box<dyn std::error::Error>> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(lock_file_path)?;
    file.try_lock_exclusive()?;
    log::info!("Single instance lock acquired");
    Ok(file)
}

pub fn create_file_or_directory(paths: &HashMap<String, PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    for path in paths.values() {
        if !path.exists() {
            fs::File::create(path)?;
            log::info!("Created file: {:?}", path);
        }
    }
    Ok(())
}

pub async fn run_schedule<F>(mut job: F, interval: Duration)
where
    F: FnMut() + Send + 'static,
{
    let mut interval = time::interval(interval);
    loop {
        interval.tick().await;
        job();
    }
}

pub fn thread_monitor(stop: std::sync::Arc<std::sync::atomic::AtomicBool>, interval: u64) {
    std::thread::spawn(move || {
        while stop.load(std::sync::atomic::Ordering::Relaxed) {
            let count = std::thread::current().id();
            // In Rust, thread enumeration is limited; logging current thread id
            log::info!("Thread monitor: current_thread_id={:?}", count);
            std::thread::sleep(std::time::Duration::from_secs(interval));
        }
    });
}

pub async fn send_data_periodically(db_manager: &dyn DatabaseManagerTrait) -> Result<(), Box<dyn std::error::Error>> {
    // Assuming db_manager has get_all_data_periodic method
    let (result, code, _) = db_manager.get_all_data_periodic().await?;
    if result {
        db_manager.update_last_datetime().await?;
    } else if code == 403 {
        log::error!("Received 403 Forbidden");
        access_msg().await?;
    }
    log::info!("Periodic data sent");
    Ok(())
}

pub async fn access_msg() -> Result<(), Box<dyn std::error::Error>> {
    // In Tauri, show message via command or emit event
    // For now, log it
    log::error!("Access Denied: This device is not primary");
    Ok(())
}

pub async fn check_for_updates() -> Result<(), Box<dyn std::error::Error>> {
    let os_name = std::env::consts::OS;
    let version_url = std::env::var("VERSION_URL")?;
    let product_name = std::env::var("PRODUCT_NAME")?;
    let client = reqwest::Client::new();
    let resp = client.get(&version_url)
        .query(&[("productName", &product_name), ("platform", &os_name)])
        .timeout(Duration::from_secs(5))
        .send().await?;
    let data: serde_json::Value = resp.json().await?;
    let latest_version = data["latestVersion"].as_str().ok_or("No latest version")?;
    let deployment_datetime = data["deploymentDate"].as_str();
    let deployment_date = if let Some(dt) = deployment_datetime {
        DateTime::parse_from_rfc3339(dt)?.date_naive()
    } else {
        Utc::now().date_naive()
    };
    let current_version = fs::read_to_string(VERSION_FILE_PATH)?.trim().to_string();
    log::info!("Current: {}, Latest: {}", current_version, latest_version);
    if compare_versions(&current_version, latest_version) != Ordering::Equal {
        let days_since = (Utc::now().date_naive() - deployment_date).num_days();
        if days_since > 3 {
            log::info!("Update available");
            // Trigger update dialog in GUI
        }
    }
    Ok(())
}

pub fn compare_versions(v1: &str, v2: &str) -> Ordering {
    let v1_parts: Vec<i32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let v2_parts: Vec<i32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();
    for (a, b) in v1_parts.iter().zip(&v2_parts) {
        match a.cmp(b) {
            Ordering::Equal => continue,
            ord => return ord,
        }
    }
    v1_parts.len().cmp(&v2_parts.len())
}

// Placeholder trait for db_manager
#[async_trait::async_trait]
pub trait DatabaseManagerTrait {
    async fn get_all_data_periodic(&self) -> Result<(bool, u16, String), Box<dyn std::error::Error>>;
    async fn update_last_datetime(&self) -> Result<(), Box<dyn std::error::Error>>;
}
