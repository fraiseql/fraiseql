//! The single outbound-address guard for the FraiseQL workspace.
//!
//! Every crate that opens an outbound connection validates its destination here:
//! JWKS fetches, federation subgraph calls, observer webhooks and sinks,
//! serverless-function HTTP, CDC sinks, `ClickHouse`, Vault, subscription webhooks.
//!
//! # Why this crate exists
//!
//! The workspace previously carried **eight** hand-rolled address predicates, no
//! two of which agreed. The gaps between them were exploitable: the OIDC issuer
//! allow-list accepted `IPv4`-mapped `IPv6` (#776) and so did the serverless HTTP
//! guard (#802), each found by a different reviewer in a different audit pass.
//! Four of the eight accepted `0.0.0.0`; five accepted `::ffff:169.254.169.254`,
//! which a dual-stack socket really does route to the metadata service; none
//! covered the NAT64 well-known prefix.
//!
//! One implementation, one test corpus, one place to fix the ninth vector.
//!
//! # What is blocked
//!
//! Anything that is not a globally-routable unicast address a public service
//! could legitimately live on. The full enumeration with per-range rationale is
//! [`vectors::MUST_BLOCK`]; [`vectors::MUST_ALLOW`] is its counterweight, so a
//! future tightening cannot pass by blocking everything.
//!
//! `IPv4`-mapped (`::ffff:0:0/96`) and NAT64 (`64:ff9b::/96`) addresses embed an
//! `IPv4` address that the network stack will actually route to. They are
//! canonicalised and re-checked under the `IPv4` rules rather than blanket-blocked,
//! so `::ffff:8.8.8.8` stays usable while `::ffff:169.254.169.254` does not.
//!
//! # What is *not* blocked
//!
//! DNS. A hostname that resolves to a blocked address is only caught once it is
//! resolved, so a caller must check every resolved address with [`is_blocked_ip`]
//! **and** pin the connection to those addresses — validating and then letting the
//! HTTP client re-resolve leaves a rebinding window open.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub mod vectors;

/// Why an outbound destination was refused.
///
/// Carries enough detail for an operator-facing error without echoing the whole
/// URL back into a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedReason {
    /// A hostname that always denotes the local machine (`localhost` and its aliases).
    LoopbackHostname,
    /// A hostname well known to serve cloud instance metadata.
    MetadataHostname,
    /// A literal address inside a range outbound requests must never reach.
    ReservedAddress,
}

impl BlockedReason {
    /// A short operator-facing description of the refusal.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LoopbackHostname => "host resolves to the local machine",
            Self::MetadataHostname => "host is a cloud instance-metadata endpoint",
            Self::ReservedAddress => "address is in a private or reserved range",
        }
    }
}

impl std::fmt::Display for BlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Hostnames that serve cloud instance metadata and must never be contacted.
///
/// The addresses these resolve to are already blocked, but the literal-host path
/// may run before any DNS lookup, and an operator-controlled resolver can point
/// them anywhere. Matching the name as well costs nothing.
const METADATA_HOSTNAMES: &[&str] = &["metadata.google.internal", "metadata.goog", "instance-data"];

/// Returns `true` for addresses that outbound requests must never contact.
///
/// Covers loopback, RFC 1918 private space, link-local (including every cloud's
/// instance-metadata address), CGNAT, this-network, IETF protocol assignments,
/// documentation and benchmarking ranges, multicast, and the reserved-for-future
/// `240.0.0.0/4` block including the broadcast address — and the `IPv6`
/// equivalents, with `IPv4`-mapped and NAT64 addresses canonicalised to the
/// `IPv4` address the stack would actually route to.
#[must_use]
pub const fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(*v4),
        IpAddr::V6(v6) => is_blocked_v6(*v6),
    }
}

/// The `IPv4` half of [`is_blocked_ip`].
#[must_use]
const fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    let [oct0, oct1, oct2, _] = ip.octets();
    oct0 == 0                                   // 0.0.0.0/8      this-network, incl. the unspecified address
        || oct0 == 10                           // 10.0.0.0/8     RFC 1918
        || oct0 == 127                          // 127.0.0.0/8    loopback
        || (oct0 == 100 && (oct1 & 0b1100_0000) == 64) // 100.64.0.0/10  CGNAT (RFC 6598); covers Alibaba IMDS
        || (oct0 == 169 && oct1 == 254)            // 169.254.0.0/16 link-local; AWS/GCP/Azure IMDS 169.254.169.254
        || (oct0 == 172 && (oct1 & 0b1111_0000) == 16) // 172.16.0.0/12  RFC 1918
        || (oct0 == 192 && oct1 == 0 && oct2 == 0)    // 192.0.0.0/24   IETF protocol assignments; Oracle IMDS 192.0.0.192
        || (oct0 == 192 && oct1 == 0 && oct2 == 2)    // 192.0.2.0/24   TEST-NET-1 (RFC 5737)
        || (oct0 == 192 && oct1 == 168)            // 192.168.0.0/16 RFC 1918
        || (oct0 == 198 && (oct1 & 0b1111_1110) == 18) // 198.18.0.0/15  benchmarking (RFC 2544)
        || (oct0 == 198 && oct1 == 51 && oct2 == 100) // 198.51.100.0/24 TEST-NET-2 (RFC 5737)
        || (oct0 == 203 && oct1 == 0 && oct2 == 113)  // 203.0.113.0/24 TEST-NET-3 (RFC 5737)
        || oct0 >= 224 // 224.0.0.0/4 multicast + 240.0.0.0/4 reserved, incl. 255.255.255.255 broadcast
}

