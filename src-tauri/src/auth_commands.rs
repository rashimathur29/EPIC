// src/auth_commands.rs
//
// BYPASS MODE (BYPASS_API=true):
//   validate_organization(domain)  → any domain → static org → org_config table
//   login_user(email, password)    → email + BYPASS_PASSWORD → static user
//                                    → user_details + user_session tables
//
// LIVE MODE (BYPASS_API=false):
//   → Hits real ORG_URL / LOGIN_URL APIs (AES-256 encrypted payloads)
//
// Static test data is defined in bypass_data.rs 

use tauri::{command, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use rusqlite::params;
use chrono::{Utc, Duration};

use crate::AppState;
use crate::env_config::AppEnvConfig;
use crate::encryption_utils::AES256Encryptor;
use crate::bypass_data::{
    BYPASS_PASSWORD, BYPASS_TOKEN,
    ORG_PLAN_TYPE, ORG_DEPLOYMENT, ORG_IS_ACTIVE, ORG_SSO_ENABLED,
    ORG_START_OFFSET_DAYS, ORG_END_OFFSET_DAYS, ORG_NAME_SUFFIX,
    USER_LAST_NAME, USER_DESIGNATION, USER_PHONE, USER_SHIFT_TIMING, USER_TIMEZONE,
};

// ─────────────────────────────────────────────────────────────
// Public response types (returned to JS)
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrgValidationResult {
    pub valid:       bool,
    pub message:     String,
    pub org_id:      Option<String>,
    pub name:        Option<String>,
    pub sso_enabled: bool,
    pub logo_url:    Option<String>,
    pub domain:      Option<String>,
    pub plan_type:   Option<String>,
    pub is_active:   bool,
    pub deployment:  Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginResult {
    pub success:        bool,
    pub message:        String,
    pub user_id:        Option<String>,
    pub email:          Option<String>,
    pub first_name:     Option<String>,
    pub last_name:      Option<String>,
    pub org_id:         Option<String>,
    pub token:          Option<String>,
    pub is_first_login: bool,
    pub designation:    Option<String>,
    pub phone_number:   Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CurrentUser {
    pub logged_in:  bool,
    pub kicked:     bool,
    pub user_id:    Option<String>,
    pub email:      Option<String>,
    pub first_name: Option<String>,
    pub last_name:  Option<String>,
    pub org_id:     Option<String>,
    pub org_name:   Option<String>,
    pub org_domain: Option<String>,
}

// ─────────────────────────────────────────────────────────────
// ID derivation helpers
// ─────────────────────────────────────────────────────────────

fn org_id_from_domain(domain: &str) -> String {
    let slug = domain
        .to_uppercase()
        .replace('.', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
    format!("ORG-{}", slug)
}

fn user_id_from_email(email: &str) -> String {
    let slug = email
        .to_uppercase()
        .replace('@', "-AT-")
        .replace('.', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
    format!("USR-{}", slug)
}

fn first_name_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("User");
    let mut chars = local.chars();
    match chars.next() {
        None    => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ─────────────────────────────────────────────────────────────
// Static data builders (all consts from bypass_data.rs)
// ─────────────────────────────────────────────────────────────

struct StaticOrg {
    org_id:       String,
    org_name:     String,
    domain:       String,
    logo_url:     String,
    plan_type:    &'static str,
    start_date:   String,
    end_date:     String,
    deployment:   &'static str,
    api_base_url: String,
    is_active:    i32,
    sso_enabled:  i32,
}

fn build_static_org(domain: &str) -> StaticOrg {
    let today = Utc::now().date_naive();
    let start = today - Duration::days(ORG_START_OFFSET_DAYS);
    let end   = today + Duration::days(ORG_END_OFFSET_DAYS);

    let base = domain.split('.').next().unwrap_or(domain);
    let mut chars = base.chars();
    let display_base = match chars.next() {
        None    => domain.to_string(),
        Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
    };

    StaticOrg {
        org_id:       org_id_from_domain(domain),
        org_name:     format!("{} {}", display_base, ORG_NAME_SUFFIX),
        domain:       domain.to_string(),
        logo_url:     format!("https://logo.clearbit.com/{}", domain),
        plan_type:    ORG_PLAN_TYPE,
        start_date:   start.format("%Y-%m-%d").to_string(),
        end_date:     end.format("%Y-%m-%d").to_string(),
        deployment:   ORG_DEPLOYMENT,
        api_base_url: format!("https://api.{}", domain),
        is_active:    ORG_IS_ACTIVE,
        sso_enabled:  ORG_SSO_ENABLED,
    }
}

struct StaticUser {
    user_id:      String,
    email:        String,
    first_name:   String,
    last_name:    &'static str,
    org_id:       String,
    designation:  &'static str,
    phone_number: &'static str,
    shift_timing: &'static str,
    timezone:     &'static str,
}

fn build_static_user(email: &str, org_id: &str) -> StaticUser {
    StaticUser {
        user_id:      user_id_from_email(email),
        email:        email.to_string(),
        first_name:   first_name_from_email(email),
        last_name:    USER_LAST_NAME,
        org_id:       org_id.to_string(),
        designation:  USER_DESIGNATION,
        phone_number: USER_PHONE,
        shift_timing: USER_SHIFT_TIMING,
        timezone:     USER_TIMEZONE,
    }
}

// ─────────────────────────────────────────────────────────────
// Timestamp helpers
// ─────────────────────────────────────────────────────────────

fn now_local() -> String { chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string() }
fn now_utc()   -> String { Utc::now().to_rfc3339() }

// ─────────────────────────────────────────────────────────────
// DB persistence — single unified helper for both bypass + live
// ─────────────────────────────────────────────────────────────

fn db_save_org_bypass(state: &AppState, o: &StaticOrg) -> rusqlite::Result<()> {
    let conn = state.db.conn.lock().unwrap();
    let now  = now_local();
    let utc  = now_utc();
    conn.execute(
        "INSERT OR REPLACE INTO org_config
            (org_id, org_name, domain, sso_enabled,
             logo_url, plan_type, start_date, end_date,
             deployment, api_base_url, is_active,
             created_at, updated_at, apscreatedatetime, apsupdatedatetime)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            o.org_id, o.org_name, o.domain, o.sso_enabled,
            o.logo_url, o.plan_type, o.start_date, o.end_date,
            o.deployment, o.api_base_url, o.is_active,
            now, now, utc, utc,
        ],
    )?;
    log::info!("[AUTH] Bypass org saved: {} ({})", o.org_name, o.org_id);
    Ok(())
}

fn db_save_org_live(state: &AppState, r: &OrgValidationResult) -> rusqlite::Result<()> {
    let conn = state.db.conn.lock().unwrap();
    let now  = now_local();
    let utc  = now_utc();
    conn.execute(
        "INSERT OR REPLACE INTO org_config
            (org_id, org_name, domain, sso_enabled,
             logo_url, plan_type, start_date, end_date,
             deployment, api_base_url, is_active,
             created_at, updated_at, apscreatedatetime, apsupdatedatetime)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            r.org_id.as_deref().unwrap_or(""),
            r.name.as_deref().unwrap_or(""),
            r.domain.as_deref().unwrap_or(""),
            r.sso_enabled as i32,
            r.logo_url.as_deref().unwrap_or(""),
            r.plan_type.as_deref().unwrap_or(""),
            "", "",
            r.deployment.as_deref().unwrap_or(""),
            "",
            r.is_active as i32,
            now, now, utc, utc,
        ],
    )?;
    Ok(())
}

/// Unified DB save for login — writes BOTH user_details AND user_session.
/// Called from both bypass and live paths so neither can forget a table.
#[allow(clippy::too_many_arguments)]
fn db_save_login(
    state:          &AppState,
    user_id:        &str,
    email:          &str,
    first_name:     &str,
    last_name:      &str,
    org_id:         &str,
    designation:    &str,
    phone_number:   &str,
    shift_timing:   &str,
    timezone:       &str,
    token:          &str,
    is_first_login: bool,
) -> rusqlite::Result<()> {
    let conn = state.db.conn.lock().unwrap();
    let now  = now_local();
    let utc  = now_utc();

    // 1. Upsert user_details — every field is refreshed on login
    conn.execute(
        "INSERT INTO user_details
            (user_id, email, first_name, last_name, org_id,
             designation, phone_number, shift_timing, timezone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(email) DO UPDATE SET
             user_id      = excluded.user_id,
             first_name   = excluded.first_name,
             last_name    = excluded.last_name,
             org_id       = excluded.org_id,
             designation  = excluded.designation,
             phone_number = excluded.phone_number,
             shift_timing = excluded.shift_timing,
             timezone     = excluded.timezone",
        params![
            user_id, email, first_name, last_name, org_id,
            designation, phone_number, shift_timing, timezone,
        ],
    )?;
    log::info!("[AUTH] user_details upserted for: {}", email);

    // 2. Replace user_session (always a single row, id = 1)
    conn.execute(
        "INSERT OR REPLACE INTO user_session
            (id, user_id, email, first_name, last_name, org_id,
             token, is_first_login, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            user_id, email, first_name, last_name, org_id,
            token, is_first_login as i32, now, utc,
        ],
    )?;
    log::info!("[AUTH] user_session saved for: {}", email);

    Ok(())
}

fn db_clear_session(state: &AppState) -> rusqlite::Result<()> {
    let conn = state.db.conn.lock().unwrap();
    conn.execute("DELETE FROM user_session WHERE id = 1", [])?;
    Ok(())
}

fn db_wipe_after_kick(state: &AppState) -> rusqlite::Result<()> {
    let conn = state.db.conn.lock().unwrap();
    conn.execute("DELETE FROM user_session WHERE id = 1", [])?;
    conn.execute("DELETE FROM org_config", [])?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────
// Live API helpers (AES-256)
// ─────────────────────────────────────────────────────────────

fn decrypt_response(cfg: &AppEnvConfig, body: &Value) -> Option<Value> {
    if let Some(data_str) = body.get("data").and_then(|d| d.as_str()) {
        if let Ok(enc) = AES256Encryptor::new(&cfg.encryption_key) {
            if let Ok(plain) = enc.decrypt(data_str) {
                if let Ok(v) = serde_json::from_str::<Value>(&plain) {
                    return Some(v);
                }
            }
        }
    }
    None
}

async fn api_validate_org(cfg: &AppEnvConfig, domain: &str) -> OrgValidationResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().unwrap();

    let enc = match AES256Encryptor::new(&cfg.encryption_key) {
        Ok(e)  => e,
        Err(_) => return org_err(domain, "Encryption init failed"),
    };
    let encrypted = match enc.encrypt(&json!({ "domain": domain }).to_string()) {
        Ok(e)  => e,
        Err(_) => return org_err(domain, "Encryption failed"),
    };

    match client.post(&cfg.org_url).json(&json!({ "data": encrypted })).send().await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(body) => {
                let data    = decrypt_response(cfg, &body).unwrap_or(body);
                let status  = data.get("status").and_then(|s| s.as_str()).unwrap_or("");
                let success = status == "success" || data.get("org_id").is_some();
                OrgValidationResult {
                    valid: success,
                    message: if success { "Organization found".into() } else {
                        data.get("message").and_then(|m| m.as_str())
                            .unwrap_or("Organization not found").to_string()
                    },
                    org_id:     data.get("org_id").and_then(|v| v.as_str()).map(String::from),
                    name:       data.get("org_name").and_then(|v| v.as_str()).map(String::from),
                    sso_enabled: data.get("sso_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                    logo_url:   data.get("logo_url").and_then(|v| v.as_str()).map(String::from),
                    domain:     Some(domain.to_string()),
                    plan_type:  data.get("plan_type").and_then(|v| v.as_str()).map(String::from),
                    is_active:  data.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true),
                    deployment: data.get("deployment").and_then(|v| v.as_str()).map(String::from),
                }
            }
            Err(e) => org_err(domain, &format!("Invalid server response: {}", e)),
        },
        Err(e) => org_err(domain, &format!("Could not reach server: {}", e)),
    }
}

fn org_err(domain: &str, msg: &str) -> OrgValidationResult {
    OrgValidationResult {
        valid: false, message: msg.to_string(),
        org_id: None, name: None, sso_enabled: false, logo_url: None,
        domain: Some(domain.to_string()), plan_type: None,
        is_active: false, deployment: None,
    }
}

async fn api_login(cfg: &AppEnvConfig, email: &str, password: &str, org_id: &str) -> LoginResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().unwrap();

    let enc = match AES256Encryptor::new(&cfg.encryption_key) {
        Ok(e)  => e,
        Err(_) => return login_err("Encryption init failed"),
    };
    let encrypted = match enc.encrypt(
        &json!({ "email": email, "password": password, "org_id": org_id }).to_string()
    ) {
        Ok(e)  => e,
        Err(_) => return login_err("Encryption failed"),
    };

    let payload = json!({ "data": encrypted, "org_id": org_id, "email": email });

    match client.post(&cfg.login_url).json(&payload).send().await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(body) => {
                let data    = decrypt_response(cfg, &body).unwrap_or(body);
                let status  = data.get("status").and_then(|s| s.as_str()).unwrap_or("");
                let success = status == "success" || data.get("user_id").is_some();
                LoginResult {
                    success,
                    message: if success { "Login successful".into() } else {
                        data.get("message").and_then(|m| m.as_str())
                            .unwrap_or("Invalid credentials").to_string()
                    },
                    user_id:        data.get("user_id").and_then(|v| v.as_str()).map(String::from),
                    email:          Some(email.to_string()),
                    first_name:     data.get("first_name").and_then(|v| v.as_str()).map(String::from),
                    last_name:      data.get("last_name").and_then(|v| v.as_str()).map(String::from),
                    org_id:         data.get("org_id").and_then(|v| v.as_str())
                                        .map(String::from).or(Some(org_id.to_string())),
                    token:          data.get("token").and_then(|v| v.as_str()).map(String::from),
                    is_first_login: data.get("is_first_login").and_then(|v| v.as_bool()).unwrap_or(false),
                    designation:    data.get("designation").and_then(|v| v.as_str()).map(String::from),
                    phone_number:   data.get("phone_number").and_then(|v| v.as_str()).map(String::from),
                }
            }
            Err(e) => login_err(&format!("Invalid server response: {}", e)),
        },
        Err(e) => login_err(&format!("Could not reach server: {}", e)),
    }
}

