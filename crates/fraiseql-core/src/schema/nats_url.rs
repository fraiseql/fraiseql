//! The NATS URL `[observers]` names.
//!
//! `[observers]` names the observer transport's NATS URL either as written (`nats_url`) or
//! by the environment variable that holds it (`nats_url_env`). The compiled schema carries
//! what was written, so a written URL is held to carrying no credential ([`check`]), and
//! the server reads the variable at startup ([`resolve`]). A NATS URL may list several
//! servers separated by commas; each is held to the same rule, and a log line names them
//! by [`host_of`], never in full.

use super::redis_url;

/// Each server a NATS URL lists.
fn servers(url: &str) -> impl Iterator<Item = &str> {
    url.split(',').map(str::trim)
}

/// Whether any server `url` lists holds a credential: user info before its host
/// (`nats://user:pass@host`, `tls://token@host`), or a password query parameter.
#[must_use]
pub fn holds_credential(url: &str) -> bool {
    servers(url).any(redis_url::holds_credential)
}

/// Each server `url` lists as `scheme://host:port`, comma-separated: what a log line may
/// show of it.
#[must_use]
pub fn host_of(url: &str) -> String {
    servers(url).map(redis_url::host_of).collect::<Vec<_>>().join(",")
}

/// Hold the NATS URL of `[observers]` to what a compiled schema may carry.
///
/// # Errors
///
/// Returns the refusal, naming the key and never the URL, when `[observers]` writes both
/// `nats_url` and `nats_url_env`, when `nats_url_env` cannot name an environment variable,
/// or when `nats_url` holds a credential.
pub fn check(url: Option<&str>, url_env: Option<&str>) -> Result<(), String> {
    if url.is_some() && url_env.is_some() {
        return Err("[observers] sets both nats_url and nats_url_env; keep one: nats_url_env \
                    names the environment variable the server reads the URL from"
            .to_owned());
    }
    if url_env.is_some_and(|name| !redis_url::is_env_name(name)) {
        return Err("[observers] nats_url_env must name an environment variable (a letter or \
                    `_`, then letters, digits or `_`), not hold the URL"
            .to_owned());
    }
    if url.is_some_and(holds_credential) {
        return Err("[observers] nats_url holds a credential, and the compiled schema carries \
                    this key as written. Name the variable holding the URL with nats_url_env \
                    instead: the server reads it at startup, and only its name is compiled"
            .to_owned());
    }
    Ok(())
}

/// The NATS URL `[observers]` names, reading `nats_url_env` through `get_env`.
///
/// # Errors
///
/// Returns the refusal, naming the key and the variable but never a value, when
/// `[observers]` names its URL both ways, or when the variable `nats_url_env` names is
/// unset or empty.
pub fn resolve(
    url: Option<&str>,
    url_env: Option<&str>,
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, String> {
    match (url, url_env) {
        (Some(_), Some(_)) => {
            Err("[observers] sets both nats_url and nats_url_env; keep one".to_owned())
        },
        (Some(url), None) => Ok(Some(url.to_owned())),
        (None, Some(name)) => match get_env(name) {
            Some(value) if !value.trim().is_empty() => Ok(Some(value)),
            Some(_) => Err(format!("[observers] nats_url_env names `{name}`, which is empty")),
            None => Err(format!("[observers] nats_url_env names `{name}`, which is not set")),
        },
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
#[path = "nats_url_tests.rs"]
mod tests;
