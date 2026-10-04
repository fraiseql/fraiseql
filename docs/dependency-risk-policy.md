# Dependency Risk Acceptance Policy

## Principles

FraiseQL aims to have zero accepted CVEs with no upstream fix path. When a CVE must be
accepted temporarily, it requires explicit documentation, a mitigation strategy, and a
hard review deadline.

## Acceptance Criteria

A CVE is accepted only when **all** of the following are true:

1. No upstream fix is available (`ignore-unfixed: true` in Trivy)
2. **The exposure is stated accurately and argued.** Three of the eight current acceptances
   are for crates that *are* in the default build, so "not reachable in a default
   configuration" is not a precondition — it is one of two arguments that can carry an
   acceptance, and the row must say which one it is making:
   - **absence** — the crate is not compiled into the default artifact (`feature-gated:<f>`
     or `not-compiled`), so a default deployment cannot reach the defect; or
   - **usage** — the crate is in the build, but the specific vulnerable operation is not
     performed, or the advisory is a maintenance one with no exploit path. This argument must
     name the operation and why FraiseQL does not perform it.

   An acceptance may not be carried by an absence claim that `cargo tree` contradicts. Until
   2026-08-16 three of them were, which is why `tools/check-advisory-paths.sh` exists.
3. The risk is documented with a specific mitigation strategy
4. A review deadline is set (maximum 6 months from acceptance)
5. The entry includes a `reason` field in `deny.toml`, and the `Exposure` value in the table
   below matches what the dependency graph actually shows

## Current Accepted Advisories

**`deny.toml` is the source of truth.** This table is its published rendering: edit
`deny.toml` (and `.cargo/audit.toml`, which is kept in lockstep with it) first, then this
table. `tools/check-audit-lockstep.sh` fails when the three files disagree on which advisories
are accepted or on their deadlines, and `tools/check-advisory-paths.sh` fails when the
**Exposure** column below disagrees with `cargo tree`.

⚠ **An ignore in `deny.toml` can be broader than the row it implements.** cargo-deny cannot
scope an advisory ignore to one crate version: `[advisories] ignore` accepts only `id` and
`reason`, and a `crate = "…"` key there is a *yanked-crate* ignore that suppresses no
vulnerability at all. So when one advisory matches two resolved versions of a crate and only
one of them is acceptable, ignoring it by id silences both. Where that happens, the
default-build half must be pinned by `tools/check-default-build-minimums.sh`, which fails when
a crate in the default build falls below a declared floor. RUSTSEC-2026-0258 was the case on
record — `h2@0.3.27` accepted for the opt-in aws-* path while `h2` under hyper/axum was held at
≥ 0.4.16 by that gate. That acceptance is **resolved** (see *Resolved acceptances* below) and no
current row has this shape, but the `h2@0.4.16` floor is kept: it was verified load-bearing
(restoring 0.4.15 left `cargo deny check advisories` reporting `advisories ok` and only the
floor gate red), and a security floor on the shipped listener is worth holding on its own.

The **Exposure** column is machine-readable, and is checked against the real dependency graph
on every run of the Dagger `security` leg:

| Value | Meaning, and what the gate verifies |
|---|---|
| `default-build` | `cargo tree -i <crate>@<version> -e normal` on default features resolves the crate — it is compiled into the shipped artifact |
| `feature-gated:<f>` | absent from the default build with normal edges; present with `--features <f>` |
| `not-compiled` | absent even with `--all-features` — present in `Cargo.lock` only |

`-e normal` excludes dev- and build-dependency edges, so "not in the shipped binary" is
established by construction rather than asserted in prose. Every crate is named **with its
version**: two versions of the same crate can coexist, and an unqualified spec is ambiguous
— `deny.toml` carries two `hashbrown` and two `getrandom` acceptances for exactly
that reason.

