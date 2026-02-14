# Migration Guide: From Individual Crates to Root Crate

This guide helps users migrate from importing individual `fraiseql-*` crates to using the unified `fraiseql` crate.

## Why Migrate?

The root `fraiseql` crate provides:

- Simplified imports (`use fraiseql::prelude::*`)
- Centralized feature flags
- Unified documentation
- Easier version management

## Migration Examples

### Basic Server

**Before:**

```rust
use fraiseql_core::{CompiledSchema, FraiseQLConfig};
use fraiseql_server::{Server, ServerConfig};
use std::sync::Arc;

let schema = CompiledSchema::from_file("schema.compiled.json")?;
let config = ServerConfig::from_file("fraiseql.toml")?;
let db = Arc::new(fraiseql_core::db::PostgresAdapter::new(&url).await?);
let server = Server::new(config, schema, db, None).await?;
```

**After:**

```rust
use fraiseql::prelude::*;
use std::sync::Arc;

let schema = CompiledSchema::from_file("schema.compiled.json")?;
let config = ServerConfig::from_file("fraiseql.toml")?;
let db = Arc::new(db::PostgresAdapter::new(&url).await?);
let server = Server::new(config, schema, db, None).await?;
```

### With Observers

**Before:**

```rust
use fraiseql_observers::{ObserverExecutor, EntityEvent};
use fraiseql_server::Server;
```

**After:**

```rust
use fraiseql::prelude::*;
// Or explicitly:
use fraiseql::observers::{ObserverExecutor, EntityEvent};
use fraiseql::server::Server;
```

## Feature Flag Equivalence

| Individual Crates | Root Crate Feature |
|-------------------|-------------------|
| `fraiseql-core[postgres]` | `fraiseql[postgres]` (default) |
| `fraiseql-server` | `fraiseql[server]` |
| `fraiseql-observers` | `fraiseql[observers]` |
| `fraiseql-arrow` | `fraiseql[arrow]` |
| `fraiseql-wire` | `fraiseql[wire]` |
| All of the above | `fraiseql[full]` |

## Dependency Migration

### Cargo.toml Changes

**Before:**

```toml
[dependencies]
fraiseql-core = "2.0.0-alpha.5"
fraiseql-server = "2.0.0-alpha.5"
fraiseql-observers = { version = "2.0.0-alpha.5", optional = true }
```

**After:**

```toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["server", "observers"] }
```

## Backward Compatibility

Individual `fraiseql-*` crates remain fully supported. You can:

- Continue using individual crates
- Mix root crate with individual crates
- Gradually migrate module by module

The root crate is a convenience layer with no new functionality.

## Feature Combinations

### Minimal Setup (Core Only)

```rust
// Cargo.toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["minimal"] }
```

### Development Setup (Server + Observers)

```rust
// Cargo.toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["server", "observers"] }
```

### Production Setup (Full Features)

```rust
// Cargo.toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["full"] }
```

## Troubleshooting

### Compile Error: "module X is private"

If migrating from individual crates, you may have used private modules. The root crate re-exports only public APIs.

**Solution:** Check that the module is listed in the public API:

```rust
// This won't work (private internal module)
use fraiseql_core::compiler::internal;

// Use the public API instead
use fraiseql::compiler::translate;
```

### Feature Not Available

Ensure the required feature is enabled in Cargo.toml:

```toml
# Add the feature
fraiseql = { version = "2.0.0-alpha.5", features = ["server"] }
```

### Version Mismatch

All sub-crates are versioned together. Using different versions of `fraiseql` and `fraiseql-*` can cause issues:

```toml
# ✅ Good
fraiseql = "2.0.0-alpha.5"

# ❌ Bad (avoid)
fraiseql = "2.0.0-alpha.5"
fraiseql-core = "2.0.0-alpha.4"
```

## Performance Impact

The root crate uses zero-cost abstractions (re-exports only). There is **no performance penalty** compared to using individual crates directly.
