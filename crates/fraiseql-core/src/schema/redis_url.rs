//! The Redis URL a security section names.
//!
//! `[security.rate_limiting]`, `[security.pkce]` and `[security.token_revocation]` each name
//! a Redis URL either as written (`redis_url`) or by the environment variable that holds it
//! (`redis_url_env`). The compiled schema carries what was written, so a written URL is held
//! to carrying no credential ([`check`]), and the server reads the variable at startup
//! ([`resolve`]). Logs name the URL by [`host_of`], never in full.

use super::SecurityConfig;

/// Each Redis-backed section `security` declares: its name, `redis_url` and `redis_url_env`.
#[must_use]
pub fn declared(security: &SecurityConfig) -> Vec<(&'static str, Option<&str>, Option<&str>)> {
    let mut declared = Vec::new();
    if let Some(rl) = &security.rate_limiting {
        declared.push(("rate_limiting", rl.redis_url.as_deref(), rl.redis_url_env.as_deref()));
    }
    if let Some(pkce) = &security.pkce {
        declared.push(("pkce", pkce.redis_url.as_deref(), pkce.redis_url_env.as_deref()));
    }
    if let Some(tr) = &security.token_revocation {
        declared.push(("token_revocation", tr.redis_url.as_deref(), tr.redis_url_env.as_deref()));
    }
    declared
}

/// The query parameters through which a Redis URL can carry a password.
const PASSWORD_PARAMS: [&str; 2] = ["password", "pass"];

/// Whether `url` holds a credential: user info before its host (`redis://user:pass@host`,
/// `redis://:pass@host`, `redis://user@host`), or a password query parameter.
#[must_use]
pub fn holds_credential(url: &str) -> bool {
    let (authority, query) = split(url);
    authority.contains('@')
        || query.is_some_and(|query| {
            query.split('&').any(|pair| {
                let key = pair.split_once('=').map_or(pair, |(key, _)| key);
                PASSWORD_PARAMS.iter().any(|param| key.eq_ignore_ascii_case(param))
            })
        })
}

/// `url` with its user info, path and query removed: `scheme://host:port`, or the socket
/// path of a `redis+unix://` URL. What a log line may show of it.
#[must_use]
pub fn host_of(url: &str) -> String {
    let (authority, _) = split(url);
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    match url.split_once("://") {
        Some((scheme, rest)) if scheme.ends_with("+unix") || scheme == "unix" => {
            let path = rest.split(['?', '#']).next().unwrap_or_default();
            let path = path.rsplit_once('@').map_or(path, |(_, path)| path);
            format!("{scheme}://{path}")
        },
        Some((scheme, _)) => format!("{scheme}://{host}"),
        None => "(not a URL)".to_owned(),
    }
}

/// Whether `name` can name an environment variable: a letter or `_`, then letters,
/// digits or `_`.
#[must_use]
pub fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Hold the Redis URL of `[security.<section>]` to what a compiled schema may carry.
///
/// # Errors
///
/// Returns the refusal, naming the key and never the URL, when the section writes both
/// `redis_url` and `redis_url_env`, when `redis_url_env` cannot name an environment
/// variable, or when `redis_url` holds a credential.
pub fn check(section: &str, url: Option<&str>, url_env: Option<&str>) -> Result<(), String> {
    if url.is_some() && url_env.is_some() {
        return Err(format!(
            "[security.{section}] sets both redis_url and redis_url_env; keep one: \
             redis_url_env names the environment variable the server reads the URL from"
        ));
    }
    if let Some(name) = url_env {
        if !is_env_name(name) {
            return Err(format!(
                "[security.{section}] redis_url_env must name an environment variable (a \
                 letter or `_`, then letters, digits or `_`), not hold the URL"
            ));
        }
    }
    if url.is_some_and(holds_credential) {
        return Err(format!(
            "[security.{section}] redis_url holds a credential, and the compiled schema \
             carries this key as written. Name the variable holding the URL with \
             redis_url_env instead: the server reads it at startup, and only its name is \
             compiled"
        ));
    }
    Ok(())
}

/// The Redis URL `[security.<section>]` names, reading `redis_url_env` through `get_env`.
///
/// # Errors
///
/// Returns the refusal, naming the key and the variable but never a value, when the
/// section names its URL both ways, or when the variable `redis_url_env` names is unset
/// or empty.
pub fn resolve(
    section: &str,
    url: Option<&str>,
    url_env: Option<&str>,
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, String> {
    match (url, url_env) {
        (Some(_), Some(_)) => {
            Err(format!("[security.{section}] sets both redis_url and redis_url_env; keep one"))
        },
        (Some(url), None) => Ok(Some(url.to_owned())),
        (None, Some(name)) => match get_env(name) {
            Some(value) if !value.trim().is_empty() => Ok(Some(value)),
            Some(_) => {
                Err(format!("[security.{section}] redis_url_env names `{name}`, which is empty"))
            },
            None => {
                Err(format!("[security.{section}] redis_url_env names `{name}`, which is not set"))
            },
        },
        (None, None) => Ok(None),
    }
}

/// `url`'s authority (between `://` and the first `/`, `?` or `#`) and its query.
fn split(url: &str) -> (&str, Option<&str>) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let before_fragment = rest.split('#').next().unwrap_or_default();
    let (before_query, query) = match before_fragment.split_once('?') {
        Some((before, query)) => (before, Some(query)),
        None => (before_fragment, None),
    };
    let authority = before_query.split('/').next().unwrap_or_default();
    (authority, query)
}

#[cfg(test)]
#[path = "redis_url_tests.rs"]
mod tests;
