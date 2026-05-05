---
title: Test Extraction — fraiseql-federation
status: planned
---

# Phase 33: `fraiseql-federation`

## Objective

Extract inline tests from all 30 files in `fraiseql-federation`.

## Files

All 30 files are leaf modules at the top level of `src/` or in `saga_*/`
subdirectories:

| File |
|------|
| `composition_validator.rs` |
| `connection_manager.rs` |
| `database_resolver.rs` |
| `dependency_graph.rs` |
| `direct_db_resolver.rs` |
| `entity_resolver.rs` |
| `health.rs` |
| `http_resolver.rs` |
| `logging.rs` |
| `metadata_helpers.rs` |
| `mutation_detector.rs` |
| `mutation_executor.rs` |
| `mutation_http_client.rs` |
| `mutation_query_builder.rs` |
| `observability.rs` |
| `query_builder.rs` |
| `query_plan_cache.rs` |
| `representation.rs` |
| `requires_provides_validator.rs` |
| `saga_compensator.rs` |
| `saga_coordinator.rs` |
| `saga_recovery_manager.rs` |
| `saga_store.rs` |
| `selection_parser.rs` |
| `service_sdl.rs` |
| `sql_utils.rs` |
| `subscription_forwarder.rs` |
| `tls.rs` |
| `tracing.rs` |
| `types.rs` |

> `saga_executor/tests.rs` already exists — skip that subdirectory.

## Steps

Given all 30 files are top-level leaf files, consolidate into one
`src/tests.rs`. Group by concern in the file:

```
// --- composition ---
use super::composition_validator::…;

// --- query execution ---
use super::query_builder::…;
use super::query_plan_cache::…;

// --- saga ---
use super::saga_compensator::…;
// etc.
```

Add `#[cfg(test)] mod tests;` in `lib.rs`.

## Visibility watch-list

Many `fraiseql-federation` types are `pub` (cross-crate). Private helpers
tested directly may need `pub(crate)` promotion. Check clippy for
dead-code warnings after extraction.

## Commit

```
refactor(federation): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-federation --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-federation --lib
```
