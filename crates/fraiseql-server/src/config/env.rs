use std::env;
use std::time::Duration;

/// Resolve a value that may be an environment variable reference
pub fn resolve_env_value(value: &str) -> Result<String, EnvError> {
    if value.starts_with("${") && value.ends_with("}") {
        let var_name = &value[2..value.len()-1];

        // Support default values: ${VAR:-default}
        if let Some((name, default)) = var_name.split_once(":-") {
            return env::var(name).or_else(|_| Ok(default.to_string()));
        }

        // Support required with message: ${VAR:?message}
        if let Some((name, message)) = var_name.split_once(":?") {
            return env::var(name).map_err(|_| EnvError::MissingVarWithMessage {
                name: name.to_string(),
                message: message.to_string(),
            });
        }

        env::var(var_name).map_err(|_| EnvError::MissingVar {
            name: var_name.to_string()
        })
    } else {
        Ok(value.to_string())
    }
}

/// Get value from environment variable name stored in config
pub fn get_env_value(env_var_name: &str) -> Result<String, EnvError> {
    env::var(env_var_name).map_err(|_| EnvError::MissingVar {
        name: env_var_name.to_string()
    })
}

/// Parse size strings like "10MB", "1GB"
pub fn parse_size(s: &str) -> Result<usize, ParseError> {
    let s = s.trim();
    let s_upper = s.to_uppercase();

    let (num_str, multiplier) = if s_upper.ends_with("GB") {
        (&s[..s.len()-2], 1024 * 1024 * 1024)
    } else if s_upper.ends_with("MB") {
        (&s[..s.len()-2], 1024 * 1024)
    } else if s_upper.ends_with("KB") {
        (&s[..s.len()-2], 1024)
    } else if s_upper.ends_with("B") {
        (&s[..s.len()-1], 1)
    } else {
        // Assume bytes if no unit
        (s, 1)
    };

    let num: usize = num_str.trim().parse()
        .map_err(|_| ParseError::InvalidSize {
            value: s.to_string(),
            reason: "Invalid number".to_string(),
        })?;

    num.checked_mul(multiplier)
        .ok_or_else(|| ParseError::InvalidSize {
            value: s.to_string(),
            reason: "Value too large".to_string(),
        })
}

/// Parse duration strings like "30s", "5m", "1h"
pub fn parse_duration(s: &str) -> Result<Duration, ParseError> {
    let s = s.trim().to_lowercase();

    let (num_str, multiplier_ms) = if s.ends_with("ms") {
        (&s[..s.len()-2], 1u64)
    } else if s.ends_with('s') {
        (&s[..s.len()-1], 1000)
    } else if s.ends_with('m') {
        (&s[..s.len()-1], 60 * 1000)
    } else if s.ends_with('h') {
        (&s[..s.len()-1], 60 * 60 * 1000)
    } else if s.ends_with('d') {
        (&s[..s.len()-1], 24 * 60 * 60 * 1000)
    } else {
        return Err(ParseError::InvalidDuration {
            value: s,
            reason: "Missing unit (ms, s, m, h, d)".to_string(),
        });
    };

    let num: u64 = num_str.trim().parse()
        .map_err(|_| ParseError::InvalidDuration {
            value: s.clone(),
            reason: "Invalid number".to_string(),
        })?;

    Ok(Duration::from_millis(num * multiplier_ms))
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("Missing environment variable: {name}")]
    MissingVar { name: String },

    #[error("Missing environment variable {name}: {message}")]
    MissingVarWithMessage { name: String, message: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid size value '{value}': {reason}")]
    InvalidSize { value: String, reason: String },

    #[error("Invalid duration value '{value}': {reason}")]
    InvalidDuration { value: String, reason: String },
}
