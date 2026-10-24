//! The hosts guest code may call.
//!
//! An application's functions and source connectors run guest code that can make
//! outbound calls. Every such call needs the deployment's permission by host name, in
//! addition to the address checks the rest of [`crate::net`] makes. One matcher reads
//! that permission, so a host one guest is granted is granted to every guest, and one
//! refused is refused everywhere.

/// Whether `host` is one of `patterns`.
///
/// A pattern is `*` (any host), `*.example.com` (any subdomain of `example.com`, but not
/// `example.com` itself), or a host name or IP literal matched exactly. Host names
/// compare without regard to ASCII case. An empty list permits nothing.
#[must_use]
pub fn allows(host: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if pattern == "*" {
            return true;
        }
        match pattern.strip_prefix("*.") {
            Some(domain) => host
                .len()
                .checked_sub(domain.len() + 1)
                .filter(|&dot| host.as_bytes()[dot] == b'.')
                .is_some_and(|dot| host[dot + 1..].eq_ignore_ascii_case(domain)),
            None => host.eq_ignore_ascii_case(pattern),
        }
    })
}

#[cfg(test)]
mod tests;
