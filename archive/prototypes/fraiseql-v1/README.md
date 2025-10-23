# FraiseQL v1 - Architecture Prototype

## ⚠️ DEVELOPMENT PROTOTYPE - NOT FOR PRODUCTION

**This is an experimental prototype** exploring clean architecture patterns for the upcoming v1.0 industrial framework. **Not intended for production use.**

**For Production**: Use the stable [v0.11.5](../README.md) in the root directory.
**For v1 Development**: See [`fraiseql/`](../fraiseql/) directory for the main v1.0 implementation.

---

**Purpose**: Architecture exploration and pattern validation
**Strategy**: Rapid prototyping of core concepts
**Timeline**: 8 weeks to concept validation
**Status**: 🚧 Planning complete, ready for Week 1

**📍 You are here: v1 Architecture Prototype (Experimental)**

**Relationship to Main Project**: Experimental prototype exploring patterns for the main v1.0 rebuild. See [fraiseql/](../fraiseql/) for the production v1.0 development.

---

## 🎯 Vision

See **[`/V1_VISION.md`](../V1_VISION.md)** for the complete master plan.

**Quick Summary**:
- **Goal**: Validate architecture patterns for industrial v1.0 framework
- **Explores**: CQRS, database optimization, Rust integration, stored procedures
- **Core Patterns**: Trinity identifiers + Mutations as functions (DEFAULT)
- **Target**: ~3,000 LOC prototype (vs 50,000 in v0)
- **Performance**: Sub-1ms queries, 7-10x transformation speedup (Rust vs Python)

---

## 🏗️ Architecture Highlights

### **Trinity Identifiers** (DEFAULT)
```sql
-- Command Side: Fast joins with SERIAL, secure API with UUID
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,           -- Fast INT joins (10x faster)
    fk_organisation INT NOT NULL,         -- Fast foreign keys
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,  -- Public API
    identifier TEXT UNIQUE NOT NULL,      -- Human-readable (username)
    ...
);

-- Query Side: Clean GraphQL with just UUID
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    identifier TEXT UNIQUE NOT NULL,
    data JSONB NOT NULL
);
```

### **Mutations as Functions** (DEFAULT)
```sql
-- All business logic in PostgreSQL
CREATE FUNCTION fn_create_user(...) RETURNS UUID AS $$
BEGIN
    -- Validation, transaction, sync - all in one
    INSERT INTO tb_user (...) RETURNING id INTO v_id;
    PERFORM fn_sync_tv_user(v_id);
    RETURN v_id;
END;
$$ LANGUAGE plpgsql;
```

```python
# Python becomes trivial (3 lines)
@mutation
async def create_user(info, organisation: str, identifier: str, name: str, email: str):
    id = await db.fetchval("SELECT fn_create_user($1, $2, $3, $4)", ...)
    return await QueryRepository(db).find_one("tv_user", id=id)
```

---

## 📦 Project Structure

```
fraiseql-v1/
├── docs/                      # Philosophy-driven documentation
│   ├── philosophy/           # Why FraiseQL exists
│   ├── architecture/         # Technical deep dives
│   ├── guides/              # How-to guides
│   └── api/                 # API reference
├── examples/                 # Working examples
│   ├── quickstart/          # 5-minute hello world
│   ├── blog/                # Full blog with CQRS
│   └── ecommerce/           # Product catalog
├── src/fraiseql/            # Core library (~3,000 LOC)
│   ├── types/              # Type system (800 LOC)
│   ├── decorators/         # @query, @mutation (400 LOC)
│   ├── repositories/       # Command/Query/Sync (600 LOC)
│   ├── sql/                # WHERE builder (500 LOC)
│   ├── core/               # Rust transformer (300 LOC)
│   └── gql/                # Schema generation (400 LOC)
├── fraiseql_rs/            # Rust crate
└── tests/                  # 100% coverage on core
```

**Current Status**: Skeleton created (211 LOC), ready for implementation

---

## 🚀 8-Week Timeline

### **Week 1-2: Documentation Foundation** ⏳ NEXT
Philosophy docs (WHY_FRAISEQL, CQRS_FIRST, MUTATIONS_AS_FUNCTIONS, RUST_ACCELERATION)

### **Week 3-4: Core Implementation**
Type System + Decorators + Schema Generation

### **Week 5-6: CQRS Implementation**
Repositories (Command/Query/Sync) + WHERE Builder

### **Week 6-7: Rust Integration**
Port Rust transformer + Performance benchmarks

### **Week 7-8: Examples & Polish**
3 examples + Documentation polish + README with benchmarks

---

## 🎓 Interview Talking Points

**60-Second Pitch**:
> "I built FraiseQL to solve GraphQL performance in Python. Traditional frameworks suffer from N+1 queries and Python overhead. I implemented CQRS at the database level using PostgreSQL JSONB with a Trinity identifier pattern - SERIAL for fast joins, UUID for secure APIs, slugs for SEO. I wrote a Rust extension for JSON transformation giving 7-10x speedup. The result: sub-1ms query latency. All business logic lives in PostgreSQL functions, making it reusable and transactionally safe."

**Shows Understanding Of**:
- Database performance (INT vs UUID joins)
- API security (don't expose sequential IDs)
- Stored procedures (database-first thinking)
- Systems languages (Rust for critical path)
- Trade-off analysis (complexity vs performance)

---

## 🎯 Success Criteria

**Technical Validation**:
- [ ] < 1ms query latency
- [ ] 7-10x transformation speedup (benchmarked)
- [ ] 100% test coverage on core

**Architecture Proof**:
- [ ] Trinity identifiers pattern validated
- [ ] Mutations-as-functions pattern working
- [ ] Rust integration performance proven
- [ ] CQRS separation implemented

**Prototype Complete**:
- [ ] Core patterns implemented
- [ ] Performance benchmarks documented
- [ ] Architecture decisions validated
- [ ] Foundation for main v1.0 established

---

## 📚 Documentation

**Master Plan**: [`/V1_VISION.md`](../V1_VISION.md) - Complete vision and timeline

**Reference Docs** (synthesized into vision):
- `V1_COMPONENT_PRDS.md` - Component specifications
- `V1_ADVANCED_PATTERNS.md` - Trinity + Functions patterns
- `V1_DOCUMENTATION_PLAN.md` - Documentation structure

**Archived** (future v2 material):
- `_archive/v2_planning/` - Production evolution strategy (17 weeks)

---

## ⏭️ Next Step

**Start Week 1, Day 1**:
```bash
cd docs
mkdir -p philosophy architecture guides api
code philosophy/WHY_FRAISEQL.md
```

See V1_VISION.md for detailed Week 1 template and instructions.

---

**Built to validate architecture for industrial framework.** 🏗️
**Target: Production-ready v1.0 framework.**
**Timeline: 8 weeks to pattern validation.**

Let's prototype this! 🚀
