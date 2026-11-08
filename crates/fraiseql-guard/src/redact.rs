//! Credential-free renderings of connection URLs, for logs, errors and `Debug` output.
//!
//! A database, broker or webhook URL can carry a password in its userinfo
//! (`postgres://app:secret@db/app`) or a token in its query (`…/hook?token=secret`).
//! Anything that writes such a URL where people or log pipelines can read it writes
//! [`url`]'s rendering instead: the user, host, port and path stay readable, the password
//! and every query value and fragment become [`MASK`].
//!
//! Input that does not have the `scheme://authority` shape is masked whole: a string this
//! module cannot take apart is one it cannot vouch for, so none of it is echoed.

use std::fmt;

/// What a credential is replaced with.
pub const MASK: &str = "***";

/// `raw` with its password, query values and fragment replaced by [`MASK`].
///
/// ```
/// use fraiseql_guard::redact;
///
/// assert_eq!(redact::url("postgres://app:secret@db:5432/app"), "postgres://app:***@db:5432/app");
/// assert_eq!(redact::url("https://hooks.example.com/x?token=abc&v=2"), "https://hooks.example.com/x?token=***&v=***");
/// assert_eq!(redact::url("not a url"), "***");
/// ```
#[must_use]
pub fn url(raw: &str) -> String {
    Url(raw).to_string()
}

/// A [`Display`](fmt::Display) of [`url`]'s rendering that does not allocate up front, for
/// `tracing` fields: `info!(url = %redact::Url(&config.url), …)`.
#[derive(Clone, Copy)]
pub struct Url<'a>(pub &'a str);

impl fmt::Display for Url<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some((scheme, rest)) = split_scheme(self.0) else {
            return f.write_str(MASK);
        };
        f.write_str(scheme)?;
        f.write_str("://")?;
        // The host follows the last `@`, searched across the whole rest rather than up to the
        // first `/`, `?` or `#`: a password may carry any of those unencoded, and stopping at
        // one would print the password's tail as the host. An `@` later in a path or query
        // makes the split land too far right, which only masks more.
        let after_userinfo = match rest.rsplit_once('@') {
            Some((userinfo, after)) => {
                match userinfo.split_once(':') {
                    Some((user, _password)) if is_plain_user(user) => write!(f, "{user}:{MASK}@")?,
                    None if is_plain_user(userinfo) => write!(f, "{userinfo}@")?,
                    // The "user" runs into a path, query or fragment: there is no telling
                    // where a secret starts, so none of it is printed.
                    _ => write!(f, "{MASK}@")?,
                }
                after
            },
            None => rest,
        };
        let authority_end = after_userinfo.find(['/', '?', '#']).unwrap_or(after_userinfo.len());
        let (host, tail) = after_userinfo.split_at(authority_end);
        f.write_str(host)?;
        let (before_fragment, fragment) = match tail.split_once('#') {
            Some((before, fragment)) => (before, Some(fragment)),
            None => (tail, None),
        };
        match before_fragment.split_once('?') {
            Some((path, query)) => {
                write!(f, "{path}?")?;
                for (i, pair) in query.split('&').enumerate() {
                    if i > 0 {
                        f.write_str("&")?;
                    }
                    match pair.split_once('=') {
                        Some((key, _value)) => write!(f, "{key}={MASK}")?,
                        None if pair.is_empty() => {},
                        // A bare `?token` is a value with no key: mask it whole.
                        None => f.write_str(MASK)?,
                    }
                }
            },
            None => f.write_str(before_fragment)?,
        }
        if let Some(fragment) = fragment {
            write!(f, "#{}", if fragment.is_empty() { "" } else { MASK })?;
        }
        Ok(())
    }
}

impl fmt::Debug for Url<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{self}\"")
    }
}

/// `Some((scheme, after "://"))` when `raw` has a valid scheme and an authority, and no
/// whitespace or control characters anywhere.
fn split_scheme(raw: &str) -> Option<(&str, &str)> {
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, rest) = raw.split_once("://")?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some((scheme, rest))
}

/// Whether `user` can only be a user name, not a user name run into a path, query or fragment.
fn is_plain_user(user: &str) -> bool {
    !user.contains(['/', '?', '#'])
}

/// [`url`] over an optional value, for `Debug` impls of optional URL fields.
#[must_use]
pub fn url_opt(raw: Option<&str>) -> Option<String> {
    raw.map(url)
}

#[cfg(test)]
mod tests;
