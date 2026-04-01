// src/env_config.rs
// Loads the correct .env.{env} file based on APP_ENV env var.
// Falls back to .env.dev if not set.
// All fields are read once at startup and stored in AppEnvConfig.

use std::env;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEnvConfig {
    pub app_env: String,
    pub org_url: String,
    pub login_url: String,
    pub version_url: String,
    pub status_url: String,
    pub productivity_api_url: String,
    pub product_name: String,
    pub app_name: String,
    pub encryption_key: String,
    /// When true, all API calls return static mock data (testing mode)
    pub bypass_api: bool,
}

impl AppEnvConfig {
    /// Load config from the appropriate .env.{env} file.
    /// Call this once at app startup before any API calls.
    pub fn load() -> Self {
        // 1. Determine which env to load
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "dev".to_string());

        // 2. Try to load the env-specific file from several candidate paths:
        //    - next to the binary  (release)
        //    - project root        (cargo run / dev)
        let file_name = format!(".env.{}", app_env);
        let candidates = vec![
            PathBuf::from(&file_name),
            PathBuf::from("src-tauri").join(&file_name),
            PathBuf::from("..").join(&file_name),
        ];

        for path in &candidates {
            if path.exists() {
                dotenv::from_path(path).ok();
                log::info!("[ENV] Loaded {}", path.display());
                break;
            }
        }

        // 3. Fallback: try plain dotenv (reads .env in cwd)
        dotenv::dotenv().ok();

        Self {
            app_env: app_env.clone(),
            org_url: env::var("ORG_URL")
                .unwrap_or_else(|_| "http://103.224.246.148:8444/api/org/getOrgConfig".to_string()),
            login_url: env::var("LOGIN_URL")
                .unwrap_or_else(|_| "http://103.224.246.148:8444/api/v1/auth/login".to_string()),
            version_url: env::var("VERSION_URL")
                .unwrap_or_else(|_| "http://103.224.246.148:8444/api/utility/app-version".to_string()),
            status_url: env::var("STATUS_URL")
                .unwrap_or_else(|_| "http://103.224.246.148:8444/api/utility/user-active".to_string()),
            productivity_api_url: env::var("PRODUCTIVITY_API_URL")
                .unwrap_or_else(|_| "http://103.224.246.148:8080".to_string()),
            product_name: env::var("PRODUCT_NAME").unwrap_or_else(|_| "epic".to_string()),
            app_name: env::var("APP_NAME").unwrap_or_else(|_| "EPIC".to_string()),
            encryption_key: env::var("ENCRYPTION_KEY")
                .unwrap_or_else(|_| "Xub0K6p17TMHV9e+A2Nt2/c4f+xXw6EeD4XqJI143xs=".to_string()),
            bypass_api: env::var("BYPASS_API")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(false),
        }
    }

    /// Convenience: is this a testing / bypass environment?
    pub fn is_bypass(&self) -> bool {
        self.bypass_api
    }
}