fn login_err(msg: &str) -> LoginResult {
    LoginResult {
        success: false, message: msg.to_string(),
        user_id: None, email: None, first_name: None, last_name: None,
        org_id: None, token: None, is_first_login: false,
        designation: None, phone_number: None,
    }
}

// ─────────────────────────────────────────────────────────────
// TAURI COMMANDS
// ─────────────────────────────────────────────────────────────

/// Step 1 — domain lookup (find_your_organization.html)
#[command]
pub async fn validate_organization(
    domain:  String,
    state:   State<'_, AppState>,
    env_cfg: State<'_, AppEnvConfig>,
) -> Result<OrgValidationResult, String> {
    log::info!("[AUTH] validate_organization: domain={}", domain);

    let domain = domain.trim().to_lowercase();
    if domain.is_empty() || domain == "__bypass_check__" {
        return Ok(org_err(&domain, "Domain cannot be empty"));
    }

    if env_cfg.is_bypass() {
        log::info!("[AUTH] BYPASS — generating static org for: {}", domain);
        let org = build_static_org(&domain);
        let result = OrgValidationResult {
            valid:       true,
            message:     "Organization found".into(),
            org_id:      Some(org.org_id.clone()),
            name:        Some(org.org_name.clone()),
            sso_enabled: org.sso_enabled == 1,
            logo_url:    Some(org.logo_url.clone()),
            domain:      Some(domain.clone()),
            plan_type:   Some(org.plan_type.to_string()),
            is_active:   org.is_active == 1,
            deployment:  Some(org.deployment.to_string()),
        };
        if let Err(e) = db_save_org_bypass(&state, &org) {
            log::warn!("[AUTH] Failed to save bypass org: {}", e);
        }
        crate::timestamp::TimestampManager::init(&state.db);
        return Ok(result);
    }

    let result = api_validate_org(&env_cfg, &domain).await;
    if result.valid {
        if let Err(e) = db_save_org_live(&state, &result) {
            log::warn!("[AUTH] Failed to save live org: {}", e);
        }
    }
    Ok(result)
}

