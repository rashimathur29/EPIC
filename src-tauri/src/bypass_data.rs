// src/bypass_data.rs
//
// ALL static / test-mode data lives here.
// Nothing in auth_commands.rs or anywhere else should hard-code
// org names, user fields, passwords, plan types, etc.
// Add new fields here; the rest of the code just references these consts.

// ─────────────────────────────────────────────────────────────
// BYPASS PASSWORD
// In bypass mode this is the only accepted password.
// Change here once to update everywhere.
// ─────────────────────────────────────────────────────────────
pub const BYPASS_PASSWORD: &str = "12345678";

// ─────────────────────────────────────────────────────────────
// STATIC ORG DEFAULTS
// Used when building the deterministic org from a domain.
// ─────────────────────────────────────────────────────────────
pub const ORG_PLAN_TYPE:    &str = "Monthly";
pub const ORG_DEPLOYMENT:   &str = "Cloud";
pub const ORG_IS_ACTIVE:    i32  = 1;
pub const ORG_SSO_ENABLED:  i32  = 0;
/// How many days before today the plan "started"
pub const ORG_START_OFFSET_DAYS: i64 = 30;
/// How many days from today the plan "expires"
pub const ORG_END_OFFSET_DAYS:   i64 = 335;
/// Suffix appended to the org display name, e.g. "Acme (Test)"
pub const ORG_NAME_SUFFIX:  &str = "(Test)";
/// Token returned for every bypass login
pub const BYPASS_TOKEN:     &str = "bypass-token-xyz";

// ─────────────────────────────────────────────────────────────
// STATIC USER DEFAULTS
// Used when building the deterministic user from an email.
// ─────────────────────────────────────────────────────────────
pub const USER_LAST_NAME:    &str = "User";
pub const USER_DESIGNATION:  &str = "Software Engineer";
pub const USER_PHONE:        &str = "+91-9999999999";
pub const USER_SHIFT_TIMING: &str = "09:00-18:00";
pub const USER_TIMEZONE:     &str = "Asia/Kolkata";
