//! The server HMAC secret, read from the variable `hmac_secret_env` names.
//!
//! The secret signs the idempotency token functions see and keys the recipient address
//! hash the email suppression check uses. A named variable that is unset or empty
//! refuses boot, as the other `*_env` keys do: running on refused the signature and the
//! suppression check while the startup log read healthy.

use crate::ServerError;

/// The secret the variable `env_name` holds, or `None` when no variable is named.
///
/// # Errors
///
/// [`ServerError::ConfigError`], naming the variable but never its value, when it is
/// unset or empty.
pub(super) fn resolve(
    env_name: Option<&str>,
    get_env: impl Fn(&str) -> Option<String>,
) -> crate::Result<Option<String>> {
    let Some(env_name) = env_name else {
        return Ok(None);
    };
    match get_env(env_name) {
        Some(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
        _ => Err(ServerError::ConfigError(format!(
            "hmac_secret_env names {env_name}, which is unset or empty. Set it to the server \
             HMAC secret, stable across restarts and shared by every instance, or remove \
             hmac_secret_env to run with unsigned idempotency tokens and no address \
             suppression."
        ))),
    }
}
