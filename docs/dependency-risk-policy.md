# Dependency Risk Acceptance Policy

## Principles

FraiseQL aims to have zero accepted CVEs with no upstream fix path. When a CVE must be
accepted temporarily, it requires explicit documentation, a mitigation strategy, and a
hard review deadline.

## Acceptance Criteria

A CVE is accepted only when **all** of the following are true:

1. No upstream fix is available (`ignore-unfixed: true` in Trivy)
2. The vulnerable code path is not reachable in default configurations
3. The risk is documented with a specific mitigation strategy
4. A review deadline is set (maximum 6 months from acceptance)
5. The entry includes a `reason` field in `deny.toml`

## Current Accepted Advisories

| Advisory | Crate | Path | Mitigation | Deadline |
|----------|-------|------|------------|---------|
| RUSTSEC-2023-0071 | `rsa` | `sqlx-mysql` (optional `mysql` feature) | TLS at load balancer; mysql not default | 2026-10-01 |
| RUSTSEC-2025-0134 | `rustls-pemfile` | `bollard` (optional Docker feature) | Blocked on bollard upstream; assess drop | 2026-07-01 |
| RUSTSEC-2026-0098 | `rustls-webpki` | `aws-smithy-http-client` (optional `aws-s3`) | Optional feature; no default exploit path | 2026-06-15 |
| RUSTSEC-2026-0099 | `rustls-webpki` | `aws-smithy-http-client` (optional `aws-s3`) | Same as above | 2026-06-15 |

## Resolution Tracking

### RUSTSEC-2023-0071 (RSA Marvin Attack)

**Root cause**: `mysql_async` crate uses the `rsa` crate, which has a timing sidechannel.

**Blocked on**: `sqlx` upgrade to 0.9+ (which removes `mysql_async` or upgrades it).
`sqlx 0.9.0-alpha.1` is available as of 2026-04-25 — monitor for stable release.

**Mitigation**:
- `mysql` feature is non-default; users must explicitly opt in
- TLS termination at the load balancer prevents timing attacks over the wire
- No client input reaches the RSA code path in typical deployments

**Review action by 2026-10-01**:
1. If `sqlx 0.9` stable is released: upgrade and remove this entry
2. If still blocked: evaluate replacing `mysql_async` with a pure-TLS connector

### RUSTSEC-2026-0098 / RUSTSEC-2026-0099 (rustls-webpki name constraints)

**Root cause**: `aws-smithy-http-client` depends on `rustls 0.21` via `hyper-rustls 0.24`.

**Blocked on**: `aws-sdk-s3` migrating to `hyper-rustls 0.27` (rustls 0.23).

**Review action by 2026-06-15**:
1. Check `aws-sdk-s3` changelog for rustls 0.23 migration
2. If migrated: upgrade `aws-sdk-s3` and remove the `[[bans.skip]]` entries for
   `hyper-rustls 0.24.2`, `tokio-rustls 0.24.1`, `rustls 0.21.12`
3. If not migrated: evaluate pinning the aws-sdk feature behind a more restrictive
   opt-in or sourcing an alternative S3 client

## Dependency Upgrade Policy

| Change type | Timeline |
|------------|----------|
| Patch (x.y.Z) | Automatic via Dependabot weekly; no review required |
| Minor (x.Y.z) | Review within 30 days; run full test suite |
| Major (X.y.z) | Review within 90 days; prefer grouping in one PR |
| EOL crate | Resolve or formally defer within 60 days of EOL notice |
| New CVE (CRITICAL) | Resolve within 7 days; accept with documented deadline only if no fix exists |
| New CVE (HIGH) | Resolve within 30 days |

## Multi-Version Skip List Review

The `[[bans.skip]]` entries in `deny.toml` represent transitive duplicate versions
that cannot be eliminated without upstream changes. Review these quarterly:

1. Run `cargo tree --duplicates` to see the current state
2. For each entry, check if the upstream dependency has been updated
3. Remove entries where the duplication is resolved
4. Update the `# Skip entries last reviewed:` date in `deny.toml`

The current skip list primarily falls into these categories:

- **aws-sdk-s3 chain** (rustls 0.21, hyper-rustls 0.24, tokio-rustls 0.24,
  aws-smithy-http, aws-smithy-json): Resolves when aws-sdk-s3 migrates to rustls 0.23
- **rand ecosystem** (rand 0.7/0.8, rand_chacha, rand_core, getrandom): Resolves
  when `quickcheck` and other old-rand consumers upgrade
- **thiserror 1.x**: Resolves when `graphql-parser` migrates to thiserror 2.x
- **windows-sys** (0.48/0.52/0.59): Resolves as Windows ecosystem standardises
- **wasi** (0.9/0.11): Transitive; resolves with getrandom consolidation

## Adding a New Skip Entry

When adding a new `[[bans.skip]]` or `[[advisories.ignore]]` entry:

1. Add a `reason` field explaining the root cause
2. For advisories: include the deadline in the reason string
3. Update the "Skip entries last reviewed" date in the deny.toml header
4. Add the entry to the table above in this document
5. Run `cargo deny check` to confirm the entry resolves the warning
