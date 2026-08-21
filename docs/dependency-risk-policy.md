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
a crate in the default build falls below a declared floor. RUSTSEC-2026-0258 is the case on
record: `h2@0.3.27` is accepted below, while `h2` under hyper/axum is held at ≥ 0.4.16 by that
gate — verified load-bearing, since restoring 0.4.15 leaves `cargo deny check advisories`
reporting `advisories ok` and only the floor gate red.

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
(see the RUSTSEC-2026-0098 row).

| Advisory | Crate | Path | Exposure | Mitigation | Deadline |
|----------|-------|------|----------|------------|---------|
| RUSTSEC-2023-0071 | `rsa@0.9.10` | `jsonwebtoken` 10.4 → `fraiseql-auth` | `default-build` | No fixed `rsa` release exists. The exposure is RS256 access-token signing (`session_postgres.rs:175`), an attacker-triggerable private-key operation — the shape Marvin targets. Re-argue on this path by the deadline; do not re-approve on the old `sqlx-mysql` text | 2026-10-01 |
| RUSTSEC-2025-0134 | `rustls-pemfile@2.2.0` | direct dependency of `fraiseql-wire`; optional in `fraiseql-db` | `default-build` | Deprecated/unmaintained-crate advisory, not an exploitable defect, so presence in the build is not itself an exploit path. Blocked on `bollard` migrating off `pem` 0.x upstream | 2026-10-01 |
| RUSTSEC-2026-0098 | `rustls-webpki@0.101.7` | `aws-smithy-http-client` (rustls 0.21) → `aws-config` | `feature-gated:aws-s3` | Certificate-validation defect reachable only on a TLS handshake an `aws-*` client makes against a hostile or misissued chain. Not in the default build; not remotely triggerable by a FraiseQL client. Blocked on the aws stack reaching rustls 0.23 (#1111) | 2026-12-01 |
| RUSTSEC-2026-0099 | `rustls-webpki@0.101.7` | `aws-smithy-http-client` (rustls 0.21) → `aws-config` | `feature-gated:aws-s3` | Same root cause and same reasoning as RUSTSEC-2026-0098 | 2026-12-01 |
| RUSTSEC-2026-0104 | `rustls-webpki@0.101.7` | `aws-smithy-http-client` (rustls 0.21) → `aws-config` | `feature-gated:aws-s3` | CRL-parsing panic; same root cause and same reasoning as RUSTSEC-2026-0098 | 2026-12-01 |
| RUSTSEC-2026-0194 | `quick-xml@0.37.5` | `samael` 0.0.21 → `fraiseql-auth` | `feature-gated:auth-saml` | Quadratic run time checking a start tag for duplicate attribute names. SAML SP only, opt-in; `samael` pins `quick-xml` 0.37.5 and no fix is in range | 2026-10-01 |
| RUSTSEC-2026-0195 | `quick-xml@0.37.5` | `samael` 0.0.21 → `fraiseql-auth` | `feature-gated:auth-saml` | Unbounded namespace-declaration allocation in `NsReader`. Same path and same upstream block as RUSTSEC-2026-0194 | 2026-10-01 |
| RUSTSEC-2026-0204 | `crossbeam-epoch@0.9.18` | `moka` 0.12 → `fraiseql-core` | `default-build` | Invalid-pointer dereference reachable only when `fmt::Pointer` Debug-formats an invalid `Atomic`/`Shared`, which `moka` does not do. Accepted on usage grounds, not on absence from the build | 2026-10-01 |
| RUSTSEC-2026-0258 | `h2@0.3.27` | `aws-smithy-http-client` (hyper 0.14) → `aws-config` | `feature-gated:aws-s3` | Unbounded empty-DATA-frame queueing (DoS). No fix exists in the 0.3 series and `aws-smithy-http-client` pins hyper 0.14. **The default build is not affected** — its `h2` was bumped to 0.4.16 rather than accepted, so this acceptance is scoped to `h2@0.3.27` in `deny.toml`. Blocked on the same aws-stack migration as RUSTSEC-2026-0098 (#1111) | 2026-12-01 |

⚠ Three of these rows were corrected on 2026-08-16 after being checked against `cargo tree`
rather than trusted: RUSTSEC-2023-0071 claimed `sqlx-mysql` (gone since #374, and `rsa` in
fact arrives via `jsonwebtoken` in the **default** build — #1110); RUSTSEC-2025-0134 claimed
dev-dependency-only; RUSTSEC-2026-0204 claimed criterion dev-dependencies (#1137). All three
acceptances still stand, but two of them stood on sentences that were false, which is the
condition the `Exposure` gate now makes impossible to leave in place.

## Resolution Tracking

### RUSTSEC-2023-0071 (RSA Marvin Attack)

**Root cause**: the `rsa` crate has a timing sidechannel in its private-key operations.

**Status**: `rsa@0.9.10` is in the **default build**, reached as
`jsonwebtoken 10.4 → fraiseql-auth`. `fraiseql-auth`'s `generate_rs256_token`
(`crates/fraiseql-auth/src/session_postgres.rs:175`) signs access tokens with an RSA
**private** key, so the vulnerable operation is one an unauthenticated caller can trigger by
completing a login. That is the shape the Marvin Attack targets.

This entry previously recorded the opposite — "`sqlx-mysql`, lockfile-only, never compiled".
That path was removed with MySQL support in #374; `cargo tree -i sqlx-mysql --all-features`
prints nothing. The acceptance was corrected in `deny.toml` on 2026-08-13 and here on
2026-08-16 (#1110).

**Blocked on**: nothing upstream — there is no fixed `rsa` release, and none is scheduled.
The `sqlx 0.9` upgrade that used to be the tracked resolution path is irrelevant to the real
dependency edge.

**Review action by 2026-10-01**: the choice is between continuing to accept the timing
sidechannel on RS256 signing and moving RS256 issuance off `jsonwebtoken`'s `rsa` backend.
Decide that, on this path. Re-approving on the removed MySQL text is not a review.

### RUSTSEC-2025-0134 (rustls-pemfile deprecated)

**Root cause**: `rustls-pemfile` is deprecated in favour of `rustls-pki-types`' own PEM
support. This is a maintenance advisory, not an exploitable defect.

**Status**: `rustls-pemfile@2.2.0` is a direct `[dependencies]` entry of `fraiseql-wire`
(`Cargo.toml:37`) and an optional one of `fraiseql-db`, so it **is** in the default build.
The acceptance previously recorded "DEV-dependency only … no production runtime exposure",
which was false (#1137). It stands anyway — a deprecated crate in the build is a maintenance
risk, not an attack path — but on those grounds, not the old ones.

**Blocked on**: `bollard` migrating off `pem` 0.x upstream, and on `fraiseql-wire` moving its
own PEM parsing to `rustls-pki-types`. The second is in FraiseQL's hands and is the real
resolution path.

**Review action by 2026-10-01**: migrate `fraiseql-wire` off `rustls-pemfile`, or re-accept
with the maintenance risk stated.

### RUSTSEC-2026-0098 / -0099 / -0104 (rustls-webpki 0.101.7)

**Root cause**: `aws-smithy-http-client` pulls `rustls 0.21`, which pulls
`rustls-webpki 0.101.7` — two name-constraint bugs and a CRL-parsing panic.

**Status**: `feature-gated:aws-s3`. Verified by construction, not assertion: with default
features `cargo tree -i rustls-webpki@0.101.7 -e normal` does not resolve the package at all,
and the default build carries only `rustls-webpki@0.103.13` via `rustls 0.23`. All three
defects are certificate-**validation** bugs, reachable only on a TLS handshake an `aws-*`
client makes against a hostile or misissued chain — an operator who opted into S3 or Kinesis
*and* whose AWS endpoint is already being impersonated. They are not reachable by a FraiseQL
client.

**Blocked on**: the aws stack reaching rustls 0.23. **Ruled out — do not retry**: the #975
coordinated bump moved `aws-sdk-kinesis` to 1.112, `aws-config` to 1.10.1, `aws-sdk-s3` to
1.141 and `aws-smithy-http-client` to 1.3.0, and `rustls 0.21.12` survived all of it. Every
`aws-config` HTTP-client feature (`default-https-client` / `client-hyper` / `rustls`) routes
to `aws-smithy-runtime`'s legacy rustls-0.21 connector. The fix is a code-level custom
`hyper-rustls 0.27` `HttpClient`, tracked as **#1111**.

**Review action by 2026-12-01** — deliberately staggered off the 2026-10-01 cluster so that
six acceptances do not lapse on one day and block every open branch at once:

1. Check whether `aws-smithy-http-client` has a rustls-0.23 path
2. If it does: upgrade and remove the `[[bans.skip]]` entries for `hyper-rustls 0.24.2`,
   `tokio-rustls 0.24.1`, `rustls 0.21.12`
3. If not: progress #1111, or state why the opt-in exposure remains acceptable

### RUSTSEC-2026-0194 / -0195 (quick-xml DoS pair)

**Root cause**: quadratic duplicate-attribute checking, and unbounded namespace-declaration
allocation in `NsReader`.

**Status**: `feature-gated:auth-saml`. `quick-xml@0.37.5` arrives via
`samael 0.0.21 → fraiseql-auth` and is absent from the default build. A deployment that
enables SAML SP support is parsing attacker-supplied XML assertions, so the exposure is real
for that configuration.

**Blocked on**: `samael` pinning `quick-xml` 0.37.5 with no fixed version in range.

**Review action by 2026-10-01**: check for a `samael` release that relaxes the pin; otherwise
state whether SAML SP deployments should carry a request-size limit in front of assertion
parsing.

### RUSTSEC-2026-0204 (crossbeam-epoch invalid pointer)

**Root cause**: `fmt::Pointer` on an invalid `Atomic`/`Shared` dereferences it.

**Status**: `default-build` — `crossbeam-epoch@0.9.18` arrives via `moka 0.12 → fraiseql-core`,
the result cache, not via `rayon`/`criterion` dev-dependencies as previously recorded
(#1137). Accepted on **usage** grounds: reaching the defect requires Debug-formatting an
invalid pointer, which `moka` does not do and FraiseQL does not do.

**Blocked on**: a `crossbeam-epoch` release `moka` will take.

**Review action by 2026-10-01**: re-check `moka`'s dependency range for a fixed
`crossbeam-epoch`.

### RUSTSEC-2026-0258 (h2 unbounded empty DATA frames)

**Root cause**: `h2` accepts and queues empty DATA frames without limit, so a peer can drive
memory growth on an HTTP/2 connection — a remote DoS wherever the affected `h2` terminates
untrusted connections.

**This advisory matched two instances, and they were resolved differently.**

`h2@0.4.15` was in the **default build**, via `hyper 1.10 ← axum ← fraiseql-server`. That is the
GraphQL listener itself: `axum::serve` (`server/lifecycle.rs:883`) serves HTTP/2 by prior
knowledge, so the defect was reachable by any client that could open a connection. It was
**fixed, not accepted** — `cargo update -p h2@0.4.15 --precise 0.4.16`.

`h2@0.3.27` is the acceptance recorded above. It is reached only through
`aws-smithy-http-client 1.3.0`, which pins hyper 0.14, which pins the 0.3 series — where no
fixed release exists (the advisory's remedy is `>= 0.4.16`, and `--precise 0.3.28` resolves to
"no matching package named `h2`"). Exposure is `feature-gated:aws-s3`, established by
construction rather than asserted: `cargo tree -i h2@0.3.27 -e normal` reports "did not match
any packages", while the same query under `--features fraiseql-server/aws-s3` resolves it.

**Why the scoping matters**: an unscoped `{id = …}` ignore would have silenced the default-build
path too, including a future regression that reintroduces a vulnerable `h2` under hyper.
`deny.toml` therefore pins the acceptance to `h2@0.3.27`. `.cargo/audit.toml` cannot express
that — cargo-audit ignores by advisory id alone — so `cargo deny` is the authoritative gate for
this row.

**Residual risk**: an operator who opts into `aws-s3` or `cdc-kinesis` runs an HTTP/2 client
against AWS endpoints. Triggering the defect requires that endpoint to be hostile or
impersonated; it is not reachable by a FraiseQL client.

**Blocked on**: the aws-* stack moving off hyper 0.14 — the same migration that holds
RUSTSEC-2026-0098/-0099/-0104 (#1111).

**Review action by 2026-12-01**: re-check whether `aws-smithy-http-client` has reached hyper 1.x.

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
