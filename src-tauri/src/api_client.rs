use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::{Result, anyhow};
use uuid::Uuid;
use std::env;
use std::fs;
use base64::{Engine as _, engine::general_purpose};
use hostname;

use crate::encryption_utils::AES256Encryptor;
use crate::utils::{validate_required_fields, get_current_utc_timestamp};

#[derive(Clone)]
pub struct ProductivityAPIClient {
    base_url: String,
    client: Client,
    timeout: Duration,
    user_id: Option<String>,
    user_email: Option<String>,
    org_id: Option<String>,
    encryption_key: Option<String>,
    max_retries: usize,
    retry_delay: u64,
    encryptor: Option<AES256Encryptor>,
    device_id: String,
}

impl ProductivityAPIClient {
    pub fn new(
        base_url: String,
        user_email: Option<String>,
        user_id: Option<String>,
        org_id: Option<String>,
        timeout: Option<u64>,
    ) -> Result<Self> {
        dotenv::dotenv().ok();
        let encryption_key = env::var("ENCRYPTION_KEY").ok();
        let encryptor = if let Some(key) = &encryption_key {
            Some(AES256Encryptor::new(key)?)
        } else {
            None
        };

        let client = Client::builder()
            .user_agent("ProductivityAPIClient/1.0")
            .timeout(Duration::from_secs(timeout.unwrap_or(300)))
            .build()?;

        let device_id = hostname::get()?.to_string_lossy().to_string();

        log::info!("Initialized APIClient");

        Ok(Self {
            base_url,
            client,
            timeout: Duration::from_secs(timeout.unwrap_or(300)),
            user_id,
            user_email,
            org_id,
            encryption_key,
            max_retries: 3,
            retry_delay: 1,
            encryptor,
            device_id,
        })
    }

    pub fn is_internet_connected() -> bool {
        match std::net::TcpListener::bind("0.0.0.0:0") {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    fn encrypt_payload(&self, data: &Value) -> Result<Value> {
        if let Some(encryptor) = &self.encryptor {
            let encrypted = encryptor.encrypt(&data.to_string())?;
            Ok(json!({"data": encrypted}))
        } else {
            Ok(data.clone())
        }
    }

    pub async fn make_request(
        &self,
        email: &str,
        org_id: &str,
        user_id: &str,
        endpoint: &str,
        data: Option<&Value>,
        method: &str,
        files: Option<HashMap<String, Vec<u8>>>,
    ) -> Result<Value> {
        let url = format!("{}/{}", self.base_url, endpoint);

        if !Self::is_internet_connected() {
            log::warn!("No internet connection detected. Bypassing API call.");
            return Ok(json!({"status": "error", "message": "No internet connection"}));
        }

        let is_file_upload = files.is_some();
        let response = if let Some(files) = files {
            // File upload
            let mut form = reqwest::multipart::Form::new();
            if let Some(data) = data {
                for (k, v) in data.as_object().unwrap() {
                    form = form.text(k.clone(), v.to_string());
                }
            }
            form = form.text("email", email.to_string());
            form = form.text("org_id", org_id.to_string());
            form = form.text("user_id", user_id.to_string());

            for (name, content) in files {
                let part = reqwest::multipart::Part::bytes(content)
                    .file_name(name.clone());
                form = form.part(name, part);
            }

            self.client.request(method.parse()?, &url)
                .multipart(form)
                .send()
                .await?
        } else if method.to_uppercase() == "GET" {
            let enc_payload = self.encrypt_payload(data.unwrap_or(&json!({})))?;
            let params = [("data", enc_payload["data"].as_str().unwrap_or(""))];
            self.client.get(&url)
                .query(&params)
                .send()
                .await?
        } else {
            let mut payload = if let Some(data) = data {
                self.encrypt_payload(data)?
            } else {
                json!({})
            };
            payload["org_id"] = json!(org_id);
            payload["email"] = json!(email);
            payload["user_id"] = json!(user_id);

            self.client.request(method.parse()?, &url)
                .json(&payload)
                .send()
                .await?
        };

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("Request failed with status: {}", status));
        }

        let response_data: Value = response.json().await?;
        self.process_response(response_data, is_file_upload)
    }