/// The `IPv6` half of [`is_blocked_ip`].
///
/// `IPv4`-mapped and NAT64 addresses are unwrapped and judged as the `IPv4`
/// address the stack would route to, not blanket-refused: a mapped public
/// address is a public address.
#[must_use]
const fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if let Some(embedded) = embedded_v4(ip) {
        return is_blocked_v4(embedded);
    }
    let seg = ip.segments();
    ip.is_loopback()                          // ::1
        || ip.is_unspecified()                // ::
        || (seg[0] & 0xfe00) == 0xfc00          // fc00::/7     unique-local
        || (seg[0] & 0xffc0) == 0xfe80          // fe80::/10    link-local
        || (seg[0] & 0xffc0) == 0xfec0          // fec0::/10    site-local (deprecated, still routed by some stacks)
        || (seg[0] & 0xff00) == 0xff00          // ff00::/8     multicast
        || (seg[0] == 0x0100 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0) // 100::/64  discard-only (RFC 6666)
        || (seg[0] == 0x2001 && seg[1] == 0x0db8) // 2001:db8::/32 documentation
        || (seg[0] == 0x2001 && seg[1] == 0x0002 && seg[2] == 0) // 2001:2::/48 benchmarking
        || (seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2] == 0x0001) // 64:ff9b:1::/48 NAT64 local-use
        || is_v4_compatible(ip) // ::/96 deprecated IPv4-compatible; no legitimate use
}

/// Extracts the `IPv4` address embedded in a mapped or NAT64 `IPv6` address.
///
/// - `::ffff:0:0/96` — `IPv4`-mapped. A dual-stack socket connecting to one of these reaches the
///   corresponding `IPv4` host, which is what made #776 and #802 exploitable.
/// - `64:ff9b::/96` — the NAT64 well-known prefix (RFC 6052). Wherever a NAT64 gateway exists this
///   is a live route to the embedded `IPv4` address, including the metadata service.
/// - `64:ff9b:1::/48` — NAT64 local-use prefixes (RFC 8215). The embedded address sits at a
///   translator-defined offset, so these are refused wholesale by [`is_blocked_v6`] rather than
///   unwrapped.
const fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let seg = ip.segments();
    let is_mapped =
        seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0xffff;
    let is_nat64_wellknown = seg[0] == 0x0064
        && seg[1] == 0xff9b
        && seg[2] == 0
        && seg[3] == 0
        && seg[4] == 0
        && seg[5] == 0;
    if is_mapped || is_nat64_wellknown {
        let [hi0, hi1] = seg[6].to_be_bytes();
        let [lo0, lo1] = seg[7].to_be_bytes();
        return Some(Ipv4Addr::new(hi0, hi1, lo0, lo1));
    }
    None
}

/// Returns `true` for the deprecated `IPv4`-compatible form `::a.b.c.d`.
///
/// Linux does not translate these to an `IPv4` connection, so they are a hygiene
/// gap rather than an exploit primitive — but nothing legitimate emits them.
const fn is_v4_compatible(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    seg[0] == 0
        && seg[1] == 0
        && seg[2] == 0
        && seg[3] == 0
        && seg[4] == 0
        && seg[5] == 0
        && !(seg[6] == 0 && (seg[7] == 0 || seg[7] == 1)) // exclude :: and ::1, handled above
}

/// Returns `true` if `host` denotes the local machine.
///
/// Distinguishes "this is my own dev service" from the rest of the blocked set.
/// A development escape hatch that exists to reach a broker or database on
/// `localhost` should use this rather than disabling the address guard wholesale
/// — otherwise the escape hatch also unlocks the instance-metadata service.
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.starts_with("localhost.") {
        return true;
    }
    let candidate = lower.split_once(':').map_or(lower.as_str(), |(head, _)| head);
    match lower.parse::<IpAddr>().or_else(|_| candidate.parse::<IpAddr>()) {
        Ok(IpAddr::V4(v4)) => v4.is_loopback(),
        // A mapped or NAT64 loopback is still loopback.
        Ok(IpAddr::V6(v6)) => {
            v6.is_loopback() || embedded_v4(v6).is_some_and(|v4| v4.is_loopback())
        },
        Err(_) => false,
    }
}

/// Returns the reason a host string must not be contacted, or `None` if it is allowed.
///
/// Accepts a raw URL host component: `IPv6` literals may carry the brackets the
/// `url` crate leaves in `host_str()`, and matching is case-insensitive.
///
/// A hostname that is not a literal address returns `None` unless it is a known
/// loopback or metadata alias — the caller must still resolve it and check every
/// resulting address with [`is_blocked_ip`].
#[must_use]
pub fn blocked_host_reason(host: &str) -> Option<BlockedReason> {
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    let lower = host.to_ascii_lowercase();

    if lower == "localhost" || lower.ends_with(".localhost") || lower.starts_with("localhost.") {
        return Some(BlockedReason::LoopbackHostname);
    }
    // Exact match or a true subdomain — `notmetadata.goog` is somebody else's domain.
    if METADATA_HOSTNAMES
        .iter()
        .any(|name| lower == *name || lower.ends_with(&format!(".{name}")))
    {
        return Some(BlockedReason::MetadataHostname);
    }
    // A bare IPv4 host may still carry a port; an IPv6 literal never reaches here
    // with one, because the port sits outside the brackets.
    let candidate = lower.split_once(':').map_or(lower.as_str(), |(head, _)| head);
    if let Ok(ip) = lower.parse::<IpAddr>().or_else(|_| candidate.parse::<IpAddr>()) {
        if is_blocked_ip(&ip) {
            return Some(BlockedReason::ReservedAddress);
        }
    }
    None
}

#[cfg(test)]
mod tests;
