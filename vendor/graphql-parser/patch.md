# Patch: graphql-parser vendor override

**Upstream**: https://github.com/graphql-rust/graphql-parser
**Upstream version patched**: 0.4.1
**Vendored in commit**: 6f6e69125 (`chore(vendor): track graphql-parser patch in vendor/`)
**Date**: 2026-02-XX
**Reason**: Upgrade `thiserror` from 1.x to 2.x to eliminate duplicate versions in the
workspace dependency tree. The upstream crate pins `thiserror = "1"`.

## Upstream PR

File a PR against https://github.com/graphql-rust/graphql-parser once the upstream
maintainers are active again, or link it here once opened.

## Unblock condition

When the upstream crate releases a version with `thiserror 2.x` support:

1. Remove the `[patch.crates-io]` stanza from the root `Cargo.toml`.
2. Delete this directory (`vendor/graphql-parser/`).
3. Run `cargo update graphql-parser` to pull the released version.
4. Verify `cargo clippy --workspace` passes.

A CI job (`vendor-drift.yml`) warns when the upstream crate version on crates.io
diverges from the version patched here.
