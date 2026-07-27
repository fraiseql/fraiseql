//! SSRF URL validation helpers for outbound HTTP connections.
//!
//! The address ranges and hostname aliases come from [`fraiseql_guard::net`], the
//! workspace's single outbound guard. This module owns only URL parsing, the
//! observer error mapping, and the `FRAISEQL_OBSERVERS_ALLOW_INSECURE` policy.

use fraiseql_guard::net::{blocked_host_reason, is_blocked_ip as is_ssrf_blocked_ip};

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
    // The `FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass is honored only in
    // development environments; refused when any production marker is set.
    // See `crate::insecure_guard` for the full policy.
    if crate::insecure_guard::is_outbound_insecure_allowed() {
        return Ok(());
    }

    let parsed = reqwest::Url::parse(url).map_err(|e| ObserverError::InvalidConfig {
        message: format!("Invalid URL '{url}': {e}"),
    })?;

    let host = parsed.host_str().ok_or_else(|| ObserverError::InvalidConfig {
        message: format!("URL has no host: {url}"),
    })?;

    // Bracket stripping, loopback/metadata hostname aliases and the literal-IP
    // range check all live in the shared guard.
    if let Some(reason) = blocked_host_reason(host) {
        return Err(ObserverError::InvalidConfig {
            message: format!("URL targets {host} — SSRF protection blocked ({reason})"),
        });
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
    // The `FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass is honored only in
    // development environments; refused when any production marker is set.
    // See `crate::insecure_guard` for the full policy.
    if crate::insecure_guard::is_outbound_insecure_allowed() {
        return Ok(());
    }

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
/// Accepts `tls://` always; accepts plaintext `nats://` only when the operator
/// has opted in via `FRAISEQL_NATS_ALLOW_PLAINTEXT` in a non-production
/// environment (L-nats-plaintext). Rejects every other scheme and any
/// private/loopback host.
///
/// # Errors
///
/// Returns `ObserverError::InvalidConfig` if the URL uses an unsupported scheme,
/// uses plaintext `nats://` without an explicit opt-in, or targets a forbidden host.
#[cfg(feature = "nats")]
pub fn validate_nats_url(url: &str) -> crate::error::Result<()> {
    let is_tls = url.starts_with("tls://");
    let is_plaintext = url.starts_with("nats://");
    if !is_tls && !is_plaintext {
        return Err(ObserverError::InvalidConfig {
            message: format!("NATS URL must use nats:// or tls:// scheme (got: {url})"),
        });
    }
    // L-nats-plaintext: plaintext nats:// has no transport encryption — change-log
    // events would cross the wire in the clear. Refuse it by default; the
    // `FRAISEQL_NATS_ALLOW_PLAINTEXT` escape hatch is honoured only outside
    // production (see `crate::insecure_guard`). tls:// is always allowed.
    if is_plaintext && !crate::insecure_guard::is_nats_plaintext_allowed() {
        return Err(ObserverError::InvalidConfig {
            message: format!(
                "NATS URL uses plaintext nats:// without TLS (got: {url}). Use tls:// for an \
                 encrypted connection, or set {}=true in a non-production environment to allow \
                 plaintext.",
                crate::insecure_guard::NATS_ALLOW_PLAINTEXT_ENV
            ),
        });
    }
    validate_outbound_url(url)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod ssrf_tests {
    use super::*;

    // Coverage retained when the drifted `dispatch.rs` copy was deleted and the
    // webhook dispatch path converged onto this canonical guard.

    #[test]
    fn rejects_localhost_dot_prefix_alias() {
        // `localhost.evil.com` and similar `localhost.` prefix aliases must be
        // rejected — this check existed only in the deleted dispatch.rs copy.
        let result = validate_outbound_url("https://localhost.evil.com/hook");
        assert!(
            result.is_err(),
            "localhost.evil.com must be rejected as a loopback alias: {result:?}"
        );
    }

    #[test]
    fn rejects_localhost_suffix_and_bare() {
        assert!(validate_outbound_url("https://localhost/hook").is_err());
        assert!(validate_outbound_url("https://api.localhost/hook").is_err());
    }

    #[test]
    fn rejects_zero_network_range() {
        // 0.0.0.0/8 "this network" — the dispatch.rs copy blocked the whole /8
        // via `o[0] == 0`; the canonical guard previously only blocked the exact
        // 0.0.0.0 address.
        assert!(validate_outbound_url("https://0.0.0.0/hook").is_err());
        assert!(
            validate_outbound_url("https://0.1.2.3/hook").is_err(),
            "0.0.0.0/8 range must be blocked, not just the unspecified address"
        );
    }

    #[test]
    fn allows_public_host_and_ip() {
        assert!(validate_outbound_url("https://api.example.com/hook").is_ok());
        assert!(validate_outbound_url("https://8.8.8.8/hook").is_ok());
    }

    #[test]
    fn rejects_private_and_loopback_ips() {
        assert!(validate_outbound_url("https://127.0.0.1/hook").is_err());
        assert!(validate_outbound_url("https://10.0.0.1/hook").is_err());
        assert!(validate_outbound_url("https://192.168.1.1/hook").is_err());
        assert!(validate_outbound_url("https://169.254.169.254/hook").is_err());
        assert!(validate_outbound_url("https://[::1]/hook").is_err());
    }
}

#[cfg(test)]
mod corpus {
    use fraiseql_guard::net::vectors::{MUST_ALLOW, MUST_BLOCK, MUST_BLOCK_HOSTS, url_host};

    use super::validate_outbound_url;

    /// Clear every bypass so the guard is actually exercised.
    fn engaged<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
        let mut out = None;
        temp_env::with_vars(
            [
                ("FRAISEQL_OBSERVERS_ALLOW_INSECURE", None::<&str>),
                ("FRAISEQL_ENV", None),
                ("FRAISEQL_PROFILE", None),
                ("KUBERNETES_SERVICE_HOST", None),
            ],
            || out = Some(f()),
        );
        out.expect("temp_env ran the closure")
    }

    #[test]
    fn refuses_every_blocked_address_in_the_corpus() {
        engaged(|| {
            for (addr, why) in MUST_BLOCK {
                let url = format!("https://{}/hook", url_host(addr));
                assert!(validate_outbound_url(&url).is_err(), "must refuse {addr} ({why})");
            }
        });
    }

    #[test]
    fn refuses_every_blocked_host_in_the_corpus() {
        engaged(|| {
            for (host, why) in MUST_BLOCK_HOSTS {
                let url = format!("https://{host}/hook");
                assert!(validate_outbound_url(&url).is_err(), "must refuse {host} ({why})");
            }
        });
    }

    #[test]
    fn permits_every_allowed_address_in_the_corpus() {
        engaged(|| {
            for addr in MUST_ALLOW {
                let url = format!("https://{}/hook", url_host(addr));
                assert!(validate_outbound_url(&url).is_ok(), "must permit {addr}");
            }
        });
    }
}
