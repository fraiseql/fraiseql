# FraiseQL v2 Implementation Roadmap

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Planning Phase

---

## Executive Summary

This document outlines the implementation strategy for FraiseQL v2, a ground-up rewrite as a compiled GraphQL execution engine. Based on analysis of the v1 codebase (~24,104 lines of Rust), we can **reuse 60-70% of existing code** with varying degrees of adaptation.

**Total Effort**: 7-9 weeks for core Rust implementation (including analytics support)
**Lines of Code**: ~15,000-17,000 lines reused from v1
**Risk Level**: Low (leveraging battle-tested code)
**Analytics Support**: +7-9 days for Phase 1-2 fact table patterns, GROUP BY, aggregates (Phase 3-5 planned)

---

## Reusability Assessment Summary

### v1 Codebase Analysis

| Category | Modules | Lines | % | Strategy |
|----------|---------|-------|---|----------|
| **REUSE** (as-is) | 8 modules | ~15,000 | 62% | Direct copy + minor adaptation |
| **REFACTOR** (adapt) | 4 modules | ~7,000 | 29% | Extract utilities, adapt interfaces |
| **REWRITE** (new) | 2 modules | ~2,100 | 9% | New compiled query engine |

### Key Modules by Reusability

**100% Reusable (Direct Copy):**

- ✅ `schema/` - Compiled schema system (PERFECT alignment!)
- ✅ `apq/` - Automatic Persisted Queries
- ✅ `config/` - Configuration system
- ✅ `error.rs` - Error handling

**90-95% Reusable (Minor Changes):**

- ✅ `db/` - Database layer (update query execution interface)
- ✅ `security/` - Complete security layer
- ✅ `cache/` - Result caching (adapt cache keys)

**60-90% Reusable (Significant Adaptation):**

- 🔧 `query/` - Extract utilities (casing, operators, vector queries)
- 🔧 `graphql/` - Move parsing to compile-time
- 🔧 `http/` - Update query dispatch logic
- 🔧 `validation/` - Adapt to v2 schema

**Not Reusable (v1-Specific):**

- ❌ Runtime query builder (v2 uses compiled SQL)

---

## v2 Project Structure

