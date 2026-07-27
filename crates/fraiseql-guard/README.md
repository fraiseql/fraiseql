# fraiseql-net-guard

The single outbound-address guard for the FraiseQL workspace.

Every FraiseQL crate that opens an outbound connection — JWKS fetches, federation
subgraph calls, observer webhooks, serverless-function HTTP, CDC sinks, ClickHouse,
Vault, subscription webhooks — validates its destination through this crate. There
is deliberately **one** implementation: the workspace previously carried eight
hand-rolled copies that disagreed with each other, and the gaps between them were
exploitable (see #776, #802).

The crate depends on `std` only, so it sits at the bottom of the dependency graph
and every crate can reach it.

## Usage

```rust
use fraiseql_net_guard::{blocked_host_reason, is_blocked_ip};

// Literal-IP or hostname check, brackets and loopback aliases handled.
if let Some(reason) = blocked_host_reason("[::ffff:169.254.169.254]") {
    return Err(format!("refusing outbound request: {reason}"));
}

// Post-DNS check: every resolved address must pass, or the name was rebound.
for addr in resolved {
    assert!(!is_blocked_ip(&addr.ip()));
}
```

## The bypass corpus

`fraiseql_net_guard::vectors::MUST_BLOCK` is the canonical list of addresses that
must never be reachable, each paired with the reason it is listed.
`MUST_ALLOW` is its counterweight — ordinary public addresses that must keep
working, so a future tightening cannot pass by blocking everything.

Both are public on purpose: every dependent crate asserts against the same table,
so a guard cannot drift without a test going red.
