# FraiseQL v2.0.0-alpha.3 — Remaining Work

## Overview

6 phases (Rust-focused) covering operator templates, caching, Arrow integration, and testing.
Skipped Python-only phases (2, 3, 7) in favor of Rust ecosystem alignment.

## Phase Map

| Phase | Title | Scope | Crates | Status |
|-------|-------|-------|--------|--------|
| 0 | Template Integration | Wire sql_templates into SQL generation | fraiseql-core | ✅ COMPLETE |
| 1 | Quick Wins | Comment cleanup, simulated events | fraiseql-observers, fraiseql-arrow | ✅ COMPLETE |
| 2 | Caching Layer Fix | RustResponseBytes serialization | fraiseql-wire | In Progress |
| 3 | Arrow Subscription Filters | Filter matching on Arrow records | fraiseql-arrow | Pending |
| 4 | JSON to Arrow Conversion | Convert historical events to Arrow | fraiseql-arrow | Pending |
| 5 | Server Testing Mocks | Mock implementations for testing | fraiseql-server | Pending |
| 6 | Finalize | Security audit, docs, cleanup | workspace | Last |

## Dependencies

```
Phase 0 (foundation)
    ↓
Phase 1 (quick wins - independent)
    ↓
Phase 2 (caching - independent)
    ↓
Phase 3 (filters - independent, depends on Arrow basics)
    ↓
Phase 4 (JSON→Arrow - independent)
    ↓
Phase 5 (testing mocks - independent)
    ↓
Phase 6 (finalize - requires all above)
```

Phases 1-5 are independently completable. Phase 6 is final cleanup.

## Skipped Phases

- **Phase 2 (old)**: IP Auto-Detection Bug Fix - Python, test infra broken
- **Phase 3 (old)**: TOML Schema Merger - Python, lower priority
- **Phase 7 (old)**: Aggregation Path Validation - Python, lower priority

## Status

[~] In Progress (Phase 2 starting)
