# Test Extraction Campaign — Full Workspace

## Goal

Eliminate all inline `#[cfg(test)] mod tests { … }` blocks from every `src/`
file in the workspace. Each module's tests live in a sibling `tests.rs` file,
declared via `#[cfg(test)] mod tests;` in the module root.

The pattern was established and enforced for `routes/rest/` in phases 1–10 of
the original campaign. This plan extends it to the remaining **521 files** across
**15 crates**.

## Current state (baseline)

| Crate | Files to clean | Notes |
|-------|---------------|-------|
| fraiseql-core | 120 | Largest — split into 8 sub-phases |
| fraiseql-server | 86 | Outside routes/rest/ only |
| fraiseql-observers | 64 | Split into 4 sub-phases |
| fraiseql-auth | 45 | Split into 3 sub-phases |
| fraiseql-cli | 47 | Split into 3 sub-phases |
| fraiseql-federation | 30 | One phase (all leaf files) |
| fraiseql-db | 28 | One phase |
| fraiseql-wire | 28 | One phase |
| fraiseql-secrets | 20 | One phase |
| fraiseql-functions | 16 | One phase |
| fraiseql-webhooks | 14 | One phase |
| fraiseql-arrow | 13 | One phase |
| fraiseql-test-utils | 9 | One phase |
| fraiseql-storage | 1 | Bundled with test-utils |
| fraiseql-error | 1 | Bundled with test-utils |
| **Total** | **522** | |

## Phase numbering

Phases are numbered `11` onwards (continuing from the original `01–10`
campaign):

```
11–18  fraiseql-core      (8 sub-phases by subsystem)
19–22  fraiseql-server    (4 sub-phases by subsystem)
23–26  fraiseql-observers (4 sub-phases by subsystem)
27–29  fraiseql-auth      (3 sub-phases by subsystem)
30–32  fraiseql-cli       (3 sub-phases by subsystem)
33     fraiseql-federation
34     fraiseql-db
35     fraiseql-wire
36     fraiseql-secrets
37     fraiseql-functions
38     fraiseql-webhooks
39     fraiseql-arrow
40     fraiseql-test-utils + fraiseql-storage + fraiseql-error
41     CI enforcement expansion (widen lint-tests-layout to all crates)
```

## Execution rules (same as original campaign)

1. Read the phase file, understand the files in scope.
2. For each file: create/update the `tests.rs` sibling, add `#[cfg(test)] mod tests;`
   declaration, remove the inline block from the source file.
3. Verify: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   and `cargo nextest run -p <crate> --lib`.
4. One commit per phase.
5. Stop on any red build.

## Commit message template

```
refactor(<crate>): extract <subsystem>/ inline tests to tests.rs
```

## Phase index

| Phase | File |
|-------|------|
| 11 | tests-extract-11-core-compiler.md |
| 12 | tests-extract-12-core-schema.md |
| 13 | tests-extract-13-core-cache.md |
| 14 | tests-extract-14-core-runtime.md |
| 15 | tests-extract-15-core-security.md |
| 16 | tests-extract-16-core-validation.md |
| 17 | tests-extract-17-core-graphql-apq-utils.md |
| 18 | tests-extract-18-core-design-config.md |
| 19 | tests-extract-19-server-middleware.md |
| 20 | tests-extract-20-server-routes.md |
| 21 | tests-extract-21-server-subscriptions-misc.md |
| 22 | tests-extract-22-server-tenancy-config.md |
| 23 | tests-extract-23-observers-tracing-logging.md |
| 24 | tests-extract-24-observers-transport-queue.md |
| 25 | tests-extract-25-observers-listener-checkpoint.md |
| 26 | tests-extract-26-observers-misc.md |
| 27 | tests-extract-27-auth-providers-oauth.md |
| 28 | tests-extract-28-auth-audit-mfa.md |
| 29 | tests-extract-29-auth-misc.md |
| 30 | tests-extract-30-cli-commands.md |
| 31 | tests-extract-31-cli-schema.md |
| 32 | tests-extract-32-cli-codegen-config.md |
| 33 | tests-extract-33-federation.md |
| 34 | tests-extract-34-db.md |
| 35 | tests-extract-35-wire.md |
| 36 | tests-extract-36-secrets.md |
| 37 | tests-extract-37-functions.md |
| 38 | tests-extract-38-webhooks.md |
| 39 | tests-extract-39-arrow.md |
| 40 | tests-extract-40-test-utils-storage-error.md |
| 41 | tests-extract-41-ci-enforcement-full.md |
