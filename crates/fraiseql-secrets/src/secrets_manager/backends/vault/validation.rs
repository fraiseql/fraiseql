//! Vault address and secret-name validation (SSRF and input-size guards).

use crate::secrets_manager::SecretsError;

/// Env var that disables the Vault address SSRF guard for local development.
///
/// Honoured only when `FRAISEQL_ENV` declares a development environment; refused
/// in production regardless of its value.
pub const VAULT_ALLOW_INSECURE_ENV: &str = "FRAISEQL_VAULT_ALLOW_INSECURE";

/// Env var holding a comma-separated allowlist of exact Vault hosts that may
/// bypass the private-range SSRF block (#727).
///
/// Production Vault almost always lives on an RFC 1918 address, and before this
/// allowlist the only way to reach it was `FRAISEQL_VAULT_ALLOW_INSECURE=true` —
/// which is refused in production and, where honoured, disables *every* guard
/// including the scheme check. The allowlist is the granular alternative: it is
/// honoured in every environment, it admits only the exact hosts named (matched
/// case-insensitively against the URL host, an IP literal or a hostname), and the
/// scheme and URL-validity checks still apply.
pub const VAULT_ALLOWED_HOSTS_ENV: &str = "FRAISEQL_VAULT_ALLOWED_HOSTS";

/// Maximum byte length for a Vault secret name / path.
///
/// Vault's own internal key-value paths top out at a few hundred characters in
/// practice; 1 024 bytes is generous while preventing cache-key `DoS` from a
/// caller that passes a megabyte-sized string.
pub const MAX_VAULT_SECRET_NAME_BYTES: usize = 1_024;

/// Validate the Vault server address against SSRF-prone destinations.
///
/// Rejects:
/// - Non-HTTP(S) schemes
/// - Loopback addresses (`127.0.0.0/8`, `::1`, `localhost`)
/// - RFC 1918 private ranges (`10/8`, `172.16/12`, `192.168/16`)
/// - Link-local addresses (`169.254/16`) — includes AWS IMDSv1/v2
/// - CGNAT range (`100.64/10`)
/// - `IPv6` ULA (`fc00::/7`)
///
/// # Errors
///
/// Returns [`SecretsError::ValidationError`] if the address uses a non-HTTP(S) scheme,
/// is not a valid URL, or targets a private/loopback address (SSRF protection).
pub(super) fn validate_vault_addr(addr: &str) -> Result<(), SecretsError> {
    // `FRAISEQL_VAULT_ALLOW_INSECURE=true` disables the SSRF guard for local
    // development and integration testing. It is honoured only in a declared
    // development environment: this used to return `Ok(())` on the env var alone,
    // with no production check of any kind, so a stray `.env` line disabled the
    // guard on a Vault address in production. `insecure_bypass` also reports the
    // decision, so a refusal reaches the operator as a log line rather than as an
    // unexplained connection failure (#882).
    if fraiseql_guard::deployment::insecure_bypass(VAULT_ALLOW_INSECURE_ENV).is_honoured() {
        return Ok(());
    }

    let lower = addr.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(SecretsError::ValidationError(format!(
            "Vault address must use http:// or https:// scheme: {addr}"
        )));
    }

    // Use a real URL parser to extract the host — manual `split('/')` / `split(':')`
    // is fragile for IPv6 literals like `[::1]` and produces an empty first segment.
    let parsed = reqwest::Url::parse(addr).map_err(|e| {
        SecretsError::ValidationError(format!("Vault address is not a valid URL ({addr}): {e}"))
    })?;

    let host_raw = parsed.host_str().unwrap_or("");
    // The `url` crate wraps IPv6 literals in brackets in `host_str()`.
    // Strip them so the IP address can be parsed by `IpAddr::from_str`.
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    // #727: an exact-host allowlist bypasses only the private-range block — the
    // scheme and URL checks above have already run. This is what lets a production
    // deployment reach a Vault on RFC 1918 space without flipping the all-guards-off
    // insecure switch.
    if host_is_allowlisted(host, std::env::var(VAULT_ALLOWED_HOSTS_ENV).ok().as_deref()) {
        return Ok(());
    }

    if is_ssrf_blocked_host_vault(host) {
        return Err(SecretsError::ValidationError(format!(
            "Vault address targets a private/loopback address (SSRF protection): {addr}. \
             If this is genuinely your Vault, allow this exact host with \
             {VAULT_ALLOWED_HOSTS_ENV}={host}"
        )));
    }
    Ok(())
}

/// Returns `true` when `host` appears in the comma-separated allowlist.
///
/// Pure so both environments of the guard are unit-testable without touching
/// process env. Matching is exact and case-insensitive per entry; empty entries
/// (stray commas, whitespace) are ignored. No wildcard or CIDR forms — an
/// allowlist that can widen silently is the footgun this replaces.
pub(super) fn host_is_allowlisted(host: &str, allowlist: Option<&str>) -> bool {
    let Some(list) = allowlist else {
        return false;
    };
    if host.is_empty() {
        return false;
    }
    list.split(',')
        .map(str::trim)
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(host))
}

/// Returns `true` for hostnames/IPs a Vault address must never point at.
///
/// Delegates to [`fraiseql_guard::net`]. The copy this replaced accepted
/// `0.0.0.0/8`, every `IPv4`-mapped address, and the whole `fe80::/10`
/// link-local range.
fn is_ssrf_blocked_host_vault(host: &str) -> bool {
    fraiseql_guard::net::blocked_host_reason(host).is_some()
}

/// Validate Vault secret name format.
///
/// # Errors
///
/// Returns [`SecretsError::ValidationError`] if the name is empty, exceeds
/// [`MAX_VAULT_SECRET_NAME_BYTES`], or contains characters other than alphanumeric, `/`, `-`, `_`.
pub fn validate_vault_secret_name(name: &str) -> Result<(), SecretsError> {
    if name.is_empty() {
        return Err(SecretsError::ValidationError("Vault secret name cannot be empty".to_string()));
    }

    if name.len() > MAX_VAULT_SECRET_NAME_BYTES {
        return Err(SecretsError::ValidationError(format!(
            "Vault secret name is too long ({} bytes, max {MAX_VAULT_SECRET_NAME_BYTES})",
            name.len()
        )));
    }

    // Vault paths typically contain slashes and lowercase alphanumeric
    if !name.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_') {
        return Err(SecretsError::ValidationError(format!(
            "Invalid Vault secret name: {}. Only alphanumeric, /, -, _ allowed",
            name
        )));
    }

    Ok(())
}