/// Step 2 — sign-in (login.html)
///
/// BYPASS: only BYPASS_PASSWORD is accepted. Any email works.
///         Both user_details and user_session are written on success.
/// LIVE:   hits LOGIN_URL API → writes both tables on success.
#[command]
pub async fn login_user(
    email:    String,
    password: String,
    org_id:   String,
    state:    State<'_, AppState>,
    env_cfg:  State<'_, AppEnvConfig>,
) -> Result<LoginResult, String> {
    log::info!("[AUTH] login_user: email={}", email);

    let email = email.trim().to_lowercase();
    if email.is_empty() || password.is_empty() {
        return Ok(login_err("Email and password are required"));
    }

    if env_cfg.is_bypass() {
        // Strict password check — return a clear error so the user knows what to type
        if password != BYPASS_PASSWORD {
            log::info!("[AUTH] BYPASS — wrong password for: {}", email);
            return Ok(LoginResult {
                success: false,
                message: format!(
                    "Incorrect password. In test mode use '{}'.",
                    BYPASS_PASSWORD
                ),
                user_id: None, email: None, first_name: None, last_name: None,
                org_id: None, token: None, is_first_login: false,
                designation: None, phone_number: None,
            });
        }

        log::info!("[AUTH] BYPASS — generating static user for: {}", email);
        let user = build_static_user(&email, &org_id);

        if let Err(e) = db_save_login(
            &state,
            &user.user_id,
            &user.email,
            &user.first_name,
            user.last_name,
            &user.org_id,
            user.designation,
            user.phone_number,
            user.shift_timing,
            user.timezone,
            BYPASS_TOKEN,
            false,
        ) {
            log::warn!("[AUTH] Failed to save bypass user to DB: {}", e);
        }
        crate::timestamp::TimestampManager::init(&state.db);

        return Ok(LoginResult {
            success:        true,
            message:        "Login successful".into(),
            user_id:        Some(user.user_id),
            email:          Some(user.email),
            first_name:     Some(user.first_name),
            last_name:      Some(user.last_name.to_string()),
            org_id:         Some(org_id),
            token:          Some(BYPASS_TOKEN.to_string()),
            is_first_login: false,
            designation:    Some(user.designation.to_string()),
            phone_number:   Some(user.phone_number.to_string()),
        });
    }

    // Live mode
    let result = api_login(&env_cfg, &email, &password, &org_id).await;
    if result.success {
        if let Err(e) = db_save_login(
            &state,
            result.user_id.as_deref().unwrap_or(""),
            result.email.as_deref().unwrap_or(""),
            result.first_name.as_deref().unwrap_or(""),
            result.last_name.as_deref().unwrap_or(""),
            result.org_id.as_deref().unwrap_or(""),
            result.designation.as_deref().unwrap_or(""),
            result.phone_number.as_deref().unwrap_or(""),
            "",
            "",
            result.token.as_deref().unwrap_or(""),
            result.is_first_login,
        ) {
            log::warn!("[AUTH] Failed to save live login to DB: {}", e);
        }
    }
    Ok(result)
}

