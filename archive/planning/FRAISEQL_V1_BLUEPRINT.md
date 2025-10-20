# FraiseQL v1 - Rebuild Blueprint for Portfolio/Hiring

**Purpose**: Clean, showcase-quality Python GraphQL framework demonstrating mastery of:
- CQRS architecture patterns
- Database-level performance optimization
- Rust integration for 40x+ speedups
- CLI-driven code generation
- Production-grade API design

**Target Audience**: Senior/Staff/Principal engineering roles at top companies

---

## 🔍 Phase 1: Code Audit Results

### ✅ **KEEP - Production-Quality Components**

#### 1. **Type System** (`src/fraiseql/types/`)
- **Why**: Clean, well-documented decorator API
- **Quality**: Comprehensive scalar types (UUID, DateTime, CIDR, LTree, etc.)
- **LOC**: ~800 lines, well-tested
- **Decision**: Port directly to v1, minimal changes
- **Files to port**:
  - `fraise_type.py` - Core type decorator
  - `fraise_input.py` - Input type decorator
  - `scalars/` - All scalar implementations
  - `generic.py` - Connection/Edge/PageInfo types

#### 2. **Where Clause Builder** (`src/fraiseql/sql/where/`)
- **Why**: "Marie Kondo" clean architecture (comment in code!)
- **Quality**: Function-based, testable, composable
- **LOC**: ~500 lines
- **Decision**: Port with improvements for tv_* views
- **Files to port**:
  - `core/field_detection.py`
  - `core/sql_builder.py`
  - `operators.py`

#### 3. **Rust Transformer** (`src/fraiseql/core/rust_transformer.py`)
- **Why**: This is your killer feature (40x speedup)
- **Quality**: Clean Python/Rust bridge
- **LOC**: ~200 lines Python + Rust crate
- **Decision**: Port and enhance for v1
- **Enhancement**: Make it central to the architecture

#### 4. **Decorator System** (`src/fraiseql/decorators.py`)
- **Why**: Clean API surface (@query, @mutation, @field)
- **Quality**: Well-documented with examples
- **LOC**: ~940 lines
- **Decision**: Simplify and port
- **Improvement**: Remove complexity (N+1 tracking, etc.) - keep it simple

#### 5. **CQRS Repository Core Logic** (`src/fraiseql/cqrs/repository.py`)
- **Why**: Command/query separation is architecturally sound
- **Quality**: Well-structured with good patterns
- **LOC**: 850 lines
- **Decision**: **Rebuild with new philosophy**
- **Changes**:
  - Remove `qm_*` references → use `tv_*` consistently
  - Remove trigger-based sync
  - Add explicit sync helpers
  - Simplify to core patterns only

---

### ❌ **REBUILD - Technical Debt**

#### 1. **Configuration System** (`src/fraiseql/config/`)
- **Why Rebuild**: Only 41 lines but likely too simple OR scattered elsewhere
- **Problem**: Configuration should be declarative and obvious
- **V1 Approach**: Single `FraiseQLConfig` with sensible defaults

#### 2. **Large Monolithic Files**
Files > 800 lines indicate complexity:
- `db.py` (1,969 lines) - Too much responsibility
- `operator_strategies.py` (1,457 lines) - Over-engineered
- `ivm/analyzer.py` (949 lines) - Complex feature, skip for v1

**Decision**: Break these into focused modules in v1

#### 3. **Feature Bloat Modules** (Skip for v1)
Remove these from v1 to focus on core value:
- `analysis/` - Complexity analysis (nice-to-have)
- `audit/` - Audit logging (add later)
- `cache/` + `caching/` - Two caching modules?! Pick one
- `debug/` - Debug mode (add later)
- `extensions/` - Unknown extensions
- `ivm/` - Incremental View Maintenance (complex, skip)
- `monitoring/` - Metrics/notifications (add later)
- `optimization/` - Separate module for optimization?
- `tracing/` - OpenTelemetry (add later)
- `turbo/` - TurboRouter (interesting but complex)
- `migration/` - Migrations (add later)
- `storage/` - APQ storage (add later)

**V1 Philosophy**: Ship a tight, focused core. Extensions come later.

---

## 📚 Phase 2: Documentation Structure

### **docs/** Directory Structure