```
fraiseql/
├── Cargo.toml                      # Workspace root
│
├── crates/
│   ├── fraiseql-core/              # Core execution engine (pure Rust)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── schema/             # ✅ REUSE from v1
│   │       │   ├── compiled.rs
│   │       │   ├── field_type.rs
│   │       │   └── tests.rs
│   │       ├── compiler/           # ❌ NEW for v2
│   │       │   ├── mod.rs
│   │       │   ├── parser.rs       # GraphQL schema → IR
│   │       │   ├── validator.rs    # Schema validation
│   │       │   ├── lowering.rs     # IR → SQL templates
│   │       │   └── codegen.rs      # Template generation
│   │       ├── runtime/            # ❌ NEW for v2
│   │       │   ├── mod.rs
│   │       │   ├── executor.rs     # Compiled query execution
│   │       │   ├── planner.rs      # Query plan selection
│   │       │   └── projection.rs   # Result projection
│   │       ├── db/                 # ✅ REUSE from v1 (95%)
│   │       │   ├── mod.rs
│   │       │   ├── pool.rs
│   │       │   ├── transaction.rs
│   │       │   ├── query.rs        # Update for compiled SQL
│   │       │   └── health.rs
│   │       ├── cache/              # ✅ REUSE from v1 (90%)
│   │       │   ├── mod.rs
│   │       │   ├── result.rs
│   │       │   └── coherency.rs    # Adapt cache keys
│   │       ├── security/           # ✅ REUSE from v1 (95%)
│   │       │   ├── auth.rs
│   │       │   ├── validator.rs
│   │       │   ├── masking.rs
│   │       │   └── audit.rs
│   │       ├── apq/                # ✅ REUSE from v1 (100%)
│   │       │   ├── hasher.rs
│   │       │   └── storage.rs
│   │       ├── config/             # ✅ REUSE from v1 (100%)
│   │       │   └── mod.rs
│   │       ├── error.rs            # ✅ REUSE from v1 (100%)
│   │       └── utils/              # 🔧 REFACTOR from v1
│   │           ├── casing.rs       # From query/
│   │           ├── operators.rs    # From query/
│   │           └── vector.rs       # From query/ + pipeline/
│   │
│   ├── fraiseql-server/            # HTTP server (Axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── routes/             # 🔧 REFACTOR from v1
│   │       │   ├── graphql.rs      # Update query dispatch
│   │       │   ├── health.rs       # Reuse from v1
│   │       │   └── introspection.rs
│   │       └── middleware/         # ✅ REUSE from v1
│   │           ├── auth.rs
│   │           ├── cors.rs
│   │           └── rate_limit.rs
│   │
│   ├── fraiseql-cli/               # CLI tool for schema compilation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/
│   │       │   ├── compile.rs      # Compile schema
│   │       │   ├── validate.rs     # Validate schema
│   │       │   └── serve.rs        # Dev server
│   │       └── error.rs
│   │
│   └── fraiseql-python/            # Python FFI (PyO3)
│       ├── Cargo.toml
│       ├── pyproject.toml
│       └── src/
│           ├── lib.rs              # PyO3 bindings
│           └── compiler.rs         # Python decorator → JSON
│
├── docs/                           # ✅ Already complete!
│
├── tests/
│   ├── integration/                # Integration tests
│   ├── e2e/                        # End-to-end tests
│   └── fixtures/                   # Test data
│
├── benches/                        # Performance benchmarks
│   ├── compilation.rs
│   ├── execution.rs
│   └── cache.rs
│
└── examples/                       # Example schemas
    ├── basic/
    ├── federation/
    └── enterprise/
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)

**Goal**: Establish core infrastructure with zero-cost v1 code reuse

**Tasks**:

1. ✅ Set up Cargo workspace
2. ✅ Copy v1 modules (direct reuse):
   - `schema/` → `fraiseql-core/src/schema/`
   - `error.rs` → `fraiseql-core/src/error.rs`
   - `config/` → `fraiseql-core/src/config/`
   - `apq/` → `fraiseql-core/src/apq/`
3. ✅ Update dependencies in `Cargo.toml`
4. ✅ Write integration tests for copied modules
5. ✅ Set up CI/CD (GitHub Actions)

**Deliverables**:

- Compiling workspace
- 4 modules with tests passing
- CI pipeline green

**Effort**: 2-3 days

---

### Phase 2: Database & Cache Infrastructure (Week 2-3)

**Goal**: Adapt database and caching layers for compiled queries

**Tasks**:

1. 🔧 Copy `db/` module from v1
2. 🔧 Update `db/query.rs`:
   - Change from `build_query()` to `execute_compiled_query()`
   - Add compiled SQL template execution
3. 🔧 Copy `cache/` module from v1
4. 🔧 Update cache key generation:
   - Use compiled query ID instead of runtime hash
   - Adapt to v2 signature format
5. ✅ Write integration tests for database + cache
6. ✅ Add connection pool benchmarks

**Deliverables**:

- Database layer executing compiled SQL
- Cache layer with v2-compatible keys
- Integration tests passing
- Performance benchmarks

**Effort**: 6 days

---

### Phase 3: Security Layer (Week 4)

**Goal**: Integrate complete security infrastructure

**Tasks**:

1. ✅ Copy `security/` module from v1:
   - `auth.rs` - JWT, Auth0, Clerk
   - `validator.rs` - Query depth, complexity
   - `masking.rs` - PII field masking
   - `audit.rs` - Audit logging
2. ✅ Copy `validation/` module from v1
3. 🔧 Minimal integration updates (if needed)
4. ✅ Write security integration tests
5. ✅ Add auth middleware benchmarks

**Deliverables**:

- Complete auth system
- Query validation
- Field masking
- Audit logging
- Security tests passing

**Effort**: 2 days

---

### Phase 4: Compiler Infrastructure (Week 4-5)

**Goal**: Build schema compiler (GraphQL decorators → CompiledSchema JSON)

**Tasks**:

1. ❌ Design compiler architecture:
   - Parse GraphQL schema (decorators, directives)
   - Build Authoring IR
   - Validate schema (types, bindings, auth rules)
   - Generate SQL templates for each query/mutation
   - Emit CompiledSchema JSON
2. ❌ Implement `compiler/parser.rs`:
   - 🔧 Reuse `graphql/parser.rs` from v1
   - Adapt for schema parsing (not query parsing)
   - Parse decorators: `@fraiseql.type`, `@fraiseql.query`, etc.
3. ❌ Implement `compiler/validator.rs`:
   - Schema validation rules
   - Type checking
   - Binding validation (types → database views)
4. ❌ Implement `compiler/lowering.rs`:
   - IR → SQL template generation
   - Database-specific lowering (PostgreSQL, MySQL, SQLite, SQL Server)
   - 🔧 Reuse operator logic from v1 `query/operators.rs`
5. ❌ Implement `compiler/codegen.rs`:
   - Generate CompiledSchema JSON
   - Optimize SQL templates
   - Emit capability manifest
6. ❌ **[Analytics]** Implement `compiler/fact_table.rs`:
   - Detect fact table structure (tf_* prefix)
   - Identify measure columns (numeric types: INT, DECIMAL, FLOAT)
   - Detect dimension JSONB column (default: `data`)
   - Map denormalized filter columns (indexed attributes)
7. ❌ **[Analytics]** Implement `compiler/aggregate_types.rs`:
   - Generate AggregateType (count, sum, avg, min, max per measure)
   - Generate GroupByInput (dimension paths + temporal buckets)
   - Generate HavingInput (aggregate filters)
   - Database-specific aggregate function selection from capability manifest
8. ❌ **[Analytics]** Implement `compiler/aggregation.rs`:
   - Generate GROUP BY execution plan
   - Generate aggregate function calls
   - Generate temporal bucketing expressions (DATE_TRUNC, DATE_FORMAT, strftime, DATEPART)
   - Lower conditional aggregates (FILTER vs CASE WHEN emulation)
9. ✅ Write compiler tests:
   - Unit tests for each phase
   - Integration tests (end-to-end compilation)
   - Golden file tests (known schemas)
   - **[Analytics]** Fact table introspection tests
   - **[Analytics]** Aggregate type generation tests

**Deliverables**:

- Working schema compiler
- SQL template generation
- CompiledSchema JSON output
- **[Analytics]** Fact table introspection
- **[Analytics]** Auto-generated aggregate types
- Compiler tests passing

**Effort**: 13-16 days (+3-4 days for analytics)

---

### Phase 5: Runtime Executor (Week 6-7)

**Goal**: Build compiled query executor

**Tasks**:

1. ❌ Design runtime architecture:
   - Load CompiledSchema at startup
   - Parse incoming GraphQL queries
   - Match query to compiled template
   - Execute SQL with variable substitution
   - Project results
2. ❌ Implement `runtime/executor.rs`:
   - Query pattern matching
   - Variable binding
   - SQL execution (via `db/` module)
   - Result projection
3. ❌ Implement `runtime/planner.rs`:
   - Query plan selection
   - Optimization hints
4. ❌ Implement `runtime/projection.rs`:
   - JSONB result → GraphQL response
   - 🔧 Reuse projection logic from v1 if applicable
5. ❌ **[Analytics]** Implement `runtime/aggregation.rs`:
   - Lower GROUP BY to database-specific SQL
   - Apply aggregate functions with database capability awareness
   - Apply HAVING filters post-aggregation
   - Handle NULL grouping keys
6. ❌ **[Analytics]** Implement `runtime/temporal.rs`:
   - DATE_TRUNC for PostgreSQL (second, minute, hour, day, week, month, quarter, year)
   - DATE_FORMAT for MySQL (day, week, month, year)
   - strftime for SQLite (day, week, month, year)
   - DATEPART for SQL Server (day, week, month, quarter, year, hour, minute)
7. ✅ Write runtime tests:
   - Unit tests for execution
   - Integration tests (query → response)
   - Performance benchmarks
   - **[Analytics]** Aggregation execution tests (GROUP BY, HAVING)
   - **[Analytics]** Temporal bucketing tests (all databases)

**Deliverables**:

- Working runtime executor
- Query pattern matching
- Result projection
- **[Analytics]** Aggregation executor
- **[Analytics]** Temporal bucketing support
- Execution tests passing
- Performance benchmarks

**Effort**: 13-17 days (+1-2 days for analytics)

---

### Phase 6: HTTP Server (Week 7-8)

**Goal**: Build HTTP server with Axum

**Tasks**:

1. 🔧 Copy `http/` module from v1
2. 🔧 Update `routes/graphql.rs`:
   - Replace resolver-based execution
   - Use v2 runtime executor
   - Keep: APQ, caching, auth middleware
3. ✅ Copy health check endpoints from v1
4. ✅ Add introspection endpoint
5. ✅ Write server integration tests
6. ✅ Add load testing benchmarks

**Deliverables**:

- HTTP server with GraphQL endpoint
- Health checks
- Introspection
- Server tests passing
- Load tests

**Effort**: 5 days

---

### Phase 7: Utilities & Vector Support (Week 8)

**Goal**: Extract and adapt v1 utilities

**Tasks**:

1. 🔧 Copy from v1 `query/`:
   - `casing.rs` → `utils/casing.rs` (direct copy)
   - `operators.rs` → `utils/operators.rs` (adapt for validation)
   - `vector.rs` → `utils/vector.rs` (adapt for pgvector)
2. 🔧 Copy from v1 `pipeline/`:
   - `vector.rs` → integrate into `utils/vector.rs`
3. ✅ Write utility tests
4. ✅ Add vector query benchmarks

**Deliverables**:

- Case conversion utilities
- Operator registry
- Vector query support
- Utility tests passing

**Effort**: 4-5 days

---

### Phase 8: Python Schema Authoring (Week 9)

**Goal**: Build Python decorator package (authoring-only, no runtime FFI)

**Architecture Note**: Python is ONLY for authoring schemas, NOT runtime execution.
Python decorators generate JSON schema files that are compiled by `fraiseql-cli`.
There is NO PyO3/FFI layer - the compiled Rust engine runs standalone.

**Tasks**:

1. 🔧 Copy `fraiseql-python/` decorator structure from v1
2. ❌ Implement decorator system (JSON output only):
   - `@fraiseql.type` → JSON schema
   - `@fraiseql.query` → JSON schema
   - `@fraiseql.mutation` → JSON schema
   - NO FFI bindings to Rust (compile-time only)
3. ❌ Implement schema file writer:
   - Output: `schema.json` (for fraiseql-cli to compile)
   - NO runtime Rust calls
4. ❌ **[Analytics]** Implement analytics decorators:
   - `@fraiseql.fact_table(measures=[...], dimensions=[...])` → Mark type as fact table
   - `@fraiseql.aggregate_query()` → Auto-generate aggregate query types
   - Validation: Ensure measures are numeric types, dimensions reference JSONB paths
5. ✅ Write Python tests (decorator → JSON validation)
6. ✅ **[Analytics]** Write fact table decorator tests
7. ✅ Build wheel packaging

**Deliverables**:

- Python package with decorators (authoring-only)
- JSON schema output (consumed by fraiseql-cli)
- **[Analytics]** Fact table and aggregate query decorators
- Python tests passing
- Pip-installable wheel

**Effort**: 4-5 days (+1 day for analytics)

---

### Phase 9: CLI Tool (Week 9-10)

**Goal**: Build CLI for schema compilation and dev server

**Architecture Note**: The CLI is the bridge between authoring (Python/TS → JSON)
and runtime (CompiledSchema → Execution). It compiles JSON schemas into optimized
Rust-native CompiledSchema files.

**Tasks**:

1. ❌ Implement `cli/commands/compile.rs`:
   - Read `schema.json` (from Python/TS decorators)
   - Parse and validate schema structure
   - Generate `schema.compiled.json` (CompiledSchema format)
   - Optimize SQL templates
2. ❌ Implement `cli/commands/validate.rs`:
   - Validate schema without compilation
   - Report errors with line numbers
3. ❌ Implement `cli/commands/serve.rs`:
   - Development server (wraps fraiseql-server)
   - Watch `schema.json` for changes
   - Auto-recompile and reload
4. ✅ Write CLI tests
5. ✅ Add CLI documentation

**Deliverables**:

- CLI tool with compile/validate/serve commands
- Dev server with hot-reload
- CLI tests passing
- User documentation

**Effort**: 3-4 days

---

### Phase 10: Testing & Benchmarks (Week 10-11)

**Goal**: Comprehensive testing and performance validation

**Tasks**:

1. ✅ Write integration tests:
   - End-to-end compilation
   - End-to-end query execution
   - Multi-database tests (PostgreSQL, MySQL, SQLite, SQL Server)
2. ✅ Write performance benchmarks:
   - Compilation speed
   - Query execution speed
   - Cache hit rates
   - Connection pool performance
3. ✅ Add load testing:
   - Concurrent queries
   - Sustained load
   - Memory profiling
4. ✅ Test coverage analysis:
   - Target: 85%+ coverage
   - Identify gaps
   - Add missing tests

**Deliverables**:

- 85%+ test coverage
- Performance benchmarks
- Load test results
- Coverage report

**Effort**: 7-10 days

---

### Phase 11: Documentation & Examples (Week 11-12)

**Goal**: Complete developer documentation and examples

**Tasks**:

1. ✅ Write API documentation:
   - Rust API docs (rustdoc)
   - Python API docs
   - CLI documentation
2. ✅ Create examples:
   - Basic schema
   - Federation example
   - Enterprise example (RBAC, audit)
3. ✅ Write migration guide:
   - v1 → v2 migration steps
   - Breaking changes
   - Feature parity matrix
4. ✅ Update README
5. ✅ Create changelog

**Deliverables**:

- Complete API documentation
- Example schemas
- Migration guide
- Updated README
- Changelog

**Effort**: 5 days

---

## Timeline Summary

| Phase | Duration | Type | Complexity |
|-------|----------|------|------------|
| 1. Foundation | 2-3 days | ✅ Reuse | Low |
| 2. Database & Cache | 6 days | 🔧 Adapt | Medium |
| 3. Security | 2 days | ✅ Reuse | Low |
| 4. Compiler | 10-12 days | ❌ New | High |
| 5. Runtime | 12-15 days | ❌ New | High |
| 6. HTTP Server | 5 days | 🔧 Adapt | Medium |
| 7. Utilities | 4-5 days | 🔧 Adapt | Low |
| 8. Python FFI | 5-7 days | 🔧 Adapt | Medium |
| 9. CLI | 3-4 days | ❌ New | Low |
| 10. Testing | 7-10 days | ❌ New | Medium |
| 11. Documentation | 5 days | ❌ New | Low |
| **Total** | **61-73 days** | | |

**Calendar Time**:

- **Optimistic**: 10 weeks (parallel work, minimal blockers)
- **Realistic**: 12-14 weeks (sequential dependencies, testing)
- **Conservative**: 16-18 weeks (architectural refinements, polish)

---

## Risk Assessment

### Low Risk (Mitigated by v1 Reuse)

- ✅ Database layer - proven in production
- ✅ Security - battle-tested auth/audit
- ✅ Configuration - stable and complete
- ✅ Error handling - comprehensive types

### Medium Risk (New Development)

- ⚠️ Compiler - new code but clear requirements
- ⚠️ Runtime - new execution model but proven SQL patterns
- ⚠️ HTTP server - adaptation of v1 patterns

### High Risk (Critical Path)

- 🔴 Compiler correctness - must generate valid SQL
- 🔴 Runtime performance - must match/exceed v1
- 🔴 Schema validation - must catch errors at compile-time

---

## Success Criteria

### Alpha Release (v2.0.0-alpha.2)

- [ ] Core compilation working (PostgreSQL only)
- [ ] Basic query execution (SELECT)
- [ ] Mutations working (INSERT, UPDATE, DELETE)
- [ ] Python decorators functional
- [ ] CLI tool compiles schemas
- [ ] Integration tests passing
- [ ] Basic benchmarks show feasibility

### Beta Release (v2.0.0-beta.1)

- [ ] All databases supported (PostgreSQL, MySQL, SQLite, SQL Server)
- [ ] Complete security layer (auth, RBAC, audit)
- [ ] Caching working (APQ + result cache)
- [ ] Federation support
- [ ] Subscriptions working (CDC)
- [ ] 85%+ test coverage
- [ ] Performance parity with v1

### Production Release (v2.0.0)

- [ ] All documentation complete
- [ ] Migration guide from v1
- [ ] Example schemas
- [ ] Load testing validated
- [ ] Security audit passed
- [ ] Production deployment guide
- [ ] Community feedback addressed

---

## Next Steps

1. **Create Cargo workspace** (this document provides structure)
2. **Begin Phase 1**: Copy foundation modules from v1
3. **Set up CI/CD**: GitHub Actions for testing
4. **Create project roadmap**: GitHub project board with milestones

**Ready to start implementation!** 🚀

---

*Last Updated: January 11, 2026*
*Status: Planning Complete, Ready for Phase 1*