/// Called on every app launch — cross-checks session vs user_details.
#[command]
pub fn get_current_user(state: State<'_, AppState>) -> Result<CurrentUser, String> {
    let conn = state.db.conn.lock().unwrap();

    let session: Option<(String, String, String, String, String)> = conn.query_row(
        "SELECT user_id, email, first_name, last_name, org_id
         FROM user_session WHERE id = 1 LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).ok();

    let (user_id, email, first_name, last_name, org_id) = match session {
        None => {
            log::info!("[AUTH] get_current_user: no session");
            return Ok(CurrentUser {
                logged_in: false, kicked: false,
                user_id: None, email: None, first_name: None,
                last_name: None, org_id: None, org_name: None, org_domain: None,
            });
        }
        Some(s) => s,
    };

    let user_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM user_details WHERE email = ?1 AND user_id = ?2",
        params![&email, &user_id],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !user_exists {
        log::warn!("[AUTH] KICKED — session for '{}' but no user_details row", email);
        drop(conn);
        let _ = db_wipe_after_kick(&state);
        return Ok(CurrentUser {
            logged_in: false, kicked: true,
            user_id: Some(user_id), email: Some(email),
            first_name: None, last_name: None,
            org_id: None, org_name: None, org_domain: None,
        });
    }

    let (org_name, org_domain): (Option<String>, Option<String>) = conn.query_row(
        "SELECT org_name, domain FROM org_config WHERE org_id = ?1 LIMIT 1",
        params![&org_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap_or((None, None));

    log::info!("[AUTH] get_current_user: valid session for '{}'", email);
    Ok(CurrentUser {
        logged_in: true, kicked: false,
        user_id:    Some(user_id),
        email:      Some(email),
        first_name: Some(first_name),
        last_name:  Some(last_name),
        org_id:     Some(org_id),
        org_name,
        org_domain,
    })
}

/// Normal logout — clears session only, keeps user_details + org_config.
#[command]
pub fn logout(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("[AUTH] logout");
    db_clear_session(&state).map_err(|e| e.to_string())
}

#[command]
pub fn get_app_env(env_cfg: State<'_, AppEnvConfig>) -> String {
    env_cfg.app_env.clone()
}