# Batch 5 — Crate Architecture: fraiseql-core Split

## Problem

`fraiseql-core` contains 209,277 lines of Rust in a single crate. This has
two concrete consequences:

1. **Incremental compilation is ineffective.** Any change to `db/mysql/adapter.rs`
   forces recompilation of `cache/`, `graphql/`, `compiler/`, `security/`, etc.
   — all unrelated modules. On a developer machine, a one-line change in the
   database layer may trigger 30–60 second rebuilds.

2. **Dependency graph is opaque.** It is impossible to depend on just the database
   abstraction without also pulling in all of GraphQL execution, caching, schema
   compilation, and security utilities. External consumers of the crate cannot
   take a minimal dependency.

## Dependency Analysis

Current module structure of `fraiseql-core/src/`:

```
apq/             ← depends on: cache, graphql, security
audit/           ← depends on: db, security
cache/           ← depends on: db, schema, graphql
compiler/        ← depends on: schema, db
config/          ← standalone
db/              ← depends on: schema, utils, types  ← SPLIT TARGET
design/          ← depends on: schema
error.rs         ← standalone
federation/      ← depends on: db, schema, graphql, cache
filters/         ← depends on: schema, db
graphql/         ← depends on: schema
observability/   ← standalone
runtime/         ← depends on: db, graphql, cache, schema, security
schema/          ← depends on: types, config  ← SPLIT TARGET (executor dep)
security/        ← depends on: db, schema
tenancy/         ← depends on: db, schema
types/           ← standalone
utils/           ← standalone
validation/      ← depends on: schema, types
```

## Proposed Split

### Step 1 — Extract `fraiseql-db`

Move `crates/fraiseql-core/src/db/` to `crates/fraiseql-db/src/`.

**Contents**:
- `traits.rs` — `DatabaseAdapter`, `MutationCapable`, `RelayDatabaseAdapter`
- `postgres/` — full PostgreSQL adapter
- `mysql/` — full MySQL adapter
- `sqlite/` — read-only SQLite adapter
- `sqlserver/` — SQL Server adapter
- Supporting: `collation.rs`, `identifier.rs`, `path_escape.rs`,
  `projection_generator.rs`, `types.rs`, `where_clause.rs`, `where_sql_generator.rs`,
  `wire_pool.rs`, `fraiseql_wire_adapter.rs`

**Dependencies** of `fraiseql-db`:
- `fraiseql-error` (errors)
- `fraiseql-wire` (wire backend, optional feature)
- Standard database drivers (tokio-postgres, sqlx, tiberius)
- No dependency on `fraiseql-core`

**`fraiseql-core` after step 1**:
- Adds `fraiseql-db` as a dependency
- Re-exports all public items from `fraiseql-db` via `pub use fraiseql_db::*;`
  in `crates/fraiseql-core/src/db/mod.rs` (kept as a thin re-export shim)
- All downstream crates that import from `fraiseql-core::db` continue to work
  without changes (backwards compatible)

### Step 2 — Extract `fraiseql-executor`

Move `crates/fraiseql-core/src/runtime/executor/` to `crates/fraiseql-executor/src/`.

**Contents**:
- The GraphQL executor — the component that translates a parsed GraphQL
  operation into `fraiseql-db` calls and assembles the response
- APQ cache integration
- Field-level auth enforcement

**Dependencies** of `fraiseql-executor`:
- `fraiseql-db` (database calls)
- `fraiseql-core` (schema types, security context, graphql parsing)

Note: `fraiseql-executor` depends on `fraiseql-core` for schema types.
This is a one-way dependency (no cycle). The executor uses schema types
but does not own them.

### Step 3 — Update workspace

```toml
# Cargo.toml workspace members, add:
"crates/fraiseql-db",
"crates/fraiseql-executor",

# fraiseql-core/Cargo.toml:
[dependencies]
fraiseql-db = { path = "../fraiseql-db" }

# fraiseql-server/Cargo.toml:
# No change needed — it depends on fraiseql-core which re-exports everything.

# Optional: add fraiseql-db as a direct dependency where only DB
# functionality is needed, bypassing the full core crate.
```

### Step 4 — Add crate size enforcement

See infrastructure document `crate-size-policy.md`.

---

## Migration Steps

1. Create `crates/fraiseql-db/` with `Cargo.toml` and `src/lib.rs`
2. Move (not copy) all files from `fraiseql-core/src/db/` to `fraiseql-db/src/`
3. Add `fraiseql-db` to workspace and to `fraiseql-core`'s dependencies
4. Replace `fraiseql-core/src/db/mod.rs` with a thin re-export:
   ```rust
   // Backwards-compatible re-export. Direct imports from fraiseql-db
   // are preferred for new code.
   pub use fraiseql_db::*;
   ```
5. Run `cargo check --workspace` — fix any import path breakage
6. Run `cargo test --workspace` — no test should fail
7. Repeat for `fraiseql-executor`

---

## Expected Impact

| Metric | Before | After (estimate) |
|--------|--------|-----------------|
| `fraiseql-core` lines | 209K | ~140K (db removed) |
| `fraiseql-db` lines | — | ~30K |
| `fraiseql-executor` lines | — | ~25K |
| Rebuild after `db/mysql/adapter.rs` change | Full core rebuild | Only `fraiseql-db` + dependents |
| Rebuild after `graphql/` change | Full core rebuild | Only `fraiseql-core` + dependents |

---

## Verification Checklist

- [ ] `cargo check --workspace` passes after each step
- [ ] `cargo test --workspace` passes after both steps
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `wc -l crates/fraiseql-core/src/**/*.rs | tail -1` shows < 150K lines
- [ ] `crates/fraiseql-db/` has its own `README.md` explaining its scope
- [ ] Snapshot tests (`cargo test --test sql_snapshots`) still pass
