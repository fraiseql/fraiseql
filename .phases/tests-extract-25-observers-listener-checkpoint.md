---
title: Test Extraction — fraiseql-observers listener/, checkpoint/, dedup/, resilience/
status: planned
---

# Phase 25: `fraiseql-observers` — `listener/`, `checkpoint/`, `dedup/`, `resilience/`

## Objective

Extract inline tests from four mid-sized subsystems of `fraiseql-observers`.

## Files

### listener/ (6 files)

Event listener implementations (database change, webhook, realtime).

→ `listener/tests.rs`

### checkpoint/ (2 files)

Checkpointing for at-least-once delivery guarantees.

→ `checkpoint/tests.rs`

### dedup/ (2 files)

Deduplication logic for event processing.

→ `dedup/tests.rs`

### resilience/ (4 files)

Retry, circuit breaker, and backpressure logic for observer reliability.

→ `resilience/tests.rs`

### search/ (2 files)

Search sink implementations (Elasticsearch, etc.).

→ `search/tests.rs`

### executor/ residual (2 files)

> `executor/tests.rs` already exists — merge residual inline blocks.

### cache/ (2 files)

Observer-level caching.

→ `cache/tests.rs`

## Steps

For each group, apply the standard pattern:
1. Create `tests.rs` with merged content.
2. Add `#[cfg(test)] mod tests;` in module root.
3. Remove inline blocks.

## Commit

```
refactor(observers): extract listener/, checkpoint/, dedup/, resilience/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-observers --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-observers --lib
```
