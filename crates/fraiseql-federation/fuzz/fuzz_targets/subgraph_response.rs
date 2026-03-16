#![no_main]

use libfuzzer_sys::fuzz_target;

use fraiseql_federation::types::FederationMetadata;

fuzz_target!(|data: &[u8]| {
    // ── 1. Entity representation parser ─────────────────────────────────
    // Attempt to interpret the fuzz input as JSON and feed it through the
    // representation parser.  We expect graceful errors, never panics.
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
            let metadata = FederationMetadata::default();
            let _ = fraiseql_federation::parse_representations(&value, &metadata);
        }
    }

    // ── 2. SSRF URL validator ───────────────────────────────────────────
    // Feed arbitrary bytes as a URL string; must never panic.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = fraiseql_federation::validate_subgraph_url(s);
    }

    // ── 3. IP address blocking ──────────────────────────────────────────
    // Try to parse as an IP address and check the SSRF guard.
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(ip) = s.parse::<std::net::IpAddr>() {
            let _ = fraiseql_federation::is_ssrf_blocked_ip(&ip);
        }
    }
});
