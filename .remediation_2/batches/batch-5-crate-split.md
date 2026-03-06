# Batch 5 — Crate Architecture: fraiseql-core Split

## Problem

`fraiseql-core` contains 107,712 lines of Rust in a single crate. This has
two concrete consequences:

1. **Incremental compilation is ineffective.** Any change to `db/mysql/adapter.rs`
   forces recompilation of `cache/`, `graphql/`, `compiler/`, `security/`, etc.
   — all unrelated modules. On a developer machine, a one-line change in the
   database layer may trigger 30–60 second rebuilds.

2. **Dependency graph is opaque.** It is impossible to depend on just the database
   abstraction without also pulling in all of GraphQL execution, caching, schema
   compilation, and security utilities. External consumers of the crate cannot
   take a minimal dependency.

---

## Actual Dependency Analysis (2026-03-05 Audit)

Current module structure of `fraiseql-core/src/`:

```
apq/             ← depends on: cache, graphql, security
audit/           ← depends on: db, security
cache/           ← depends on: db, schema, graphql
compiler/        ← depends on: schema, db
config/          ← standalone
db/              ← SPLIT TARGET — but see coupling blockers below
design/          ← depends on: schema
error.rs         ← standalone
federation/      ← depends on: db, schema, graphql, cache
filters/         ← depends on: schema, db
graphql/         ← depends on: schema
observability/   ← standalone
runtime/         ← depends on: db, graphql, cache, schema, security
schema/          ← depends on: types, config
security/        ← depends on: db, schema
tenancy/         ← depends on: db, schema
types/           ← standalone
utils/           ← standalone
validation/      ← depends on: schema, types
```

### db/ Module Size

24 files, 16,164 lines across:
- 4 database adapters (PostgreSQL 2,172L, SQL Server 1,718L, MySQL 916L, SQLite 911L)
- Traits, WHERE clause types, projection generator, collation, identifier utilities

### runtime/executor/ Module Size

7 files, 3,396 lines — GraphQL operation execution, mutation, aggregate, federation.

---

## Coupling Blockers (CA-1 and CA-2)

### CA-1 Blocker: db/ → compiler + schema imports

The `db/` module is NOT cleanly isolated. It imports from outside its own
subtree:

| File | Imports from fraiseql-core |
|------|---------------------------|
| `db/traits.rs` | `compiler::aggregation::OrderByClause` (used in trait method signature) |
| `db/traits.rs` | `schema::SqlProjectionHint` (used in trait method signature) |
| `db/postgres/adapter.rs` | `compiler::aggregation::{OrderByClause, OrderDirection}` |
| `db/mysql/adapter.rs` | `compiler::aggregation::{OrderByClause, OrderDirection}` |
| `db/sqlserver/adapter.rs` | `compiler::aggregation::{OrderByClause, OrderDirection}` |
| `db/collation.rs` | `config::CollationConfig` |

**Usage breadth**:
- `OrderByClause`/`OrderDirection`: 16 files throughout runtime, compiler, cache
- `SqlProjectionHint`: 14 files throughout schema, runtime, cache, db
- `CollationConfig`: config module

**Why this blocks the split**: If `fraiseql-db` is created as a standalone crate,
it cannot import `OrderByClause` or `SqlProjectionHint` from `fraiseql-core`
because that would create a circular dependency (`fraiseql-core` → `fraiseql-db`
→ `fraiseql-core`).

**Required prerequisite**: Move `OrderByClause`, `OrderDirection`, `SqlProjectionHint`,
and `CollationConfig` to `fraiseql-db` (since they are fundamentally DB-query-construction
types) and update all 30+ import sites in the compiler, runtime, and cache modules.
This is a separate refactoring campaign.

### CA-2 Blocker: executor/ ↔ cache circular dependency

The `runtime/executor/` calls into `cache/adapter/` for result caching, and
`cache/adapter/` calls back into the executor (callback pattern). This circular
relationship prevents clean extraction of `fraiseql-executor` without first
introducing a trait abstraction between them.

---

## What Was Done (CA-4)

CA-4 was implemented independently as it has no coupling dependencies:

### `[workspace.metadata.crate-size-budget]` in `Cargo.toml`

Per-crate line-count budgets have been added to the workspace root `Cargo.toml`.
fraiseql-core is budgeted at 150,000 lines (split threshold). After the
CA-1 split, the budget should be lowered to ~70,000.

### `tools/check-crate-sizes.sh`

Enforcement script that:
- Parses budgets from `[workspace.metadata.crate-size-budget]`
- Counts `.rs` source lines for each budgeted crate
- Warns at 85% of budget, fails at 100%
- Prints a summary table with current/budget/status
- Exits 0 if all pass, 1 if any crate is over budget

Usage:
```bash
tools/check-crate-sizes.sh              # check all crates
tools/check-crate-sizes.sh fraiseql-core # check a single crate
```

Current check results (all pass):

| Crate | Lines | Budget | Status |
|-------|-------|--------|--------|
| fraiseql-core | 107,712 | 150,000 | ✅ OK |
| fraiseql-server | 34,010 | 55,000 | ✅ OK |
| fraiseql-observers | 27,504 | 45,000 | ✅ OK |
| fraiseql-cli | 26,070 | 40,000 | ✅ OK |
| fraiseql-auth | 15,761 | 25,000 | ✅ OK |
| fraiseql-secrets | 10,785 | 20,000 | ✅ OK |
| fraiseql-wire | 10,041 | 20,000 | ✅ OK |
| fraiseql-arrow | 8,076 | 15,000 | ✅ OK |
| fraiseql-webhooks | 2,139 | 10,000 | ✅ OK |
| fraiseql-test-utils | 1,248 | 5,000 | ✅ OK |
| fraiseql-error | 686 | 5,000 | ✅ OK |
| fraiseql-observers-macros | 192 | 2,000 | ✅ OK |

---

## Deferred Work (CA-1, CA-2, CA-3)

CA-1 and CA-2 require a prerequisite types refactoring campaign:

**Step 0 (prerequisite, not yet planned)**:
Move `OrderByClause`, `OrderDirection`, `SqlProjectionHint`, `CollationConfig`
to a standalone location (either to a new `fraiseql-types` crate, or define them
in `fraiseql-db` and update all import sites). Approximately 30–40 files to touch.

**CA-1 (deferred)**:
Once Step 0 is complete, move `crates/fraiseql-core/src/db/**` to `crates/fraiseql-db/`,
add re-export shim in `fraiseql-core/src/db/mod.rs`.

**CA-2 (deferred)**:
After CA-1, resolve the executor↔cache circular dependency via a trait abstraction,
then move `runtime/executor/**` to `crates/fraiseql-executor/`.

**CA-3 (deferred)**:
Update `fraiseql-core/Cargo.toml` and workspace as part of CA-1/CA-2.

---

## Verification Checklist

- [x] `tools/check-crate-sizes.sh` passes on current codebase
- [x] `[workspace.metadata.crate-size-budget]` added to `Cargo.toml`
- [x] `cargo check --workspace` passes after CA-1 split (2026-03-06)
- [x] `cargo test --workspace` passes after CA-1 split (fraiseql-db: 149, fraiseql-core: 2138, fraiseql-server: 623 unit tests)
- [x] `wc -l crates/fraiseql-core/src/**/*.rs | tail -1` shows 92,218 after split
- [ ] `crates/fraiseql-db/` has its own `README.md` explaining its scope
- [x] Snapshot tests (`cargo test --test sql_snapshots`) still pass
