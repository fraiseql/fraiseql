# FraiseQL v2.0.0-alpha.3 — Merged Execution Plan

## Overview

12 phases combining WHERE operator implementation (Phases 2-5), performance optimization (Phase 6), Rust features (Phases 7-10), and finalization (Phases 11-12).
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
| 2 | Network Operators | IsIPv4, IsIPv6, IsPrivate, InSubnet, etc. | 2-3 days | ✅ COMPLETE |
| 3 | LTree Operators | AncestorOf, DescendantOf, MatchesLquery, etc. | 2-3 days | ✅ COMPLETE |
| 4 | Array & FTS Operators | LenEq, LenGt, Matches, PlainQuery, PhraseQuery | 3-4 days | ✅ COMPLETE |
| 5 | Extended/Rich Operators | Email, Country, Coordinates, VIN, IBAN, etc. (44 types) | 3-5 days | ✅ COMPLETE |

### Performance Optimization (fraiseql-core)
| Phase | Title | Scope | Effort | Status |
|-------|-------|-------|--------|--------|
| 6 | Direct Column Optimization | Use SQL columns instead of JSONB when available | 2-3 days | ✅ COMPLETE |

### Rust Features (Various Crates)
| Phase | Title | Scope | Crates | Effort | Status |
|-------|-------|-------|--------|--------|--------|
| 7 | TOML Schema Merger | Fix types/fields array conversion | fraiseql-cli | 2-3 days | Pending |
| 8 | Arrow Subscription Filters | Implement expression-based filter evaluation | fraiseql-arrow | 2-3 days | Pending |
| 9 | JSON to Arrow Conversion | Convert historical events to Arrow format | fraiseql-arrow | 2-3 days | Pending |
| 10 | Server Testing Mocks | Mock implementations for integration tests | fraiseql-server | 2-3 days | Pending |

### Cleanup & Verification
| Phase | Title | Scope | Effort | Status |
|-------|-------|-------|--------|--------|
| 11 | Python Operator Cleanup | Remove old Python operators, verify Rust complete | 1-2 days | Pending |
| 12 | Finalize | Security audit, QA review, documentation | 1-2 days | Last |

## Dependencies

```
Phase 0: Template Integration (COMPLETE)
    ↓
Phase 1: Quick Wins (COMPLETE)
    ↓
Phases 2-5: WHERE Operators (independent of each other)
    2 (network) ─┐
    3 (ltree)   ├─→ Phase 6 (Direct Column Optimization)
    4 (array)   │      ↓
    5 (rich)    ┘      Phase 11 (Python cleanup - depends on all above complete)
         ↓
         Phase 7+ (Rust Features - independent, can run in parallel with Phase 6)
         7 (TOML)   ─┐
         8 (Arrow)  ├─→ Phase 12 (Finalize - requires all above)
         9 (JSON)   │
         10 (testing)┘
```

**All phases 2-5 can run in parallel. Phase 6 depends on 2-5 complete. Phases 7-10 are independent and can run in parallel with Phase 6. Phases 11-12 are sequential at the end.**

## Skipped Phases

- **IP Auto-Detection Bug Fix** - Python, test infrastructure broken
- **Caching Layer Fix** - Python, deferred
- **Aggregation Path Validation** - Python, deferred

## Implementation Strategy

1. **Phases 2-5** implement WHERE operators using existing templates and database adapters (✅ COMPLETE)
2. **Phase 6** optimizes query performance by using direct columns instead of JSONB when available (✅ COMPLETE)
3. **Phases 7-10** implement independent Rust features in parallel (ready to start)
4. **Phase 11** removes Python operator code (now redundant with Rust implementation)
5. **Phase 12** finalizes, audits, documents

## Status

✅ Phase 6 Complete - Ready for Phase 7

Completed:
- Phases 0-1: Foundation ✅
- Phases 2-5: WHERE Operators (Network, LTree, Array/FTS, Extended) ✅
- Phase 6: Direct Column Optimization ✅

Next:
- Phases 7-10: Rust Features (TOML Merger, Arrow Filters, JSON→Arrow, Server Mocks) - Independent, can run in parallel
- Phase 11: Python Cleanup - Depends on 2-10 complete
- Phase 12: Finalize - Final phase
