//! Vault address and secret-name validation (SSRF and input-size guards).

use crate::secrets_manager::SecretsError;

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
    // When `FRAISEQL_VAULT_ALLOW_INSECURE=true` all SSRF guards are disabled.
    // This is intended for local development and integration testing only —
    // never set in production.
    let allow_insecure = std::env::var("FRAISEQL_VAULT_ALLOW_INSECURE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if allow_insecure {
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

    if is_ssrf_blocked_host_vault(host) {
        return Err(SecretsError::ValidationError(format!(
            "Vault address targets a private/loopback address (SSRF protection): {addr}"
        )));
    }
    Ok(())
}

fn is_ssrf_blocked_host_vault(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback()
            || addr.is_private()
            || addr.is_link_local()
            || is_cgnat_v4_vault(addr);
    }
    if let Ok(addr) = host.parse::<std::net::Ipv6Addr>() {
        return addr.is_loopback() || addr.is_unspecified() || is_ula_v6_vault(addr);
    }
    false
}

const fn is_cgnat_v4_vault(addr: std::net::Ipv4Addr) -> bool {
    // 100.64.0.0/10
    let octets = addr.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

const fn is_ula_v6_vault(addr: std::net::Ipv6Addr) -> bool {
    // fc00::/7
    (addr.segments()[0] & 0xFE00) == 0xFC00
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
