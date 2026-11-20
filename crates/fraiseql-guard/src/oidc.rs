//! The check that an OIDC discovery document belongs to the issuer it was fetched for.
//!
//! `OpenID` Connect Discovery 1.0 §4.3: the `issuer` a discovery document names must be
//! the issuer URL the document was requested for. A document naming another issuer
//! comes from a wrong tenant, or from a proxy or redirect serving another issuer's
//! metadata; the endpoints it lists are that other issuer's.
//!
//! Here rather than in one consumer because the workspace has more than one discovery
//! fetch, in crates that do not depend on each other, and the rule must be one answer.
//! Errors are plain `String`s: each caller wraps the message in its own error type.

/// How strictly a discovered `issuer` is compared with the configured one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IssuerMatch {
    /// Byte for byte. For a configured value that is also compared with a token's
    /// `iss` claim: a document that differs from it in any way, a trailing slash
    /// included, names an issuer whose tokens the comparison refuses.
    Exact,
    /// Equal once one trailing `/` is removed from each side. For a configured value
    /// that is a base URL the discovery path is appended to, and that no token is
    /// compared with: `https://tenant.example` and `https://tenant.example/` name the
    /// same document.
    IgnoringTrailingSlash,
}

/// Check that the `discovered` issuer of the document at `discovery_url` is the
/// `configured` one, under `rule`.
///
/// # Errors
///
/// Returns a message naming the discovery URL and both issuers when they differ.
pub fn check_discovered_issuer(
    discovery_url: &str,
    configured: &str,
    discovered: &str,
    rule: IssuerMatch,
) -> Result<(), String> {
    let same = match rule {
        IssuerMatch::Exact => configured == discovered,
        IssuerMatch::IgnoringTrailingSlash => {
            without_one_slash(configured) == without_one_slash(discovered)
        },
    };
    if same {
        return Ok(());
    }
    Err(format!(
        "the OIDC discovery document at {discovery_url} names the issuer {discovered:?}, but the \
         configured issuer is {configured:?}: the document belongs to another issuer. Configure \
         the issuer the document names if it is the intended one, or point the configuration at \
         the intended issuer"
    ))
}

fn without_one_slash(url: &str) -> &str {
    url.strip_suffix('/').unwrap_or(url)
}

#[cfg(test)]
mod tests;
