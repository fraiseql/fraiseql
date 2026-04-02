//! Helpers for resolving configuration values from environment variables.
//!
//! Provides [`resolve_env_value`] which transparently dereferences values
//! that start with `$` as environment variable names, and parse utilities
//! for human-friendly size strings (`"10MB"`) and duration strings (`"30s"`).

use std::{env, time::Duration};

/// Resolve a value that may be an environment variable reference
///
/// # Errors
///
/// Returns `EnvError::MissingVar` if the referenced environment variable is not set.
/// Returns `EnvError::MissingVarWithMessage` if the variable uses the `:?` syntax and is not set.
pub fn resolve_env_value(value: &str) -> Result<String, EnvError> {
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];

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
            name: var_name.to_string(),
        })
    } else {
        Ok(value.to_string())
    }
}

/// Get value from environment variable name stored in config
///
/// # Errors
///
/// Returns `EnvError::MissingVar` if the named environment variable is not set.
pub fn get_env_value(env_var_name: &str) -> Result<String, EnvError> {
    env::var(env_var_name).map_err(|_| EnvError::MissingVar {
        name: env_var_name.to_string(),
    })
}

/// Parse size strings like "10MB", "1GB"
///
/// # Errors
///
/// Returns `ParseError::InvalidSize` if the string is not a valid size or the number overflows.
pub fn parse_size(s: &str) -> Result<usize, ParseError> {
    let s = s.trim();
    let s_upper = s.to_uppercase();

    let (num_str, multiplier) = if s_upper.ends_with("GB") {
        (&s[..s.len() - 2], 1024 * 1024 * 1024)
    } else if s_upper.ends_with("MB") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s_upper.ends_with("KB") {
        (&s[..s.len() - 2], 1024)
    } else if s_upper.ends_with('B') {
        (&s[..s.len() - 1], 1)
    } else {
        // Assume bytes if no unit
        (s, 1)
    };

    let num: usize = num_str.trim().parse().map_err(|_| ParseError::InvalidSize {
        value: s.to_string(),
        reason: "Invalid number".to_string(),
    })?;

    num.checked_mul(multiplier).ok_or_else(|| ParseError::InvalidSize {
        value: s.to_string(),
        reason: "Value too large".to_string(),
    })
}

/// Parse duration strings like "30s", "5m", "1h"
///
/// # Errors
///
/// Returns `ParseError::InvalidDuration` if the string is missing a unit suffix or the number is
/// invalid.
pub fn parse_duration(s: &str) -> Result<Duration, ParseError> {
    let s = s.trim().to_lowercase();

    let (num_str, multiplier_ms) = if s.ends_with("ms") {
        (&s[..s.len() - 2], 1u64)
    } else if s.ends_with('s') {
        (&s[..s.len() - 1], 1000)
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], 60 * 1000)
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], 60 * 60 * 1000)
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], 24 * 60 * 60 * 1000)
    } else {
        return Err(ParseError::InvalidDuration {
            value: s,
            reason: "Missing unit (ms, s, m, h, d)".to_string(),
        });
    };

    let num: u64 = num_str.trim().parse().map_err(|_| ParseError::InvalidDuration {
        value: s.clone(),
        reason: "Invalid number".to_string(),
    })?;

    Ok(Duration::from_millis(num * multiplier_ms))
}

/// Errors produced when a required environment variable is absent.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnvError {
    /// A required environment variable was not set.
    #[error("Missing environment variable: {name}")]
    MissingVar {
        /// Name of the missing variable.
        name: String,
    },

    /// A required environment variable was not set; carries an extra explanation.
    #[error("Missing environment variable {name}: {message}")]
    MissingVarWithMessage {
        /// Name of the missing variable.
        name: String,
        /// Human-readable explanation of why the variable is required.
        message: String,
    },
}

