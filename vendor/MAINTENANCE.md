# Vendored Dependencies

## graphql-parser

**Why vendored:** `graphql-parser 0.4.1` (2021, upstream unmaintained) depends on `thiserror 1.x`.
FraiseQL requires `thiserror 2.x`. The local patch in this directory upgrades the dependency.

**Patch applied:** `Cargo.toml` updated to use `thiserror = "2"` instead of `thiserror = "1"`.

**Upstream PR status:** No upstream PR was opened — `graphql-rust/graphql-parser` has had no
commits since 2021 and is effectively abandoned. The `thiserror 2.x` upgrade cannot be upstreamed.

**Limitation:** `cargo audit` does not scan vendored crates. Security patches from upstream
graphql-rust/graphql-parser will not be automatically applied to this directory.

**Automated drift check:** `make security` runs `tools/check-vendor-security.sh`, which
compares the vendored version against the crates.io version and warns when they diverge.

## Maintenance Protocol

1. Check https://github.com/graphql-rust/graphql-parser/releases quarterly for upstream updates
2. Check https://rustsec.org/advisories/ for any `graphql-parser` security advisories
3. Apply upstream patches manually by comparing against `Cargo.toml.orig` and the `src/` directory
4. Run `make security` to verify the vendor check passes before each release

**To apply an upstream CVE fix manually:**

```bash
# Fetch upstream changes
git fetch https://github.com/graphql-rust/graphql-parser main
# Compare vendor/graphql-parser/src with upstream
git diff FETCH_HEAD -- src/
# Cherry-pick the security fix into vendor/graphql-parser/src/
```

## Exit Plan

**Target:** Migrate to a published fork on crates.io before end of 2026.

The exit plan is to:
1. Fork `graphql-parser` to `github.com/fraiseql/graphql-parser`
2. Apply the `thiserror 2.x` patch (already done here)
3. Publish as `graphql-parser-fraiseql = "0.4.2"` on crates.io
4. Update workspace `Cargo.toml` to use the published fork (remove `[patch.crates-io]` entry)
5. Delete this `vendor/` directory

Tracking: open a GitHub issue to track this migration.
