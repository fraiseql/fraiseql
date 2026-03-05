# Batch 6 — Deprecation Enforcement: `observers-full`

## Problem

`fraiseql-server/Cargo.toml` marks the `observers-full` feature as deprecated
with a target removal in v2.3.0, but does so silently — no compile-time
warning is emitted when a user enables the feature. Users will discover
the removal only when it breaks their build.

```toml
# Current state in Cargo.toml:
observers-full = ["observers-enterprise"]  # deprecated, remove in 2.3.0
```

This is documentation-only deprecation.

---

## DA-1 — Emit compile-time warning via `build.rs`

Cargo features cannot be deprecated with `#[deprecated]` (that attribute is
for Rust items, not Cargo features). The correct mechanism is a `build.rs`
that emits a `cargo::warning` when the feature is active.

**Create** `crates/fraiseql-server/build.rs`:

```rust
fn main() {
    // Emit a compile-time warning when the deprecated `observers-full` feature
    // is enabled. This feature will be removed in v2.3.0.
    // Migration: replace `observers-full` with `observers-enterprise`.
    #[cfg(feature = "observers-full")]
    {
        println!(
            "cargo::warning=\
            The `observers-full` feature is deprecated and will be removed in \
            fraiseql-server v2.3.0. \
            Migrate to `observers-enterprise` (identical functionality). \
            See docs/migrations/observers-full-removal.md for details."
        );
    }
}
```

Note: `build.rs` does not support `#[cfg(feature = ...)]` directly — the
feature check must use an environment variable:

```rust
fn main() {
    if std::env::var("CARGO_FEATURE_OBSERVERS_FULL").is_ok() {
        println!(
            "cargo::warning=\
            `observers-full` is deprecated (remove in v2.3.0). \
            Use `observers-enterprise` instead. \
            See docs/migrations/observers-full-removal.md"
        );
    }
}
```

This will produce a visible `warning: ...` line in any build that includes
`observers-full`, including CI builds.

---

## DA-2 — Create migration guide

**Create** `docs/migrations/observers-full-removal.md`:

```markdown
# Migration: `observers-full` → `observers-enterprise`

## Timeline

- **v2.0.0**: `observers-full` deprecated, aliased to `observers-enterprise`
- **v2.3.0**: `observers-full` removed

## What Changed

`observers-full` was an early name for the full observer feature set including
NATS integration and enterprise actions. It has been renamed to
`observers-enterprise` for clarity.

The two features are functionally identical:
\`\`\`
observers-full        == observers-enterprise + fraiseql-observers/nats
\`\`\`

## Migration

In your `Cargo.toml`, replace:

\`\`\`toml
fraiseql-server = { ..., features = ["observers-full"] }
\`\`\`

with:

\`\`\`toml
fraiseql-server = { ..., features = ["observers-enterprise"] }
\`\`\`

One-liner for the common case:
\`\`\`bash
sed -i 's/"observers-full"/"observers-enterprise"/g' Cargo.toml
\`\`\`

## Questions

Open an issue at https://github.com/fraiseql/fraiseql/issues
```

---

## Verification Checklist

- [ ] `cargo build -p fraiseql-server --features observers-full 2>&1 | grep "deprecated"`
      produces the deprecation warning
- [ ] `cargo build -p fraiseql-server --features observers-enterprise 2>&1 | grep "deprecated"`
      produces no deprecation warning
- [ ] `docs/migrations/observers-full-removal.md` exists with accurate content
- [ ] Deprecation warning message includes the removal version and the migration target
