---
title: Test Extraction — fraiseql-arrow
status: planned
---

# Phase 39: `fraiseql-arrow`

## Objective

Extract inline tests from all 13 files in `fraiseql-arrow`.

## Files

All 13 files are leaf modules at the top level of `src/`:

| File |
|------|
| `schema_gen.rs` |
| `clickhouse_sink.rs` |
| `export.rs` |
| `event_schema.rs` |
| `ticket.rs` |
| `cache.rs` |
| `db_convert.rs` |
| `exchange_protocol.rs` |
| `convert.rs` |
| `event_storage.rs` |
| `schema.rs` |
| `subscription.rs` |
| `metadata.rs` |

> `flight_server/tests.rs` already exists — skip that subdirectory.

## Steps

All 13 files are top-level leaf files → consolidate into `src/tests.rs`.
Group by concern in the file:

```
// --- Schema + type conversion ---
use super::schema_gen::…;
use super::schema::…;
use super::convert::…;
use super::db_convert::…;

// --- Event pipeline ---
use super::event_schema::…;
use super::event_storage::…;

// --- Export + streaming ---
use super::export::…;
use super::exchange_protocol.…;
use super::subscription::…;

// --- Storage + caching ---
use super::cache::…;
use super::metadata::…;

// --- Misc ---
use super::ticket::…;
use super::clickhouse_sink::…;
```

Add `#[cfg(test)] mod tests;` in `lib.rs`.

## Commit

```
refactor(arrow): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-arrow --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-arrow --lib
```
