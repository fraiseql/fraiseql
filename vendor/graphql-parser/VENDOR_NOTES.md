# Vendored graphql-parser

**Upstream**: <https://github.com/graphql-rust/graphql-parser>
**Vendored version**: 0.4.1 (see `Cargo.toml`)
**Patch reason**: Upgrade `thiserror` dependency from 1.x to 2.x to eliminate
duplicate `thiserror` versions in the workspace build.
**Upstream PR**: Pending — check the upstream repository for status.
**Check-in date**: 2026-03-10

## Update process

1. Check upstream for new releases: `cargo search graphql-parser`
2. Review the upstream `Cargo.toml` to see if `thiserror` 2.x is now a dependency.
   - **If yes**: remove this vendor patch from the workspace `Cargo.toml` and update
     the `graphql-parser` version to the new upstream release.
   - **If no**: apply the `thiserror` 2.x patch to the new upstream release and update
     this directory accordingly.
3. After updating, run `cargo build --workspace` and `cargo test --workspace` to verify.

## Security advisory monitoring

Subscribe to <https://github.com/graphql-rust/graphql-parser/releases> and check
advisories manually when updating other dependencies.

## What was patched

See `patch.md` in this directory for the exact diff applied to the upstream release.
