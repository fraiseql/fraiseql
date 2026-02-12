# FraiseQL v2.0.0-alpha.3 — Merged Execution Plan

## Overview

11 phases combining WHERE operator implementation (Phases 2-5), Rust features (Phases 6-9), and finalization.
Complete Rust runtime with polyglot schema authoring (any language → IntermediateSchema).
Python is optional for schema authoring only; not part of runtime.

## Architecture

```
Schema Authoring (Polyglot)
  - Python, JavaScript, Go, Java, GraphQL SDL, SQL DDL, etc.
  ↓
IntermediateSchema (language-agnostic)
  ↓
Rust Compiler
  ↓
GraphQL + Types
  ↓
TOML Configuration (database, features, caching, etc.)
  ↓
User Queries (GraphQL)
  ↓
where_sql_generator.rs (Rust Runtime, 100%)
  ↓
Database SQL
```

## Phase Map

### Core Foundation
| Phase | Title | Scope | Crates | Status |
|-------|-------|-------|--------|--------|
| 0 | Template Integration | Wire sql_templates into WhereSqlGenerator | fraiseql-core | ✅ COMPLETE |
| 1 | Quick Wins | Comment cleanup, simulated events | fraiseql-observers, fraiseql-arrow | ✅ COMPLETE |

### WHERE Operator Implementation (fraiseql-core)
| Phase | Title | Operators | Effort | Status |
|-------|-------|-----------|--------|--------|
| 2 | Network Operators | IsIPv4, IsIPv6, IsPrivate, InSubnet, etc. | 2-3 days | Pending |
| 3 | LTree Operators | AncestorOf, DescendantOf, MatchesLquery, etc. | 2-3 days | Pending |
| 4 | Array & FTS Operators | LenEq, LenGt, Matches, PlainQuery, PhraseQuery | 3-4 days | Pending |
| 5 | Extended/Rich Operators | Email, Country, Coordinates, VIN, IBAN, etc. (44 types) | 3-5 days | Pending |

### Rust Features (Various Crates)
| Phase | Title | Scope | Crates | Effort | Status |
|-------|-------|-------|--------|--------|--------|
| 6 | TOML Schema Merger | Fix types/fields array conversion | fraiseql-cli | 2-3 days | Pending |
| 7 | Arrow Subscription Filters | Implement expression-based filter evaluation | fraiseql-arrow | 2-3 days | Pending |
| 8 | JSON to Arrow Conversion | Convert historical events to Arrow format | fraiseql-arrow | 2-3 days | Pending |
| 9 | Server Testing Mocks | Mock implementations for integration tests | fraiseql-server | 2-3 days | Pending |

### Cleanup & Verification
| Phase | Title | Scope | Effort | Status |
|-------|-------|-------|--------|--------|
| 10 | Python Operator Cleanup | Remove old Python operators, verify Rust complete | 1-2 days | Pending |
| 11 | Finalize | Security audit, QA review, documentation | 1-2 days | Last |

## Dependencies

```
Phase 0: Template Integration (COMPLETE)
    ↓
Phase 1: Quick Wins (COMPLETE)
    ↓
Phases 2-5: WHERE Operators (independent of each other)
    2 (network) ─┐
    3 (ltree)   ├─→ Phase 10 (Python cleanup - depends on all complete)
    4 (array)   │
    5 (rich)    ┘
    ↓
Phases 6-9: Rust Features (completely independent)
    6 (TOML)   ─┐
    7 (Arrow)  ├─→ Phase 11 (Finalize - requires all above)
    8 (JSON)   │
    9 (testing)┘
```

**All phases 2-9 can run in parallel.** Phase 10 and 11 are sequential.

## Skipped Phases

- **IP Auto-Detection Bug Fix** - Python, test infrastructure broken
- **Caching Layer Fix** - Python, deferred
- **Aggregation Path Validation** - Python, deferred

## Implementation Strategy

1. **Phases 2-5** implement WHERE operators using existing templates and database adapters
2. **Phases 6-9** implement independent Rust features in parallel
3. **Phase 10** removes Python operator code (now redundant with Rust implementation)
4. **Phase 11** finalizes, audits, documents

## Status

[~] In Progress (Phase 2 starting)
