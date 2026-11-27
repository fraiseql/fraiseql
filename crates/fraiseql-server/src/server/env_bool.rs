//! Boolean environment variables in clap's grammar.
//!
//! The server's flags read their environment variables through clap's
//! `BoolishValueParser`. A variable the server reads outside clap takes the same
//! grammar, so one spelling means the same thing everywhere; clap is behind the `cli`
//! feature, so the grammar is restated here and a test pins it to clap's.

/// `Some(true)` for `y`, `yes`, `t`, `true`, `on` or `1`, `Some(false)` for `n`, `no`,
/// `f`, `false`, `off` or `0` (ASCII case-insensitive, surrounding whitespace ignored),
/// `None` for anything else.
pub(super) fn boolish(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "t" | "true" | "on" | "1" => Some(true),
        "n" | "no" | "f" | "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// The boolean `var` holds, or `default` when it is unset.
///
/// # Errors
///
/// [`crate::ServerError::ConfigError`] naming `var` when its value is neither true nor
/// false.
pub(super) fn env_bool(var: &str, raw: Option<&str>, default: bool) -> crate::Result<bool> {
    raw.map_or(Ok(default), |raw| {
        boolish(raw).ok_or_else(|| {
            crate::ServerError::ConfigError(format!(
                "{var} = {raw:?} is not a boolean. Use true or false (also 1/0, yes/no, on/off)."
            ))
        })
    })
}
