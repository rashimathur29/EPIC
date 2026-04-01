use chrono::{Utc};
use anyhow::{Result, anyhow};

#[macro_export]
macro_rules! log_function_call {
    () => {
        log::info!("Function called: {}", std::function_name!());
    };
}

pub fn validate_required_fields(data: &serde_json::Value, required_fields: &[&str]) -> Result<()> {
    for field in required_fields {
        if data.get(field).is_none() {
            return Err(anyhow!("Missing required field: {}", field));
        }
    }
    Ok(())
}

pub fn get_current_utc_timestamp() -> String {
    Utc::now().to_rfc3339()
}
