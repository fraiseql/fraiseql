//! The canonical outbound-address corpus.
//!
//! [`MUST_BLOCK`] is every address an outbound request must never reach, paired
//! with the reason it is listed. [`MUST_ALLOW`] is its counterweight: ordinary
//! public addresses that must keep working, so a guard cannot pass the corpus by
//! refusing everything.
//!
//! Both are public API on purpose. Every crate that makes outbound requests
//! asserts its own entry point against these tables, so a guard cannot drift
//! without a test going red — which is precisely what happened when the
//! workspace maintained eight separate copies.
//!
//! # Adding a vector
//!
//! When a new bypass is found, add it here **first** and watch the dependent
//! crates' tests fail. That is the signal that the corpus is actually wired
//! through to the guards rather than being a decorative list.

/// An address that must always be refused, with the reason it is listed.
pub type BlockedVector = (&'static str, &'static str);

/// Addresses no outbound request may reach.
///
/// Ordered by family and then by the range they exercise, so a gap in a guard
/// shows up as a contiguous block of failures rather than a scatter.
pub const MUST_BLOCK: &[BlockedVector] = &[
    // ---- IPv4: the ranges every copy already covered -----------------------
    ("127.0.0.1", "loopback 127.0.0.0/8"),
    ("127.1.2.3", "loopback is a whole /8, not one address"),
    ("10.0.0.1", "RFC 1918 10.0.0.0/8"),
    ("172.16.0.1", "RFC 1918 172.16.0.0/12 lower bound"),
    ("172.31.255.254", "RFC 1918 172.16.0.0/12 upper bound"),
    ("192.168.1.1", "RFC 1918 192.168.0.0/16"),
    ("169.254.169.254", "AWS/GCP/Azure instance metadata"),
    ("169.254.0.1", "link-local 169.254.0.0/16"),
    ("100.64.0.1", "CGNAT 100.64.0.0/10 lower bound"),
    ("100.127.255.254", "CGNAT 100.64.0.0/10 upper bound"),
    ("100.100.100.200", "Alibaba Cloud instance metadata (inside CGNAT)"),
    // ---- IPv4: ranges four of the eight copies missed -----------------------
    ("0.0.0.0", "unspecified; binds to every local interface"),
    ("0.1.2.3", "this-network 0.0.0.0/8, not just the unspecified address"),
    ("192.0.0.192", "Oracle Cloud instance metadata (IETF protocol assignments)"),
    ("255.255.255.255", "broadcast"),
    ("240.0.0.1", "reserved-for-future-use 240.0.0.0/4"),
    ("224.0.0.1", "multicast 224.0.0.0/4"),
    ("198.18.0.1", "benchmarking 198.18.0.0/15 (RFC 2544)"),
    ("192.0.2.1", "TEST-NET-1 documentation range"),
    ("198.51.100.1", "TEST-NET-2 documentation range"),
    ("203.0.113.1", "TEST-NET-3 documentation range"),
    // ---- IPv6: the ranges most copies covered ------------------------------
    ("::1", "loopback"),
    ("::", "unspecified"),
    ("fe80::1", "link-local fe80::/10"),
    ("fc00::1", "unique-local fc00::/7 lower bound"),
    ("fd00::1", "unique-local, the half actually assigned"),
    // ---- IPv6: IPv4-mapped — #776 and #802, live on a dual-stack socket -----
    ("::ffff:127.0.0.1", "IPv4-mapped loopback; reaches the IPv4 host"),
    ("::ffff:169.254.169.254", "IPv4-mapped instance metadata"),
    ("::ffff:a9fe:a9fe", "the same address in hextet form"),
    ("::ffff:10.0.0.1", "IPv4-mapped RFC 1918"),
    ("::ffff:192.168.1.1", "IPv4-mapped RFC 1918"),
    ("::ffff:0.0.0.0", "IPv4-mapped unspecified"),
    // ---- IPv6: NAT64 — no copy covered this ---------------------------------
    ("64:ff9b::169.254.169.254", "NAT64 well-known prefix to instance metadata"),
    ("64:ff9b::a9fe:a9fe", "the same NAT64 address in hextet form"),
    ("64:ff9b::127.0.0.1", "NAT64 to loopback"),
    ("64:ff9b::10.0.0.1", "NAT64 to RFC 1918"),
    (
        "64:ff9b:1::1",
        "NAT64 local-use prefix (RFC 8215); offset is translator-defined",
    ),
    // ---- IPv6: remaining hygiene -------------------------------------------
    ("::127.0.0.1", "deprecated IPv4-compatible form"),
    ("::0.0.0.2", "deprecated IPv4-compatible form"),
    ("ff02::1", "multicast ff00::/8"),
    ("fec0::1", "deprecated site-local fec0::/10"),
    ("100::1", "discard-only 100::/64 (RFC 6666)"),
    ("2001:db8::1", "documentation 2001:db8::/32"),
    ("2001:2::1", "benchmarking 2001:2::/48"),
];