| Advisory | Crate | Path | Exposure | Mitigation | Deadline |
|----------|-------|------|----------|------------|---------|
| RUSTSEC-2023-0071 | `rsa@0.9.10` | `jsonwebtoken` 10.4 (`rust_crypto`) → `fraiseql-auth` | `default-build` | Used to VERIFY RS256/384/512 tokens, a public-key operation the advisory does not affect. The one private-key operation is the opt-in `gcs` backend signing its own service-account assertion (Google requires RS256): server-initiated, cached, neither triggerable nor timeable by a caller. `tools/check-crypto-providers.sh` fails on any RSA private key outside `gcs.rs` (#1110) | 2027-01-31 |

⚠ This row was corrected twice. On 2026-08-16 it claimed `sqlx-mysql` (gone since #374); `rsa`
in fact arrives via `jsonwebtoken` in the **default** build. That correction then claimed an
unauthenticated login triggers RS256 signing. No shipped path ever did: every session store the
server builds signs HS256. It was re-argued on 2026-10-04 (#1110), and the unused RS256 signing
API was removed.

## Resolution Tracking

### RUSTSEC-2023-0071 (RSA Marvin Attack)

**Root cause**: the `rsa` crate has a timing sidechannel in its **private**-key operations.

**Status** (re-argued 2026-10-04, #1110): `rsa@0.9.10` is in the default build, reached as
`jsonwebtoken 10.4 (rust_crypto) → fraiseql-auth`. What FraiseQL does with it:

- **Verifies** RS256/384/512 tokens (OIDC, JWKS, configured public keys). These are public-key
  operations, and Marvin does not apply to them.
- **Signs** one thing: the opt-in `gcs` storage backend's service-account assertion
  (`crates/fraiseql-storage/src/backend/gcs.rs`), because Google requires RS256 there. The
  server initiates it and caches the token. A caller can neither trigger it on demand nor observe
  its timing.

The earlier text named `PostgresSessionStore::generate_access_token` as an attacker-triggerable
signing path. No shipped path reached it: the server builds every session store with
`with_hs256_secret`. The RS256 signing API (`with_rs256_key`, `generate_rs256_token`) had no
production caller and was removed.

**Kept true by a gate**: `tools/check-crypto-providers.sh` fails on any RSA private key
(`EncodingKey::from_rsa_*`, `RsaPrivateKey`) outside `gcs.rs` and test files. A new RS256
signer therefore cannot quietly make this acceptance false.

**Why not `jsonwebtoken`'s `aws_lc_rs` backend**: it would remove `rsa` outright, but
M-dual-crypto keeps the default build on a single crypto provider (`ring`), and aws-lc-rs is the
second one.

**Review action by 2027-01-31**: check for a constant-time `rsa` release (0.10, built on
`crypto-bigint`) and move to it.

## Resolved acceptances

Kept as a record, not as acceptances. Nothing below is ignored in `deny.toml` or
`.cargo/audit.toml` any more.

### RUSTSEC-2025-0134, -2026-0194, -2026-0195, -2026-0204 — resolved 2026-10-04

Each acceptance was due to lapse on 2026-10-31. Each was resolved rather than re-accepted:

- **RUSTSEC-2026-0204** (`crossbeam-epoch@0.9.18`, via `moka` in the default build): the lockfile
  moved to 0.9.21, inside `moka`'s range. No pin was needed.
- **RUSTSEC-2026-0194 / -0195** (`quick-xml@0.37.5`, via `samael` under `auth-saml`): `samael`
  0.0.22 moved to `quick-xml` 0.41.0, inside the patched range (`>= 0.41.0`). The SAML suites
  pass against PostgreSQL.
- **RUSTSEC-2025-0134** (`rustls-pemfile`): both CA-bundle readers (`fraiseql-wire`,
  `fraiseql-db`) parse PEM with rustls' `pki_types::pem::PemObject`, and the dependency is gone.
  It was never blocked on `bollard`: FraiseQL's own direct dependency was the whole edge.

Proven in both directions: the previous `Cargo.lock` under the new configuration fails
`cargo deny check advisories` (crossbeam-epoch) and `cargo audit` (all four). The current one
passes both. `cargo deny` resolves default features only, so the `auth-saml` pair was visible to
`cargo audit` alone.

### RUSTSEC-2026-0098 / -0099 / -0104 (rustls-webpki 0.101.7) and -0258 (h2 0.3.27) — resolved

All four had one root cause: the `aws-*` client stack resolved `rustls 0.21.12` and, with it,
`hyper 0.14`, `h2 0.3.27` and `rustls-webpki 0.101.7`. Removing that stack removed all four
advisories at once. `Cargo.lock` now carries `rustls 0.23.42` only, `hyper 1.10.1` only and
`h2 0.4.16` only; `cargo tree -i rustls@0.21` under `--all-features` reports
"did not match any packages".

**⚠ The "ruled out — do not retry" note recorded here was wrong, and it is why this sat
open.** It read: *"Every `aws-config` HTTP-client feature (`default-https-client` /
`client-hyper` / `rustls`) routes to `aws-smithy-runtime`'s legacy rustls-0.21 connector...
The fix is a code-level custom `hyper-rustls 0.27` `HttpClient`."*

Two things were true of `aws-config` and false of the graph:

1. **`aws-config`'s features were the wrong place to look.** The legacy connector was
   requested by the **`aws-sdk-*` crates' own `default` feature**, which includes
   `rustls = ["aws-smithy-runtime/tls-rustls"]` → `aws-smithy-http-client/legacy-rustls-ring`
   → `rustls 0.21`. `aws-config` itself never enables it, and pulls all of its sub-SDKs
   (`sso`, `ssooidc`, `sts`) with `default-features = false`.
2. **`default-https-client` is no longer the legacy connector.** In
   `aws-smithy-http-client` 1.3.0 it maps to `rustls-aws-lc` — rustls 0.23 — which was
   *already* compiled in alongside the legacy stack. The spike that produced the note
   predates that crate being split out.

So no custom `HttpClient` was needed. The fix is `default-features = false` on `aws-sdk-s3`
and `aws-sdk-kinesis`, restoring every member of their default set **except** `rustls`.

⚠ Cargo unifies features across the graph, so **every** declaration of an `aws-sdk-*`
dependency must carry it. One of the three (`fraiseql-server`'s `aws-sdk-s3`) was missed on
the first pass and re-enabled the legacy connector for the whole workspace, with the other
two already correct.

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
2. For advisories: include the deadline in the reason string, and a
   `# deadline: YYYY-MM-DD` comment line — `tools/check-deadlines.sh` greps for exactly that
   spelling, and prose forms match nothing
3. Update the "Skip entries last reviewed" date in the deny.toml header
4. Mirror the advisory id and deadline into `.cargo/audit.toml`
5. Add the entry to the accepted-advisories table above, **with its `Exposure` value derived
   from `cargo tree -i <crate>@<version> -e normal`** — not from what the upstream advisory
   or the previous entry says
6. Run `cargo deny check` to confirm the entry resolves the warning, then
   `bash tools/check-audit-lockstep.sh` and `bash tools/check-advisory-paths.sh`
   to confirm the three files agree and the exposure claim is true
