# FraiseQL Python Refactoring Plan
## Aligning Python Layer to "Python Author → Rust Execute" Architecture

**Status**: Strategic Plan
**Date**: 2026-01-10
**Total Python Code**: 467 files, 13MB
**Target Architecture**: Python as DSL/Authoring layer only; Rust handles all execution

---

## Executive Summary

### Current State
- Python has 467 files (13MB) handling both **schema authoring** AND **query execution**
- Significant duplication with Rust layer (query building, WHERE clause handling, type conversion)
- Mixed responsibilities create tight coupling and maintenance burden

### Target State
- **Python: Schema authoring, configuration, business logic only**
- **Rust: All execution, compilation, HTTP serving**
- **Clear boundary**: CompiledSchema JSON crosses FFI once at startup; zero Python during requests

### Key Principle
> *"Stop asking Python to do what Rust already does. Let Python author schemas, let Rust execute them."*

---

## Part 1: Python Module Analysis

### Current Structure (by size and responsibility)

| Module | Size | Current Role | Target Role | Action |
|--------|------|--------------|-------------|--------|
| **sql/** | 1.1M | Query generation | ❌ ELIMINATE | Migrate to Rust |
| **types/** | 892K | Type definitions | ✅ KEEP (enhanced) | Improve, document |
| **enterprise/** | 544K | Security, audit, RBAC | ✅ KEEP (enhanced) | Move execution to Rust |
| **security/** | 496K | Auth, validation | ✅ KEEP (enhanced) | Config only, execution in Rust |
| **monitoring/** | 468K | Observability | ✅ KEEP | Enhance Rust integration |
| **cli/** | 468K | CLI tools | ⚠️ PARTIAL | Keep schema tools, eliminate execution tools |
| **fastapi/** | 396K | FastAPI integration | ❌ ELIMINATE (optional) | Prefer Axum (Rust) |
| **axum/** | 364K | Axum integration | ⚠️ PARTIAL | Keep schema loading, eliminate execution |
| **db/** | 304K | DB operations | ❌ ELIMINATE | Migrate to Rust |
| **core/** | 288K | Core execution | ❌ ELIMINATE | Migrate to Rust |
| **mutations/** | 280K | Mutation handling | ⚠️ PARTIAL | Keep field selection, eliminate execution |
| **federation/** | 260K | Federation | ⚠️ PARTIAL | Config only |
| **gql/** | 244K | GraphQL builders | ⚠️ PARTIAL | Keep schema definition, eliminate builders |
| **auth/** | 244K | Authentication | ✅ KEEP | Config only |
| **... (20+ more)** | 3M | Various | ⚠️ VARIES | Per-module analysis |

---

## Part 2: Refactoring Phases

### Phase 1: Schema Authoring Layer (Foundation)
**Duration**: 2-3 weeks | **Effort**: Medium | **Risk**: Low

**Goal**: Establish clean Python authoring APIs that produce clean Rust schema JSON

#### 1.1 Clean Up Type System
- **File**: `types/` (892K)
- **Action**:
  - Keep all type definitions (@fraiseql.type, @fraiseql.query, @fraiseql.mutation)
  - Remove runtime execution code (parameter binding, result transformation)
  - Improve documentation and examples
  - Ensure proper JSON schema generation
- **Output**: Clean, well-documented type system that compiles to JSON

#### 1.2 Standardize Schema Compiler
- **Files**: `decorators.py`, `schema/`, `fields.py`
- **Action**:
  - Create single `SchemaCompiler` entry point
  - Output: `CompiledSchema` in clean JSON format
  - Version the schema JSON format
  - Add schema validation
- **Verification**: Schema produces valid JSON that Rust `CompiledSchema::from_json()` can parse

#### 1.3 Create Configuration Layer
- **Files**: Create new `config/` module (consolidate from `enterprise/`, `security/`, etc.)
- **Action**:
  - Centralize all Rust-bound configuration
  - Database connection strings
  - Authentication settings
  - Audit configuration
  - Security policies
- **Output**: Validated config objects that serialize to JSON for Rust

#### 1.4 Validate against PrintOptim
- **Action**: Ensure PrintOptim can use new APIs
- **Testing**: Run PrintOptim test suite against refactored Python layer

---

### Phase 2: Eliminate SQL/Query Execution (High Impact)
**Duration**: 3-4 weeks | **Effort**: High | **Risk**: Medium | **Impact**: -700KB Python code

**Goal**: Remove all SQL generation and query execution from Python

#### 2.1 Deprecate `sql/` Module (1.1M)
- **Current**: 44 files generating SQL (SELECT, INSERT, UPDATE, DELETE, mutations, etc.)
- **Target**: ZERO Python SQL generation
- **Migration**:
  ```
  sql/ (Python)              →  Rust fraiseql_rs/core/src/query/
  query_builder.py           →  query_builder.rs (DONE)
  where_clause_builder.py    →  where_builder.rs (EXISTS)
  mutation_builder.py        →  mutation_builder.rs (NEW)
  aggregate_builder.py       →  aggregate_builder.rs (NEW)
  ```
- **Action**:
  1. Audit what sql/ actually does
  2. Map to Rust equivalents (most already exist)
  3. Create FFI bindings for Rust builders
  4. Rewrite Python layer to call Rust
  5. Delete Python implementations
- **Tests**: All 2000+ SQL generation tests move to Rust (cargo test)

#### 2.2 Deprecate `db/` Module (304K)
- **Current**: Database connection, query execution, result mapping
- **Target**: ZERO (Rust handles via tokio-postgres)
- **Keep**: Database pool configuration classes (for Python config)
- **Action**:
  1. Move pool config to `config/`
  2. Move type mapping (db.py types) to `types/`
  3. Delete execution code
- **Verification**: PrintOptim only uses config APIs, not execution

#### 2.3 Deprecate `core/` Module (288K)
- **Current**: Execution engine, pipeline orchestration
- **Target**: ZERO (Rust Tokio runtime handles this)
- **Action**:
  1. Identify what core/ actually does (likely query execution wrapper)
  2. Replace with Rust-side equivalents
  3. Remove Python calls
  4. Delete module
- **Verification**: Zero changes to public API

---

### Phase 3: Simplify GraphQL Builder/Execution (High Impact)
**Duration**: 2-3 weeks | **Effort**: Medium | **Risk**: Medium | **Impact**: -500KB Python code

**Goal**: Move GraphQL execution to Rust; keep schema definition in Python

#### 3.1 Keep Only Schema Definition in `gql/`
- **Current**: 244K of builders, resolvers, execution
- **Target**: ~50K for schema definition only
- **Action**:
  1. Keep: `@fraiseql.type`, field definitions, field metadata
  2. Keep: Type decorators and arguments
  3. Remove: Resolver execution, query planning, field resolution
  4. Remove: Query composition logic
- **Output**: Schema definition that compiles to clean JSON

#### 3.2 Move Resolver Execution to Rust
- **Current**: Python resolvers execute and transform data
- **Target**: Rust executors with Python callbacks (optional for custom logic)
- **Action**:
  1. Design minimal Python→Rust callback interface (if needed)
  2. Move core resolution to Rust
  3. Keep only custom business logic in Python
  4. Make callbacks optional
- **Note**: Start without callbacks; add if needed

#### 3.3 Deprecate `execution/` and `graphql/` modules
- **Current**: 200K+ of execution orchestration
- **Target**: ZERO Python execution (Rust handles)
- **Action**: Delete after moving essential pieces to config/schema

---

### Phase 4: Enterprise Features (Security, Audit, Federation)
**Duration**: 2-3 weeks | **Effort**: High | **Risk**: Medium

**Goal**: Move execution of security/audit features to Rust; keep policies in Python

#### 4.1 Security Module Refactoring (496K)
- **Keep**:
  - Authentication configuration
  - Authorization policies (as data)
  - Role definitions
- **Move to Rust**:
  - Token validation
  - Permission checking
  - Rate limiting
  - Introspection filtering
- **Action**:
  1. Export security policies as JSON
  2. Rust loads policies once at startup
  3. Enforce policies during execution
  4. Remove Python enforcement code
- **Impact**: -300K Python code

#### 4.2 Enterprise Audit (544K)
- **Current**: Phase 9B integration (partially done)
- **Keep**:
  - Audit configuration
  - Event definitions
  - Storage backend interfaces
- **Move to Rust**:
  - Event capture
  - Event storage
  - Event filtering
- **Action**:
  1. Define audit event JSON format
  2. Implement audit backends in Rust
  3. Python triggers via HTTP callbacks (or async events)
  4. Delete Python event handling
- **Impact**: -200K Python code

#### 4.3 Federation (260K)
- **Current**: Likely mixed schema+execution
- **Keep**: Federation configuration
- **Move to Rust**: Federation resolution, subgraph queries
- **Action**: Analyze and refactor per findings

---

### Phase 5: Integration Layers (FastAPI, Axum, CLI)
**Duration**: 1-2 weeks | **Effort**: Medium | **Risk**: Low | **Impact**: -400KB Python code

**Goal**: Keep only thin integration layers; execution handled by Rust

#### 5.1 FastAPI Integration (396K)
- **Option A (Recommended)**: Deprecate in favor of Rust Axum
  - Requires PrintOptim migration
  - Gain: Full performance, simpler deployment
  - Effort: 3-4 weeks (PrintOptim refactoring)

- **Option B (Compatibility)**: Keep as thin wrapper around Rust
  - Python receives request
  - Calls Rust execution via FFI
  - Returns response
  - Effort: 1 week
  - Maintains PrintOptim compatibility

#### 5.2 Axum Integration (364K)
- **Current**: Native Rust server (already done)
- **Action**: Ensure Python can load schemas into Axum
- **Keep**: Schema loading utilities
- **Remove**: Any execution code

#### 5.3 CLI Tools (468K)
- **Keep**:
  - Schema validation tools
  - Schema migration tools
  - Configuration generators
- **Remove**:
  - Query execution tools
  - Debugging tools that require Python execution
  - Custom query builders
- **Impact**: -200K Python code

---

### Phase 6: Testing, Documentation, Polish
**Duration**: 2 weeks | **Effort**: Medium | **Risk**: Low

**Goal**: Comprehensive testing of refactored Python layer

#### 6.1 Test Migration
- Migrate all Python execution tests → Rust tests
- Keep Python tests for:
  - Schema validation
  - Decorator behavior
  - Configuration serialization
- Result: 80% fewer Python tests (5000+ → 1000)

#### 6.2 Documentation
- Document new "Python Author, Rust Execute" architecture
- Update PrintOptim integration guide
- Migration guide for existing applications

#### 6.3 Backward Compatibility
- Identify breaking changes
- Deprecation warnings for old APIs
- Migration paths for users

---

## Part 3: Module-by-Module Refactoring

### ELIMINATE Entirely (Priority 1)
These modules should be completely removed after migration:

1. **sql/** (1.1M) - Move to Rust QueryBuilder, WhereBuilder, etc.
2. **db/** (304K) - Move to Rust tokio-postgres, config to Python
3. **core/** (288K) - Execution engine; move to Rust
4. **execution/** (~150K) - Orchestration; move to Rust
5. **graphql/** (~120K) - Execution layer; move to Rust
6. **fastapi/** (396K) - Optional; keep for PrintOptim or migrate to Axum

**Total Elimination**: ~2.4MB (18% of Python code)

### REFACTOR (Priority 2)
These modules need significant changes:

1. **mutations/** (280K)
   - Keep: Field selection logic, mutation schema
   - Move: Execution to Rust
   - Keep: Cascade definitions, field metadata
   - Remove: SQL building, result handling
   - Target size: ~80K

2. **gql/** (244K)
   - Keep: @fraiseql.type, field definitions, schema
   - Remove: Builders, resolution logic
   - Target size: ~100K

3. **security/** (496K)
   - Keep: Auth config, policy definitions, RBAC rules
   - Move: Enforcement to Rust
   - Target size: ~200K

4. **enterprise/** (544K)
   - Keep: Audit config, event definitions
   - Move: Audit capture, storage to Rust
   - Target size: ~250K

5. **cli/** (468K)
   - Keep: Schema tools, validation
   - Remove: Execution tools
   - Target size: ~100K

6. **monitoring/** (468K)
   - Keep: Monitoring config, metric definitions
   - May migrate execution to Rust for better observability
   - Target size: ~250K

**Total Refactoring**: ~6.5MB to ~1.0MB (85% reduction)

### KEEP (Priority 3)
These are essential and should be preserved:

1. **types/** (892K) - Type definitions, decorators
2. **decorators.py** (40K) - Schema decorator syntax
3. **auth/** (244K) - Auth configuration
4. **config/** (create new) - Consolidated config
5. **validation.py**, **where_normalization.py** - Support utilities

**Total Kept**: ~1.2MB

### Size Summary

| Category | Before | After | Change |
|----------|--------|-------|--------|
| Eliminate | 2.4M | 0M | -2.4M (-100%) |
| Refactor | 6.5M | 1.0M | -5.5M (-85%) |
| Keep | 1.2M | 1.2M | 0M |
| **Total** | **13M** | **2.2M** | **-10.8M (-83%)** |

**Target**: Reduce Python from 467 files (13MB) to ~100 files (2.2MB)

---

## Part 4: Implementation Strategy

### Option A: Big Bang (NOT RECOMMENDED)
- Refactor everything at once
- Risk: High (breaks entire codebase)
- Benefit: Clean, fast
- Time: 8-12 weeks
- **Recommendation**: NO

### Option B: Incremental Deprecation (RECOMMENDED)
1. Phase 1: Establish clean Python authoring layer (Week 1-3)
2. Phase 2: Eliminate SQL generation (Week 4-7)
3. Phase 3: Eliminate core execution (Week 8-10)
4. Phase 4: Refactor enterprise features (Week 11-13)
5. Phase 5: Integration layers (Week 14-15)
6. Phase 6: Testing & cleanup (Week 16-17)

**Timeline**: 4-5 months with 1 developer
**Risk**: Low (gradual, can rollback)
**Benefit**: Can ship incremental improvements

### Option C: Dual Runtime (HYBRID)
- Keep Python layer "as is"
- Route execution to Rust gradually
- Move one module at a time
- Allows existing apps to continue working
- **Timeline**: 6-8 months
- **Risk**: Maintenance burden (dual implementations)

**Recommendation**: Option B (Incremental Deprecation)

---

## Part 5: Execution Checklist

### Pre-Refactoring Validation
- [ ] All tests passing (5991+ tests)
- [ ] PrintOptim backend tests passing
- [ ] Schema validation complete
- [ ] Architecture review completed ✅

### Phase 1: Foundation (Schema Authoring)
- [ ] Audit types/ module structure
- [ ] Create SchemaCompiler
- [ ] Validate JSON schema format
- [ ] Document authoring APIs
- [ ] Test with PrintOptim
- [ ] **Commit**: "refactor(python): establish clean schema authoring layer"

### Phase 2: SQL Elimination
- [ ] Audit sql/ module (what's in it?)
- [ ] Map to Rust equivalents
- [ ] Create Rust builders (if missing)
- [ ] Add FFI bindings
- [ ] Rewrite Python to use Rust builders
- [ ] Delete Python implementations
- [ ] Run 2000+ SQL tests in Rust
- [ ] **Commit**: "refactor(python): eliminate SQL generation, use Rust builders"

### Phase 3: Core Execution
- [ ] Audit core/ module
- [ ] Identify execution logic
- [ ] Move to Rust equivalent
- [ ] Test end-to-end
- [ ] Delete Python core/
- [ ] **Commit**: "refactor(python): eliminate core execution layer"

### Phase 4-6: ...
(Continue checklist per phase)

---

## Part 6: Success Criteria

### Code Quality
- [ ] Python code reduced from 13MB to 2.2MB (83% reduction)
- [ ] Zero duplication with Rust layer
- [ ] All modules have clear, documented purpose
- [ ] Type hints throughout (Python 3.13+)
- [ ] Comprehensive docstrings

### Performance
- [ ] Query execution >10x faster (Rust vs Python)
- [ ] No FFI calls per-request (only at startup)
- [ ] Memory usage reduced 50%

### Compatibility
- [ ] PrintOptim backend tests: 100% pass
- [ ] Existing apps work with Python layer
- [ ] Clean migration path for users

### Testing
- [ ] 5991+ tests passing
- [ ] 2000+ Python execution tests migrated to Rust
- [ ] New schema authoring tests (100+)
- [ ] Integration tests with Rust layer

### Documentation
- [ ] Updated architecture guide
- [ ] Migration guide for users
- [ ] Examples of new Python authoring style
- [ ] Deprecation warnings for old APIs

---

## Part 7: Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| PrintOptim breaks | High | Critical | Test continuously; provide migration guide |
| Incomplete Rust impl | Medium | High | Audit what Python does first; build Rust equivalent |
| Performance regression | Low | High | Benchmark each phase |
| Deployment issues | Medium | Medium | Provide Docker images; test in staging |
| Team resistance | Low | Medium | Clear communication of benefits |

---

## Part 8: Resource Requirements

### Team Composition
- **Senior Architect** (you): Architecture decisions, code review
- **Rust Developer**: Implement Rust query builders, execution
- **Python Developer**: Refactor Python layer, testing

### Tools
- Existing: cargo, pytest, ruff, clippy
- New: Python→JSON schema tool, benchmarking suite

### Timeline
- **Option B (Incremental)**: 4-5 months, 1 developer
- **Option B (Team of 2)**: 2-3 months

---

## Part 9: Next Steps (First Week)

### Immediate Actions
1. [ ] Decide between Option B (Incremental) vs Option C (Hybrid)
2. [ ] Create detailed checklist for Phase 1
3. [ ] Audit types/ module (what needs to change?)
4. [ ] Design clean JSON schema format
5. [ ] Create SchemaCompiler POC
6. [ ] Validate with PrintOptim

### Proposed Week 1-2 Work
```
Week 1:
- Monday-Tuesday: Complete Phase 1 code quality (Rust clippy fixes)
- Wednesday-Friday: Begin Python refactoring planning
  - Document current sql/ module (what does it do?)
  - Document db/ module (what does it do?)
  - Map to Rust equivalents

Week 2:
- Monday-Tuesday: Design clean JSON schema format
- Wednesday-Friday: Implement SchemaCompiler, validate
- Friday: Present plan to team
```

---

## Appendix A: Module Purpose Reference

### By Current Size
```
sql/               1.1M  SQL generation (SELECT, INSERT, UPDATE, DELETE)
types/             892K  Type system, decorators, field definitions
enterprise/        544K  Audit, RBAC, crypto, migrations
security/          496K  Auth, validation, introspection filtering
monitoring/        468K  Tracing, metrics, observability
cli/               468K  CLI tools, schema validation, testing
fastapi/           396K  FastAPI integration, middleware
axum/              364K  Axum HTTP server integration
db/                304K  Database connections, query execution
core/              288K  Execution engine, pipeline orchestration
mutations/         280K  Mutation handling, field selection
federation/        260K  GraphQL federation
gql/               244K  GraphQL builders, resolvers
auth/              244K  Authentication, JWT, OAuth
(20+ more)         3.0M  Various utilities, features
```

### By Purpose Category
```
EXECUTION (should eliminate):
- sql/ (1.1M)
- db/ (304K)
- core/ (288K)
- execution/ (~150K)
- graphql/ (~120K)
Total: ~2.0M

SCHEMA/AUTHORING (should keep):
- types/ (892K)
- gql/ (244K)
- mutations/ (280K)
- decorators.py (40K)
Total: ~1.5M

CONFIGURATION (should keep+enhance):
- security/ (496K)
- auth/ (244K)
- enterprise/ (544K)
- monitoring/ (468K)
- config/ (NEW)
Total: ~1.7M

INTEGRATION (partial):
- fastapi/ (396K) - Optional
- axum/ (364K) - Keep schema loading
- cli/ (468K) - Keep schema tools
Total: ~1.2M
```

---

**Status**: Plan Complete, Ready for Review
**Recommendation**: Proceed with Option B (Incremental Deprecation)
**Timeline**: 4-5 months with 1 developer
**Expected Outcome**: 13MB → 2.2MB Python code (83% reduction), faster queries, cleaner architecture