    fn process_response(&self, response_data: Value, is_file_upload: bool) -> Result<Value> {
        if is_file_upload {
            return Ok(response_data);
        }

        if let Some(data) = response_data.get("data") {
            if let Some(encryptor) = &self.encryptor {
                if let Some(encrypted_str) = data.as_str() {
                    let decrypted = encryptor.decrypt(encrypted_str)?;
                    return Ok(serde_json::from_str(&decrypted)?);
                }
            }
        }

        Ok(response_data)
    }

    pub async fn make_request_with_retry(
        &self,
        email: &str,
        org_id: &str,
        endpoint: &str,
        data: Option<&Value>,
        user_id: Option<&str>,
        method: &str,
        files: Option<HashMap<String, Vec<u8>>>,
    ) -> Result<Value> {
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            match self.make_request(email, org_id, user_id.unwrap_or(""), endpoint, data, method, files.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries - 1 {
                        let sleep_time = self.retry_delay * (2_u64.pow(attempt as u32));
                        sleep(Duration::from_secs(sleep_time)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Max retries exceeded")))
    }

    pub async fn send_user_checkin(
        &self,
        checkin_time: &str,
        checkout_time: Option<&str>,
        breaks: Vec<Value>,
        break_durations: Vec<f64>,
        time_logged: f64,
    ) -> Result<Value> {
        let data = json!({
            "org_id": self.org_id,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "email": self.user_email,
            "checkin_time": checkin_time,
            "checkout_time": checkout_time,
            "breaks": breaks,
            "break_duration": break_durations,
            "total_elapsed_time": time_logged,
            "timestamp": get_current_utc_timestamp()
        });

        validate_required_fields(&data, &["checkin_time"])?;

        self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "publish/user_checkin",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await
    }

    pub async fn log_user_activity_summary(
        &self,
        start_time: &str,
        end_time: &str,
        keystroke_list: Vec<Value>,
        mouse_movement_list: Vec<Value>,
        touch_list: Vec<Value>,
    ) -> Result<Value> {
        let data = json!({
            "email": self.user_email,
            "org_id": self.org_id,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "start_time": start_time,
            "end_time": end_time,
            "keystroke_list": keystroke_list,
            "mouse_movement_list": mouse_movement_list,
            "touch_events_list": touch_list,
            "timestamp": get_current_utc_timestamp()
        });

        validate_required_fields(&data, &["start_time", "end_time"])?;

        self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "publish/user_activity_summary",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await
    }

    pub async fn log_active_window_bulk(&self, window_data_list: Vec<Value>) -> Result<Value> {
        let data = json!({
            "email": self.user_email,
            "org_id": self.org_id,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "windows": window_data_list
        });

        self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "publish/active_window",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await
    }

    pub async fn log_user_inactivity(
        &self,
        inactive_start_time: &str,
        inactive_end_time: Option<&str>,
        inactivity_by: &str,
        keystroke_count: u32,
        mouse_movement_count: u32,
        is_microphone_in_use: bool,
    ) -> Result<Value> {
        let data = json!({
            "email": self.user_email,
            "org_id": self.org_id,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "inactive_start_time": inactive_start_time,
            "inactive_end_time": inactive_end_time,
            "inactivity_by": inactivity_by,
            "keystroke_count": keystroke_count,
            "mouse_movement_count": mouse_movement_count,
            "is_microphone_in_use": is_microphone_in_use,
            "timestamp": get_current_utc_timestamp()
        });

        validate_required_fields(&data, &["inactive_start_time"])?;

        self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "publish/user_inactivity",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await
    }

    pub async fn log_user_call(
        &self,
        call_start_time: &str,
        call_end_time: Option<&str>,
        duration: f64,
        reason: &str,
    ) -> Result<Value> {
        let data = json!({
            "email": self.user_email,
            "org_id": self.org_id,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "call_start_time": call_start_time,
            "call_end_time": call_end_time,
            "duration": duration,
            "type": reason,
            "timestamp": get_current_utc_timestamp()
        });

        validate_required_fields(&data, &["call_start_time", "type"])?;

        self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "publish/call_log",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await
    }

    pub async fn check_primary_device(&self, email: &str, org_id: &str, user_id: &str) -> Result<Value> {
        let data = json!({"email": email, "org_id": org_id, "user_id": user_id});
        validate_required_fields(&data, &["email", "org_id", "user_id"])?;

        self.make_request_with_retry(email, org_id, "is_primary_device", Some(&data), Some(user_id), "POST", None).await
    }

    pub async fn user_check(&self, license_key: &str, email: &str, org_id: &str, user_id: &str) -> Result<Value> {
        let data = json!({"email": email, "license_key": license_key, "org_id": org_id, "user_id": user_id});

        self.make_request_with_retry(email, org_id, "user_check", Some(&data), Some(user_id), "POST", None).await
    }

    pub async fn register_device(&self, user_id: &str, email: &str, org_id: &str, device_info: Value) -> Result<Value> {
        let data = json!({"user_id": user_id, "email": email, "org_id": org_id, "device_info": device_info});
        validate_required_fields(&data, &["user_id", "email", "org_id", "device_info"])?;

        self.make_request_with_retry(email, org_id, "register_device", Some(&data), Some(user_id), "POST", None).await
    }

    pub async fn set_primary_device(&self, user_id: &str, email: &str, org_id: &str, mut device_info: Value) -> Result<Value> {
        device_info["email"] = json!(email);
        device_info["org_id"] = json!(org_id);
        validate_required_fields(&device_info, &["email", "org_id", "selected_device_id"])?;

        self.make_request_with_retry(email, org_id, "set_primary_device", Some(&device_info), Some(user_id), "POST", None).await
    }

    pub async fn get_user_data(&self, email: &str, org_id: &str) -> Result<Value> {
        let data = json!({"email": email, "org_id": org_id, "timestamp": get_current_utc_timestamp()});
        validate_required_fields(&data, &["email", "org_id"])?;

        self.make_request_with_retry(email, org_id, "user_data", Some(&data), None, "POST", None).await
    }

    pub async fn log_activity_data(&self, user_id: &str, email: &str, org_id: &str, activity_data: Value) -> Result<(bool, u16)> {
        let entries = if activity_data.is_array() {
            activity_data.as_array().unwrap().clone()
        } else {
            vec![activity_data]
        };

        for entry in &entries {
            validate_required_fields(entry, &["email", "org_id"])?;
        }

        let payload = json!({"user_id": user_id, "device_id": self.device_id, "email": email, "org_id": org_id, "data": entries});

        let response = self.make_request_with_retry(email, org_id, "api/v1/log_data", Some(&payload), Some(user_id), "POST", None).await?;

        let status_code = response.get("status_code").and_then(|v| v.as_u64()).unwrap_or(
            response.get("status").and_then(|v| v.as_u64()).unwrap_or(500)
        ) as u16;

        if status_code == 200 || status_code == 201 || response.get("status").and_then(|v| v.as_str()) == Some("success") {
            Ok((true, 200))
        } else if status_code == 403 {
            log::error!("Data logging failed: Forbidden (403)");
            Ok((false, 403))
        } else {
            log::error!("Data logging failed: {:?}", response);
            Ok((false, 500))
        }
    }

    pub async fn check_login(&self, email: &str, org_id: &str) -> bool {
        let payload = json!({"org_id": org_id, "email": email});
        match self.make_request_with_retry(email, org_id, "login-check", Some(&payload), None, "GET", None).await {
            Ok(response) => response.get("logged_in").and_then(|v| v.as_bool()).unwrap_or(false),
            Err(e) => {
                log::error!("Error checking login: {:?}", e);
                false
            }
        }
    }

    pub async fn mark_first_login_complete(&self, email: &str, user_id: &str, org_id: &str) -> bool {
        let payload = json!({"user_id": user_id, "org_id": org_id});
        match self.make_request_with_retry(email, org_id, "first-login-complete", Some(&payload), Some(user_id), "POST", None).await {
            Ok(response) => response.get("status").and_then(|v| v.as_str()) == Some("success"),
            Err(e) => {
                log::error!("Error marking first login complete: {:?}", e);
                false
            }
        }
    }

    pub async fn send_for_approval(&self, reason: &str, duration: f64, needs_approval: bool) -> Result<Value> {
        let request_id = Uuid::new_v4().to_string();
        let data = json!({
            "user_id": self.user_id,
            "email": self.user_email.as_ref().map(|s| s.trim().to_lowercase()),
            "org_id": self.org_id,
            "device_id": self.device_id,
            "reason": reason,
            "duration": duration,
            "needs_approval": needs_approval,
            "request_id": request_id
        });

        validate_required_fields(&data, &["user_id", "email", "org_id", "reason"])?;

        let response = self.make_request_with_retry(
            self.user_email.as_deref().unwrap_or(""),
            self.org_id.as_deref().unwrap_or(""),
            "api/v1/log_data",
            Some(&data),
            self.user_id.as_deref(),
            "POST",
            None,
        ).await?;

        if needs_approval {
            self.check_approval_status(&request_id, 10, 5).await
        } else {
            Ok(response)
        }
    }

    pub async fn check_approval_status(&self, request_id: &str, max_retries: usize, delay: u64) -> Result<Value> {
        for attempt in 0..max_retries {
            let data = json!({"request_id": request_id, "timestamp": get_current_utc_timestamp()});
            match self.make_request_with_retry("", "", "check_approval", Some(&data), None, "GET", None).await {
                Ok(response) => {
                    if let Some(status) = response.get("status").and_then(|v| v.as_str()) {
                        if status == "approved" || status == "declined" {
                            return Ok(response);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Approval check failed (attempt {}): {:?}", attempt + 1, e);
                }
            }
            if attempt < max_retries - 1 {
                sleep(Duration::from_secs(delay)).await;
            }
        }

        log::info!("Auto-approving request_id: {} due to timeout", request_id);
        Ok(json!({"status": "approved", "message": "Request approved due to timeout"}))
    }

    pub async fn upload_logs(&self, user_id: &str, email: &str, org_id: &str, log_files: Vec<String>) -> Result<Value> {
        let mut files_data = Vec::new();

        for file_path in log_files {
            if !file_path.to_lowercase().ends_with(".log") && !file_path.to_lowercase().ends_with(".txt") {
                continue;
            }

            match fs::read(&file_path) {
                Ok(content) => {
                    let encoded = general_purpose::STANDARD.encode(&content);
                    let filename = std::path::Path::new(&file_path).file_name().unwrap().to_string_lossy();
                    files_data.push(json!({
                        "filename": filename,
                        "content": encoded
                    }));
                }
                Err(e) => {
                    log::error!("Failed to process log file {}: {:?}", file_path, e);
                }
            }
        }

        let payload = json!({
            "user_id": user_id,
            "email": email,
            "org_id": org_id,
            "log_files": files_data,
            "timestamp": get_current_utc_timestamp()
        });

        self.make_request_with_retry(email, org_id, "logs", Some(&payload), Some(user_id), "POST", None).await
    }

    pub async fn upload_screenshots(&self, user_id: &str, email: &str, org_id: &str, screenshot_files: Vec<String>) -> Result<Value> {
        let mut valid_files = Vec::new();

        for file_path in screenshot_files {
            match fs::read(&file_path) {
                Ok(content) => {
                    let encoded = general_purpose::STANDARD.encode(&content);
                    let filename = std::path::Path::new(&file_path).file_name().unwrap().to_string_lossy();
                    valid_files.push(json!({
                        "filename": filename,
                        "data": encoded
                    }));
                }
                Err(e) => {
                    log::error!("Failed to process screenshot {}: {:?}", file_path, e);
                }
            }
        }

        let payload = json!({
            "user_id": user_id,
            "email": email,
            "org_id": org_id,
            "timestamp": get_current_utc_timestamp(),
            "client": "rust-uploader",
            "file_count": valid_files.len(),
            "screenshots": valid_files
        });

        self.make_request_with_retry(email, org_id, "log_screenshot", Some(&payload), Some(user_id), "POST", None).await
    }

    pub async fn upload_videos(&self, user_id: &str, email: &str, org_id: &str, video_files: Vec<String>) -> Result<Value> {
        let mut files = HashMap::new();

        for file_path in video_files {
            match tokio::fs::read(&file_path).await {
                Ok(content) => {
                    let filename = std::path::Path::new(&file_path).file_name().unwrap().to_string_lossy().to_string();
                    files.insert(filename, content);
                }
                Err(e) => {
                    log::error!("Failed to process video {}: {:?}", file_path, e);
                }
            }
        }

        let form_data = json!({
            "email": email.to_lowercase(),
            "org_id": org_id,
            "user_id": user_id
        });

        self.make_request_with_retry(email, org_id, "log_video", Some(&form_data), Some(user_id), "POST", Some(files)).await
    }
}
