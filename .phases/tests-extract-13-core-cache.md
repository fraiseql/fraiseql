---
title: Test Extraction — fraiseql-core cache/
status: planned
---

# Phase 13: `fraiseql-core` — `cache/`

## Objective

Extract inline tests from the `cache/` subsystem of `fraiseql-core`.

## Files

| File | Notes |
|------|-------|
| `cache/result.rs` | ~877 test lines — second-largest block in codebase |
| `cache/mod.rs` | Cache module root |
| `cache/key.rs` | Cache key derivation |
| `cache/invalidation.rs` | Invalidation logic |
| `cache/metrics.rs` | Cache metrics |
| `cache/cascade.rs` | Cascade invalidator |
| `cache/ttl.rs` | TTL management |
| `cache/apq.rs` | APQ cache layer |
| `cache/policy.rs` | Eviction policy |
| `cache/shard.rs` | Sharded LRU implementation |
| `cache/warming.rs` | Cache warming |
| `cache/compression.rs` | Value compression |
| `cache/serialization.rs` | Serialization helpers |

> `cache/adapter/tests.rs` already exists — skip that subdirectory.

## Steps

For `cache/result.rs` (the large one): it is a leaf file, so tests go into
a new `cache/tests.rs` file that consolidates tests from all leaf files in
this directory, importing via `use super::result::…`, `use super::key::…` etc.

Alternatively, if individual files are large enough, each gets its own
`cache/<name>/tests.rs` only if the file is itself a module directory.
For leaf `.rs` files, consolidate into the parent `cache/tests.rs`.

## Commit

```
refactor(core): extract cache/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-core --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-core --lib
```