/// Errors produced when a configuration string cannot be parsed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// A size string (e.g. `"10MB"`) could not be interpreted.
    #[error("Invalid size value '{value}': {reason}")]
    InvalidSize {
        /// The raw string that failed parsing.
        value: String,
        /// Explanation of why parsing failed.
        reason: String,
    },

    /// A duration string (e.g. `"30s"`) could not be interpreted.
    #[error("Invalid duration value '{value}': {reason}")]
    InvalidDuration {
        /// The raw string that failed parsing.
        value: String,
        /// Explanation of why parsing failed.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    // ─── resolve_env_value ──────────────────────────────────────────────────

    #[test]
    fn literal_value_returned_unchanged() {
        assert_eq!(resolve_env_value("hello").unwrap(), "hello");
    }

    #[test]
    fn env_var_reference_resolves() {
        temp_env::with_var("FRAISEQL_TEST_ENV_RS", Some("resolved"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_TEST_ENV_RS}").unwrap(), "resolved");
        });
    }

    #[test]
    fn missing_env_var_returns_error() {
        temp_env::with_var("FRAISEQL_MISSING_VAR", None::<&str>, || {
            let err = resolve_env_value("${FRAISEQL_MISSING_VAR}").unwrap_err();
            assert!(
                matches!(err, EnvError::MissingVar { ref name } if name == "FRAISEQL_MISSING_VAR"),
                "expected MissingVar, got: {err:?}"
            );
        });
    }

    #[test]
    fn default_syntax_uses_fallback_when_absent() {
        temp_env::with_var("FRAISEQL_ABSENT", None::<&str>, || {
            assert_eq!(resolve_env_value("${FRAISEQL_ABSENT:-fallback}").unwrap(), "fallback");
        });
    }

    #[test]
    fn default_syntax_uses_real_value_when_present() {
        temp_env::with_var("FRAISEQL_PRESENT", Some("real"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_PRESENT:-fallback}").unwrap(), "real");
        });
    }

    #[test]
    fn required_with_message_syntax_errors_with_message() {
        temp_env::with_var("FRAISEQL_REQUIRED", None::<&str>, || {
            let err = resolve_env_value("${FRAISEQL_REQUIRED:?must be set}").unwrap_err();
            assert!(
                matches!(
                    err,
                    EnvError::MissingVarWithMessage { ref name, ref message }
                    if name == "FRAISEQL_REQUIRED" && message == "must be set"
                ),
                "expected MissingVarWithMessage, got: {err:?}"
            );
        });
    }

    #[test]
    fn required_with_message_syntax_resolves_when_present() {
        temp_env::with_var("FRAISEQL_REQUIRED_OK", Some("value"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_REQUIRED_OK:?must be set}").unwrap(), "value");
        });
    }

    // ─── get_env_value ──────────────────────────────────────────────────────

    #[test]
    fn get_env_value_returns_value_when_set() {
        temp_env::with_var("FRAISEQL_GET_TEST", Some("got_it"), || {
            assert_eq!(get_env_value("FRAISEQL_GET_TEST").unwrap(), "got_it");
        });
    }

    #[test]
    fn get_env_value_returns_error_when_missing() {
        temp_env::with_var("FRAISEQL_GET_MISSING", None::<&str>, || {
            assert!(get_env_value("FRAISEQL_GET_MISSING").is_err());
        });
    }

    // ─── parse_size edge cases ──────────────────────────────────────────────

    #[test]
    fn parse_size_overflow_returns_error() {
        // usize::MAX GB would overflow
        let result = parse_size(&format!("{}GB", usize::MAX));
        assert!(result.is_err(), "overflow must return Err");
    }

    #[test]
    fn parse_size_whitespace_trimmed() {
        assert_eq!(parse_size("  10MB  ").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size("10mb").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("10Mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_zero_is_valid() {
        assert_eq!(parse_size("0MB").unwrap(), 0);
    }

    // ─── parse_duration edge cases ──────────────────────────────────────────

    #[test]
    fn parse_duration_zero_is_valid() {
        assert_eq!(parse_duration("0s").unwrap(), Duration::from_secs(0));
    }

    #[test]
    fn parse_duration_whitespace_trimmed() {
        assert_eq!(parse_duration("  30s  ").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_missing_unit_returns_error() {
        let err = parse_duration("42").unwrap_err();
        assert!(matches!(err, ParseError::InvalidDuration { .. }));
    }

    #[test]
    fn parse_duration_non_numeric_returns_error() {
        let err = parse_duration("xyzs").unwrap_err();
        assert!(matches!(err, ParseError::InvalidDuration { .. }));
    }
}
