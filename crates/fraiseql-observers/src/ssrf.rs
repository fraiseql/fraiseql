//! SSRF URL validation helpers for outbound HTTP connections.
//!
//! Consistent with the guards in `fraiseql-federation` and `fraiseql-core`.

use crate::error::ObserverError;

/// Validate that a URL is safe for outbound HTTP contact.
///
/// Rejects URLs targeting private/loopback/link-local IP addresses to prevent
/// server-side request forgery via misconfigured or attacker-controlled URLs.
///
/// # Errors
///
/// Returns `ObserverError::InvalidConfig` if the URL is unparseable or targets
/// a forbidden host.
pub fn validate_outbound_url(url: &str) -> crate::error::Result<()> {
    // When `FRAISEQL_OBSERVERS_ALLOW_INSECURE=true` all SSRF guards are disabled.
    // This is intended for local development and integration testing only —
    // never set in production.
    let allow_insecure = std::env::var("FRAISEQL_OBSERVERS_ALLOW_INSECURE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if allow_insecure {
        return Ok(());
    }

    let parsed = reqwest::Url::parse(url).map_err(|e| ObserverError::InvalidConfig {
        message: format!("Invalid URL '{url}': {e}"),
    })?;

    let host_raw = parsed.host_str().ok_or_else(|| ObserverError::InvalidConfig {
        message: format!("URL has no host: {url}"),
    })?;

    // Strip IPv6 brackets added by the url crate (e.g. "[::1]" → "::1").
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(ObserverError::InvalidConfig {
            message: format!("URL targets a loopback host ({host}) — SSRF protection blocked"),
        });
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_ssrf_blocked_ip(&ip) {
            return Err(ObserverError::InvalidConfig {
                message: format!(
                    "URL targets a private/reserved IP ({ip}) — SSRF protection blocked"
                ),
            });
        }
    }

    Ok(())
}

/// Resolve the host via DNS and reject if any address is private/reserved.
///
/// Prevents DNS rebinding attacks where an attacker-controlled domain initially
/// resolves to a public IP (passing URL validation) but later resolves to a
/// private IP during the actual HTTP request.
///
/// # Errors
///
/// Returns `ObserverError::InvalidConfig` if DNS resolution fails, returns no
/// addresses, or any resolved address is in a private/reserved range.
pub async fn dns_resolve_and_check(url: &str) -> crate::error::Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|e| ObserverError::InvalidConfig {
        message: format!("Invalid URL '{url}': {e}"),
    })?;
    let host = parsed.host_str().ok_or_else(|| ObserverError::InvalidConfig {
        message: format!("URL has no host: {url}"),
    })?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| ObserverError::InvalidConfig {
            message: format!("DNS resolution failed for host '{host}': {e}"),
        })?
        .collect();
    if addrs.is_empty() {
        return Err(ObserverError::InvalidConfig {
            message: format!("DNS resolved to no addresses for host '{host}'"),
        });
    }
    for addr in &addrs {
        if is_ssrf_blocked_ip(&addr.ip()) {
            return Err(ObserverError::InvalidConfig {
                message: format!(
                    "DNS rebinding attack blocked: host '{host}' resolved to private/reserved IP {}",
                    addr.ip()
                ),
            });
        }
    }
    Ok(())
}

/// Validate that a NATS URL is safe to connect to.
///
/// Accepts `nats://` and `tls://` schemes only; rejects private/loopback hosts.
///
/// # Errors
///
/// Returns `ObserverError::InvalidConfig` if the URL is invalid or targets a forbidden host.
#[cfg(feature = "nats")]
pub fn validate_nats_url(url: &str) -> crate::error::Result<()> {
    if !url.starts_with("nats://") && !url.starts_with("tls://") {
        return Err(ObserverError::InvalidConfig {
            message: format!("NATS URL must use nats:// or tls:// scheme (got: {url})"),
        });
    }
    validate_outbound_url(url)
}

/// Returns `true` for IP ranges that outbound connections must never target.
fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                          // loopback 127/8
            || o[0] == 10                                        // RFC 1918 10/8
            || (o[0] == 172 && (16..=31).contains(&o[1]))       // RFC 1918 172.16/12
            || (o[0] == 192 && o[1] == 168)                     // RFC 1918 192.168/16
            || (o[0] == 169 && o[1] == 254)                     // link-local 169.254/16
            || (o[0] == 100 && (64..=127).contains(&o[1]))      // CGNAT 100.64/10
            || o == [0, 0, 0, 0] // unspecified
        },
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()                                     // ::1
            || v6.is_unspecified()                               // ::
            || {
                let s = v6.segments();
                (s[0] & 0xfe00) == 0xfc00                        // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80                    // link-local fe80::/10
                || (s[0] == 0 && s[1] == 0 && s[2] == 0        // ::ffff:0:0/96
                    && s[3] == 0 && s[4] == 0 && s[5] == 0xffff)
            }
        },
    }
}