```
docs/
├── README.md                          # Project overview, why FraiseQL exists
├── philosophy/
│   ├── WHY_FRAISEQL.md               # The problem we solve
│   ├── CQRS_FIRST.md                 # CQRS as the foundation
│   ├── RUST_ACCELERATION.md          # Why Rust transformation
│   └── CODEGEN_VISION.md             # The endgame: CLI codegen
├── architecture/
│   ├── OVERVIEW.md                   # High-level architecture diagram
│   ├── NAMING_CONVENTIONS.md         # tb_* vs tv_* vs v_*
│   ├── COMMAND_QUERY_SEPARATION.md   # How CQRS works in FraiseQL
│   ├── SYNC_STRATEGIES.md            # Explicit sync (no triggers)
│   └── RUST_INTEGRATION.md           # How Python↔Rust works
├── guides/
│   ├── QUICK_START.md                # 5-minute hello world
│   ├── DATABASE_SETUP.md             # PostgreSQL + JSONB setup
│   ├── WRITING_QUERIES.md            # @query decorator usage
│   ├── WRITING_MUTATIONS.md          # Explicit sync pattern
│   ├── TYPE_SYSTEM.md                # @type, scalars, etc.
│   └── PERFORMANCE.md                # Benchmarks, optimization tips
├── api/
│   ├── DECORATORS.md                 # @query, @mutation, @type, etc.
│   ├── REPOSITORY.md                 # CommandRepository, QueryRepository
│   ├── SYNC_FUNCTIONS.md             # sync_tv_*, batch operations
│   └── CLI.md                        # fraiseql CLI commands
└── examples/
    ├── BASIC_BLOG.md                 # Simple blog with CQRS
    ├── ECOMMERCE_API.md              # Product catalog example
    └── SAAS_MULTI_TENANT.md          # Advanced patterns
```

### **Key Documentation Principles**

1. **Philosophy First**: Explain WHY before HOW
2. **Show Performance**: Benchmarks in every doc
3. **Code Examples**: Every concept has working code
4. **Progressive Disclosure**: Simple → Advanced
5. **Diagrams**: Architecture diagrams for visual learners

---

## 📋 Phase 3: PRDs for Core Components

### **PRD 1: Core Type System & Decorators**

**Goal**: Clean, intuitive API for defining GraphQL types

**API Surface**:
```python
from fraiseql import FraiseQL, type, input, query, mutation

@type
class User:
    id: UUID
    name: str
    email: str
    posts: list["Post"]

@query
async def user(info, id: UUID) -> User:
    repo = QueryRepository(info.context["db"])
    return await repo.find_one("tv_user", id=id)
```

**Features**:
- `@type` - Define GraphQL object types
- `@input` - Define GraphQL input types
- `@query` - Define queries (auto-registered)
- `@mutation` - Define mutations (auto-registered)
- `@field` - Custom resolvers (simplified from v0)

**Implementation**:
- Port from `types/fraise_type.py`
- Port from `decorators.py` (simplified)
- Remove N+1 tracking, keep it simple
- Focus on clean API

---

### **PRD 2: CQRS Repository Pattern**

**Goal**: Explicit command/query separation with clear sync strategy

**API Surface**:
```python
# Command side (writes to tb_* tables)
class CommandRepository:
    async def create_user(self, name: str, email: str) -> UUID:
        user_id = await self.db.execute(
            "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
            name, email
        )
        # Explicit sync to query side
        await sync_tv_user(self.db, user_id)
        return user_id

# Query side (reads from tv_* views)
class QueryRepository:
    async def find_one(self, view: str, id: UUID) -> dict:
        return await self.db.fetchrow(
            "SELECT data FROM {} WHERE id = $1", view, id
        )
```

**Database Conventions**:
- `tb_*` = Tables (command side, normalized)
- `tv_*` = Table views (query side, denormalized JSONB)
- `fn_sync_tv_*` = Sync functions (explicit, no triggers)

**Features**:
- CommandRepository - Write operations
- QueryRepository - Read operations
- Explicit sync functions
- Batch operations
- Transaction support

**Files**:
- `fraiseql/repositories/command.py`
- `fraiseql/repositories/query.py`
- `fraiseql/repositories/sync.py`

---

### **PRD 3: Rust Transformation Layer**

**Goal**: 40x speedup on JSON transformation (snake_case → camelCase, field selection)

**API Surface**:
```python
# Transparent - user doesn't see this
result = await query_repo.find_one("tv_user", id=user_id)
# ↑ Automatically runs through Rust transformer
# Snake case DB → CamelCase GraphQL
```

**Rust crate** (`fraiseql_rs/`):
- `transform_json(json: &str, schema: &Schema) -> String`
- Field selection (only return requested fields)
- Type coercion (UUID → string, etc.)
- __typename injection

**Performance Target**:
- < 1ms for typical queries
- 40x faster than pure Python

**Files**:
- `fraiseql/core/rust_transformer.py` (Python bridge)
- `fraiseql_rs/src/lib.rs` (Rust implementation)

---

### **PRD 4: SQL Where Clause Builder**

**Goal**: Type-safe, composable where clauses for JSONB queries

**API Surface**:
```python
# Simple equality
where = {"status": "active"}

# Operators
where = {
    "created_at": {"gt": "2024-01-01"},
    "name": {"contains": "john"}
}

# Logical operators (v1.1+)
where = {
    "OR": [
        {"status": "active"},
        {"premium": True}
    ]
}
```

**Features**:
- Port from `sql/where/` (already clean!)
- Add tv_* JSONB support
- Operator functions: eq, ne, gt, lt, gte, lte, contains, in, not_in
- Future: AND/OR/NOT logical operators

**Files**:
- `fraiseql/sql/where_builder.py`
- `fraiseql/sql/operators.py`

