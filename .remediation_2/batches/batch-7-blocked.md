# Batch 7 — Blocked: rustls 0.21.12 CVE (V3)

## Status: ❌ BLOCKED on upstream

## Issue

`rustls 0.21.12` (EOL branch) is in `Cargo.lock` as a transitive dependency:

```
fraiseql-server
  → aws-sdk-s3
    → hyper-rustls 0.24.2
      → rustls 0.21.12   ← EOL, CVE GHSA-6g18-jhpc-69jc (RSA-PSS)
```

FraiseQL cannot update this without AWS updating `aws-sdk-s3` to depend on
`hyper-rustls ≥ 0.25`, which uses `rustls 0.23+`.

---

## Tracking

**Upstream issue**: https://github.com/awslabs/aws-sdk-rust/issues
(search for `hyper-rustls 0.25` or `rustls 0.23`)

**Resolution timeline**:
- AWS SDK Rust team has been actively updating. Check quarterly.
- `aws-sdk-s3 ≥ 1.70` is likely to include the `hyper-rustls 0.25` bump.
  Pin to this version once released and verify Cargo.lock no longer contains
  `rustls 0.21.*`.

**Calendar reminder**: Set for **2026-06-05** (90 days from campaign start).

---

## Escalation Plan (if AWS SDK not updated by 2026-06-05)

Replace `aws-sdk-s3` with `object_store`:

```toml
# crates/fraiseql-server/Cargo.toml
[dependencies]
# Replace:
# aws-sdk-s3 = "1"
# aws-config = "1"

# With:
object_store = { version = "0.11", features = ["aws"] }
```

`object_store` uses `reqwest` (which uses `rustls 0.23`) for HTTP, eliminating
the `hyper-rustls 0.24` transitive dependency entirely.

**Migration impact**:
- `fraiseql-server/src/backup/s3.rs` would need a rewrite from the AWS SDK
  API to the `object_store` API.
- `object_store` is maintained by the Apache Arrow project and is the
  standard for cloud object storage in the Rust ecosystem.
- API surface is simpler (put/get/list/delete) — the backup use case maps
  directly.

---

## Verification (when resolved)

```bash
# Confirm rustls 0.21 is gone:
cargo tree | grep "rustls"
# Must show only rustls 0.22+ or 0.23+

# Confirm no CVE:
cargo audit
# Must return zero issues for rustls
```
