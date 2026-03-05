# Vendored Dependencies

## graphql-parser

**Why vendored:** `graphql-parser 0.4.1` (2021, upstream unmaintained) depends on `thiserror 1.x`.
FraiseQL requires `thiserror 2.x`. The local patch in this directory upgrades the dependency.

**Patch applied:** `Cargo.toml` updated to use `thiserror = "2"` instead of `thiserror = "1"`.

**Limitation:** `cargo audit` does not scan vendored crates. Security patches from upstream
graphql-rust/graphql-parser will not be automatically applied to this directory.

## Maintenance Protocol

1. Check https://github.com/graphql-rust/graphql-parser/releases quarterly for upstream updates
2. Check https://rustsec.org/advisories/ for any `graphql-parser` security advisories
3. Apply upstream patches manually by comparing against `Cargo.toml.orig` and the `src/` directory

## Exit Plan

**Target:** Migrate to Option A (published fork on crates.io) before end of 2026.

The exit plan is to:
1. Fork `graphql-parser` to `github.com/fraiseql/graphql-parser`
2. Apply the `thiserror 2.x` patch (already done here)
3. Publish as `graphql-parser-fraiseql = "0.4.2"` on crates.io
4. Update `Cargo.toml` to use the published fork
5. Delete this `vendor/` directory

Tracking issue: https://github.com/fraiseql/fraiseql/issues — open an issue to track this.
