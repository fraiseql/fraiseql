//! Resolve a host and check every address it resolves to: the one implementation.
//!
//! [`blocked_host_reason`] and [`is_blocked_ip`] say what must not be contacted. This is
//! the step that applies them to a real destination: refuse the hostname aliases, resolve
//! it, and refuse it if *any* address is blocked. Federation, observers, JWKS and the
//! functions HTTP host each used to write this step themselves, and differed in whether
//! they consulted the alias list at all (#1360). A rebinding fix now has one place to
//! land.
//!
//! The addresses that passed are returned. A caller that connects should pin its client
//! to exactly those: letting the HTTP client resolve again leaves the rebinding window
//! this check exists to close.

use std::net::{IpAddr, SocketAddr};

use super::{BlockedReason, blocked_host_reason, is_blocked_ip};

/// Why a destination was refused by [`resolve_and_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The host itself is refused before any lookup: a loopback or metadata alias, or a
    /// literal address in a blocked range.
    Host(BlockedReason),
    /// The lookup failed.
    ResolutionFailed(String),
    /// The lookup returned no address.
    NoAddresses,
    /// The host resolved to an address in a blocked range.
    BlockedAddress(IpAddr),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(reason) => write!(f, "{reason}"),
            Self::ResolutionFailed(error) => write!(f, "DNS resolution failed: {error}"),
            Self::NoAddresses => f.write_str("DNS resolved to no addresses"),
            Self::BlockedAddress(ip) => {
                write!(f, "resolves to {ip}, which is in a private or reserved range")
            },
        }
    }
}

impl std::error::Error for Refusal {}

/// Refuse `host` if it is a blocked alias or address, else resolve it with `port` and
/// refuse it if any resolved address is blocked.
///
/// `host` is a URL host component; `IPv6` literals may keep their brackets. A literal
/// address is checked as is and returned without a lookup.
///
/// # Errors
///
/// The [`Refusal`] naming why: a refused host, a failed or empty lookup, or the first
/// blocked address.
pub async fn resolve_and_check(host: &str, port: u16) -> Result<Vec<SocketAddr>, Refusal> {
    if let Some(reason) = blocked_host_reason(host) {
        return Err(Refusal::Host(reason));
    }
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        // `blocked_host_reason` already refused a blocked literal; this one is allowed,
        // and a literal cannot be rebound.
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((bare, port))
        .await
        .map_err(|e| Refusal::ResolutionFailed(e.to_string()))?
        .collect();
    check_resolved(addrs)
}

/// Refuse an empty resolution, or one with any blocked address: a name answering with
/// one public and one private address is refused, not filtered.
fn check_resolved(addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, Refusal> {
    if addrs.is_empty() {
        return Err(Refusal::NoAddresses);
    }
    if let Some(addr) = addrs.iter().find(|addr| is_blocked_ip(&addr.ip())) {
        return Err(Refusal::BlockedAddress(addr.ip()));
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests;
