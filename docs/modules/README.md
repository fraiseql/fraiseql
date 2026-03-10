# Module Orientation Guides

This directory contains orientation documents for the three most complex modules in
`fraiseql-core`. Read these **before** opening the source files — they explain the
algorithm, data flow, and design decisions that would otherwise require hours of
code archaeology.

## Modules

| Module | Source | Lines | Read when... |
|--------|--------|-------|-------------|
| [cache.md](cache.md) | `cache/adapter.rs`, `result.rs`, `key.rs` | ~3,600 | Touching query result caching, invalidation, or TTL |
| [window-functions.md](window-functions.md) | `compiler/window_functions.rs` | ~1,926 | Adding window functions, changing SQL generation, or debugging dialect differences |
| [fact-table.md](fact-table.md) | `compiler/fact_table.rs` | ~1,771 | Touching analytics queries, fact table introspection, or JSONB dimension extraction |

## Quick Navigation

**I need to add a new window function** → read [window-functions.md](window-functions.md) §"Adding a New Window Function"

**I need to debug a cache key collision** → read [cache.md](cache.md) §"Cache Key Construction"

**Cached data is stale after a mutation** → read [cache.md](cache.md) §"Cascade Invalidation"

**Fact table dimensions are missing** → read [fact-table.md](fact-table.md) §"JSONB Dimension Extraction"

**Fact table aggregation results are stale** → read [fact-table.md](fact-table.md) §"Aggregation Result Caching"

**Window function fails on MySQL** → read [window-functions.md](window-functions.md) §"Database Dialect Differences"