/// Addresses that must keep working.
///
/// Without this table a guard could pass [`MUST_BLOCK`] by refusing every
/// address. `::ffff:8.8.8.8` is the load-bearing entry: mapped addresses are
/// canonicalised and judged as `IPv4`, not blanket-refused, so a mapped *public*
/// address stays reachable.
pub const MUST_ALLOW: &[&str] = &[
    "8.8.8.8",
    "1.1.1.1",
    "104.16.0.1", // Cloudflare; an earlier guard blocked the whole 100..=127 first octet
    "172.32.0.1", // just above RFC 1918 172.16.0.0/12
    "172.15.255.255", // just below it
    "100.63.255.255", // just below CGNAT 100.64.0.0/10
    "100.128.0.1", // just above it
    "192.0.1.1",  // just above the IETF protocol-assignment /24
    "223.255.255.255", // just below multicast
    "198.20.0.1", // just above the benchmarking /15
    "2001:4860:4860::8888", // Google public DNS
    "2606:4700:4700::1111", // Cloudflare public DNS
    "::ffff:8.8.8.8", // mapped public address — must NOT be blanket-blocked
    "2001:db9::1", // adjacent to the documentation range
];

/// Hostnames that must be refused before any DNS lookup runs.
pub const MUST_BLOCK_HOSTS: &[BlockedVector] = &[
    ("localhost", "bare loopback name"),
    ("LOCALHOST", "loopback names are case-insensitive"),
    ("api.localhost", "`.localhost` subdomain"),
    ("localhost.localdomain", "`localhost.` prefix alias common in /etc/hosts"),
    ("localhost.evil.com", "attacker-registered `localhost.` prefix"),
    ("metadata.google.internal", "GCP instance-metadata name"),
    (
        "[::ffff:169.254.169.254]",
        "bracketed literal, as `url` leaves it in host_str()",
    ),
    ("127.0.0.1:5432", "literal with a port"),
];

/// Hostnames that must survive the guard.
pub const MUST_ALLOW_HOSTS: &[&str] = &[
    "example.com",
    "api.example.com",
    "notmetadata.goog", // shares a suffix with a metadata name but is somebody else's domain
    "localhostage.com", // starts with `localhost` but is not a loopback alias
    "8.8.8.8:443",
    "[2001:4860:4860::8888]",
];

/// Formats a corpus address for embedding in a URL's host position.
///
/// `IPv6` literals are bracketed, as the `url` crate requires and as
/// [`crate::net::blocked_host_reason`] accepts. Hostnames pass through.
///
/// Every dependent crate's guard test uses this to turn a corpus row into a URL
/// its own entry point accepts, so the tables stay the single source of truth
/// rather than being re-typed per crate.
#[must_use]
pub fn url_host(addr: &str) -> String {
    if addr.contains(':') && !addr.starts_with('[') {
        format!("[{addr}]")
    } else {
        addr.to_owned()
    }
}