---

### **PRD 5: CLI Tool (v1.1 - Future)**

**Goal**: Auto-generate backend from database schema

**Vision**:
```bash
# Analyze database schema
fraiseql init --database postgres://...

# Generate Python types from tables
fraiseql codegen types

# Generate tv_* views
fraiseql codegen views

# Generate sync functions
fraiseql codegen sync

# Generate GraphQL schema
fraiseql codegen schema

# All-in-one
fraiseql codegen --all
```

**Generated Files**:
- `models/user.py` - @type classes
- `migrations/001_create_tv_user.sql` - tv_* views
- `migrations/002_create_sync_functions.sql` - fn_sync_tv_* functions
- `schema.graphql` - GraphQL schema

**Implementation**: Phase 2 (after v1.0 core is stable)

---

## 🏗️ Phase 4: V1 Project Structure

```
fraiseql-v1/
├── README.md                          # Impressive, concise overview
├── pyproject.toml                     # Clean dependencies
├── docs/                              # See Phase 2 above
├── examples/
│   ├── quickstart/                    # 50-line example
│   ├── blog/                          # Full blog with CQRS
│   └── ecommerce/                     # Product catalog
├── src/
│   └── fraiseql/
│       ├── __init__.py                # Clean public API
│       ├── types/                     # Type system (KEEP)
│       ├── decorators/                # @query, @mutation (SIMPLIFY)
│       ├── repositories/              # Command/Query split (NEW)
│       │   ├── command.py
│       │   ├── query.py
│       │   └── sync.py
│       ├── core/
│       │   ├── rust_transformer.py    # (KEEP)
│       │   └── config.py              # (NEW - simple config)
│       ├── sql/
│       │   ├── where_builder.py       # (KEEP from where/)
│       │   └── operators.py
│       ├── fastapi/                   # FastAPI integration (KEEP)
│       └── cli/                       # CLI tool (v1.1)
├── fraiseql_rs/                       # Rust crate (KEEP)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                     # JSON transformer
└── tests/
    ├── test_types.py
    ├── test_repositories.py
    ├── test_where_builder.py
    └── test_rust_transform.py
```

**Total LOC Target**: ~3,000-5,000 lines (vs current ~50,000)
**Focus**: Quality over quantity, every line justifiable

---

## 🎯 Success Criteria for v1.0

### **Technical**
- [ ] < 1ms query latency (with Rust transform)
- [ ] 40x speedup over traditional GraphQL
- [ ] 100% test coverage on core
- [ ] Clean public API (< 20 exports in `__init__.py`)
- [ ] Zero configuration for quickstart

### **Documentation**
- [ ] Philosophy docs explain WHY
- [ ] Architecture diagrams for visual clarity
- [ ] 3 working examples (quickstart, blog, ecommerce)
- [ ] API reference for all public functions
- [ ] Benchmarks vs competitors

### **Portfolio Impact**
- [ ] GitHub README with impressive benchmarks
- [ ] "Built with FraiseQL" showcase apps
- [ ] Blog post: "Building the Fastest Python GraphQL Framework"
- [ ] Tech talk slides ready

---

## 📅 Recommended Timeline

**Week 1-2**: Documentation & PRDs (this phase)
- Write all philosophy docs
- Finalize API design
- Create architecture diagrams

**Week 3-4**: Core Implementation
- Port type system
- Build CommandRepository/QueryRepository
- Integrate Rust transformer

**Week 5-6**: Examples & Testing
- Build 3 example apps
- 100% test coverage
- Performance benchmarks

**Week 7-8**: Polish & Launch
- Documentation polish
- README with killer demo
- Blog post & tech talk prep

**Total**: 2 months to impressive v1.0

---

## 💡 Competitive Positioning

### **vs Strawberry**
- ✅ 40x faster (Rust transformation)
- ✅ CQRS built-in (vs manual DataLoaders)
- ✅ JSONB-first (vs ORM overhead)

### **vs Graphene**
- ✅ Modern async/await
- ✅ Database-level optimization
- ✅ Production patterns included

### **vs Tartiflette**
- ✅ PostgreSQL-optimized
- ✅ Rust acceleration
- ✅ CQRS patterns

**Unique Value**: "The only Python GraphQL framework built for sub-1ms queries at scale"

---

## 🎓 Learning Showcase

This rebuild demonstrates:
1. **Architectural Thinking**: CQRS, command/query separation
2. **Performance Engineering**: Rust integration, JSONB optimization
3. **API Design**: Clean, intuitive decorator pattern
4. **Documentation**: Philosophy-driven, example-rich
5. **Systems Thinking**: Database-level optimization
6. **Future Vision**: CLI codegen for 10x productivity

**Perfect for**: Staff+ engineering interviews, architecture discussions

---

## Next Steps

1. ✅ Review this blueprint
2. Create new repo: `fraiseql-v1/`
3. Start with documentation (philosophy docs)
4. Write PRDs in detail
5. Begin implementation

**Ready to start?** Let's move to Phase 2: Documentation Structure
