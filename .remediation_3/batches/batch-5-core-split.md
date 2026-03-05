# Batch 5 — Core Split Prerequisite Work

## Problem

CA-1 and CA-2 from Campaign 2 are blocked by two concrete coupling points.
This batch resolves them, then executes the splits.

### CA-1 blocker — type locations

`db/traits.rs` imports `compiler::aggregation::OrderByClause` and
`schema::SqlProjectionHint`. These are compilation-layer types living in the
compiler and schema modules, imported by the database layer. To extract `db/`
into `fraiseql-db`, these types must move to a neutral location that both layers
can import without creating a cycle.

### CA-2 blocker — cache/adapter ↔ runtime/executor circular dependency

`cache/adapter/` directly imports concrete types from `runtime/executor/`.
`runtime/executor/` imports `cache/adapter/` for result caching. A clean split
requires the executor to implement a trait defined in the cache module, not the
cache importing the executor's concrete types.

---

## Fix Plan

### CS-1 — Relocate shared SQL types (unblocks CA-1)

**Move** `compiler::aggregation::OrderByClause` and
`schema::SqlProjectionHint` to a new file:

```
crates/fraiseql-core/src/types/sql_hints.rs
```

Update `crates/fraiseql-core/src/types/mod.rs` to export them:

```rust
pub mod sql_hints;
pub use sql_hints::{OrderByClause, SqlProjectionHint};
```

Update all import sites:
- `compiler/aggregation.rs`: `use crate::types::sql_hints::OrderByClause;`
- `schema/compiled.rs`: `use crate::types::sql_hints::SqlProjectionHint;`
- `db/traits.rs`: `use crate::types::sql_hints::{OrderByClause, SqlProjectionHint};`

**Verify**: No import in `db/` now reaches into `compiler/` or `schema/`. The
dependency graph becomes:

```
compiler/ → types/sql_hints
schema/   → types/sql_hints
db/       → types/sql_hints    (no longer → compiler/ or schema/)
```

### CS-2 — Introduce `ExecutorAdapter` trait (unblocks CA-2)

**Add** to `crates/fraiseql-core/src/cache/adapter/mod.rs`:

```rust
/// Implemented by the query executor to allow the cache adapter to drive
/// execution on cache miss, without importing executor concrete types.
pub trait ExecutorAdapter: Send + Sync {
    fn execute_query<'a>(
        &'a self,
        query: &'a str,
        params: &'a [QueryParam],
    ) -> impl Future<Output = Result<QueryResult>> + Send + 'a;
}
```

**Update** `crates/fraiseql-core/src/runtime/executor/mod.rs`:

```rust
impl ExecutorAdapter for Executor {
    fn execute_query<'a>(
        &'a self,
        query: &'a str,
        params: &'a [QueryParam],
    ) -> impl Future<Output = Result<QueryResult>> + Send + 'a {
        self.run_query(query, params)
    }
}
```

**Update** `cache/adapter/` to hold `Box<dyn ExecutorAdapter>` instead of a
concrete `Arc<Executor>`.

**Verify**: No import in `cache/` now reaches into `runtime/executor/`. The
dependency graph becomes:

```
cache/adapter/ → (defines) ExecutorAdapter trait
runtime/executor/ → cache/adapter/ (implements the trait)
```

### CS-3 — Extract `fraiseql-db` (resolves CA-1)

**Requires**: CS-1 complete.

1. Create `crates/fraiseql-db/` as a new workspace member.
2. Move `crates/fraiseql-core/src/db/` to `crates/fraiseql-db/src/`.
3. Update `crates/fraiseql-db/Cargo.toml` — dependencies include
   `fraiseql-error` and `fraiseql-core` (for `types/sql_hints` only, until
   `types/` is also split in a future campaign).
4. Update `crates/fraiseql-core/Cargo.toml` to depend on `fraiseql-db`:
   ```toml
   fraiseql-db = { path = "../fraiseql-db" }
   ```
5. Replace `use crate::db::` with `use fraiseql_db::` throughout
   `fraiseql-core/src/`.
6. Update `fraiseql-core/src/lib.rs`:
   ```rust
   // Re-export for crates that previously imported via fraiseql-core
   pub use fraiseql_db as db;
   ```
7. Add `fraiseql-db` to workspace `Cargo.toml` members.
8. Run `cargo check --workspace` and resolve any remaining import errors.

**Size impact**: `fraiseql-core` drops by ~10,000 lines; `fraiseql-db` starts
at ~10,000 lines — both well within their size budgets.

### CS-4 — Extract `fraiseql-executor` (resolves CA-2)

**Requires**: CS-2 complete.

1. Create `crates/fraiseql-executor/` as a new workspace member.
2. Move `crates/fraiseql-core/src/runtime/executor/` to
   `crates/fraiseql-executor/src/`.
3. Update `crates/fraiseql-executor/Cargo.toml` — depends on `fraiseql-core`
   for `cache/adapter/` (trait definition), `fraiseql-db` for query execution.
4. Update `crates/fraiseql-core/Cargo.toml`:
   ```toml
   fraiseql-executor = { path = "../fraiseql-executor" }
   ```
5. Replace `use crate::runtime::executor::` with `use fraiseql_executor::`.
6. Re-export from `fraiseql-core/src/lib.rs` for backward compatibility.
7. Add `fraiseql-executor` to workspace members.
8. Run `cargo check --workspace`.

---

## Post-split verification checklist

- [ ] `cargo check --workspace` passes with zero errors
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo nextest run --workspace` all tests pass
- [ ] `tools/check-crate-sizes.sh` passes (all crates within budget)
- [ ] `fraiseql-core` line count below 90,000 (down from 107,712)
- [ ] `fraiseql-db` line count below 15,000
- [ ] `fraiseql-executor` line count below 15,000
- [ ] No import in `fraiseql-db` reaches into `fraiseql-core` (check with `cargo depgraph`)
- [ ] Public API of `fraiseql-core` is unchanged (all types still re-exported)
- [ ] CA-1 and CA-2 in `.remediation_2/master-plan.md` updated to ✅

---

## Risk

This is the highest-risk batch. The recommended sequence to minimise risk:

1. CS-1: pure type relocation — should be mechanical, no behaviour change
2. CS-2: trait introduction — add `impl`, update one `Arc<Executor>` to `Box<dyn ExecutorAdapter>`; run tests after each file
3. CS-3: move `db/` directory — mechanical; `cargo check` after each file moved
4. CS-4: move `executor/` directory — same

If CI shows unexpected failures after CS-3 or CS-4, revert that step and
re-examine the remaining import graph before retrying.
