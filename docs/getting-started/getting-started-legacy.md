# Getting Started - FraiseQL v1 Production Rebuild

**Location**: `~/code/fraiseql_v1`
**Goal**: Production-ready v1.0 by February 2026
**Strategy**: Clean rebuild with enterprise features
**Status**: Week 1/15 - Ready to start documentation

---

## 🎯 Quick Overview

**What**: Production-grade Python GraphQL framework
**Why**: Clean rebuild of v0 (50K → 10K LOC, enterprise features built-in)
**When**: 15 weeks to v1.0 release
**How**: Start fresh, port best parts, add production features

---

## 📚 Read These First

### **1. VISION.md** (Read now - 30 min)
Complete 15-week plan with:
- Production architecture (Trinity + Functions + CQRS + Rust)
- Enterprise features (RLS, OpenTelemetry, Grafana, Confiture)
- Week-by-week timeline
- Success criteria

### **2. ADVANCED_PATTERNS.md** (Skim now - 15 min)
Reference for implementation:
- Trinity identifiers (pk_*, fk_*, id, identifier)
- Mutations as PostgreSQL functions
- Complete SQL examples

### **3. COMPONENT_PRDS.md** (Week 3+ reference)
Detailed specs for 5 core components:
- Type System (800 LOC)
- Repositories (900 LOC)
- Decorators (700 LOC)
- WHERE Builder (500 LOC)
- Rust Transformer (500 LOC)

---

## 🚀 Week 1: Start Here (Documentation)

### **Day 1-2: WHY_FRAISEQL.md**

```bash
cd ~/code/fraiseql_v1
mkdir -p docs/philosophy
code docs/philosophy/WHY_FRAISEQL.md
```

**Write about** (~300 lines):
1. **The Problem**: GraphQL is slow in Python
   - Strawberry: 30-100ms
   - Graphene: 50-200ms
   - Why? N+1 queries, Python overhead, JSON serialization

2. **The Solution**: Database-level optimization
   - CQRS at database level (not app level)
   - Trinity identifiers (fast INT joins, secure UUID API)
   - Rust transformation (40x speedup)

3. **Performance Results**:
   - FraiseQL: 0.5-2ms
   - Benchmark table vs competitors
   - Production requirements

4. **When to Use** (be honest):
   - ✅ High throughput (100K+ QPS)
   - ✅ Complex nested queries
   - ✅ Production scale requirements
   - ❌ Simple prototypes (overkill)
   - ❌ Team unfamiliar with PostgreSQL

---

### **Day 3-4: CQRS_FIRST.md**

```bash
code docs/philosophy/CQRS_FIRST.md
```

**Write about** (~400 lines):
1. **What is CQRS?**
   - Command/Query separation
   - Why database-level (not app-level)
   - Benefits for production

2. **Trinity Identifiers** (deep dive):
   ```sql
   pk_user SERIAL PRIMARY KEY,    -- Fast joins
   fk_organisation INT,           -- Fast FKs
   id UUID,                       -- Public API
   identifier TEXT,               -- Human URLs
   ```

3. **Command vs Query Side**:
   - tb_* (normalized, fast writes)
   - tv_* (JSONB, fast reads)
   - fn_sync_tv_* (explicit sync)

4. **Production Benefits**:
   - 10x faster joins
   - Pre-computed data
   - Explicit control

---

### **Day 5-6: MUTATIONS_AS_FUNCTIONS.md**

```bash
code docs/philosophy/MUTATIONS_AS_FUNCTIONS.md
```

**Write about** (~350 lines):
1. **The Problem**: Python business logic
   - Not reusable (Python-only)
   - Manual transactions (error-prone)
   - Multiple round-trips (slow)

2. **The Solution**: PostgreSQL functions
   ```sql
   CREATE FUNCTION fn_create_user(...) RETURNS UUID AS $$
   BEGIN
       -- All logic in one place
       INSERT INTO tb_user (...) RETURNING id INTO v_id;
       PERFORM fn_sync_tv_user(v_id);
       RETURN v_id;
   END;
   $$ LANGUAGE plpgsql;
   ```

3. **Production Benefits**:
   - Reusable (any client)
   - Automatic transactions (ACID)
   - Testable in SQL (pgTAP)
   - Single round-trip

4. **Testing Example**:
   ```sql
   SELECT lives_ok(
       $$SELECT fn_create_user('acme', 'alice', ...)$$
   );
   ```

---

### **Day 7: RUST_ACCELERATION.md**

```bash
code docs/philosophy/RUST_ACCELERATION.md
```

**Write about** (~300 lines):
1. **Profiling Results**:
   - Where time goes in GraphQL
   - JSON transformation: 53% of total time!
   - Python: 4ms vs Rust: 0.1ms

2. **Why Rust?**:
   - Critical path optimization
   - 40x speedup proven
   - When to use systems languages

3. **Production Impact**:
   - 7.5ms → 3.6ms total (52% improvement)
   - Enables sub-1ms P95 latency
   - Graceful fallback if unavailable

---

### **Week 1 Success**

**By end of Week 1**:
- [x] 4 philosophy docs (~1,350 lines)
- [x] Clear production narrative
- [x] Foundation for architecture decisions
- [x] Team alignment on approach

**Deliverable**: Can explain "why this rebuild" to engineering teams

---

## 📅 Week 2 Preview

### **Architecture Documentation**

1. **OVERVIEW.md** - Complete system architecture
2. **NAMING_CONVENTIONS.md** - Trinity reference
3. **COMMAND_QUERY_SEPARATION.md** - CQRS details
4. **SYNC_STRATEGIES.md** - Explicit vs triggers
5. **SECURITY_MODEL.md** - RLS patterns

**Total**: ~1,000 lines

---

## 🗺️ 15-Week Roadmap Summary

| Phase | Weeks | Focus | Output |
|-------|-------|-------|--------|
| **Docs** | 1-2 | Philosophy & architecture | Foundation |
| **Core** | 3-6 | Type system, CQRS, API | v0.1-0.3 |
| **Performance** | 7-8 | Rust integration | v0.4 |
| **Migrations** | 9-10 | Confiture integration | v0.5 |
| **Enterprise** | 11-12 | RLS, observability, monitoring | v0.6-0.7 |
| **DevEx** | 13 | CLI, TypeScript, patterns | v0.8 |
| **Production** | 14-15 | Examples, docs, benchmarks | v1.0! |

---

## 🎯 Production Success Criteria

**Performance** (Week 15):
- [x] < 1ms query latency (P95)
- [x] 40x speedup (benchmarked vs Strawberry/Graphene)
- [x] 100K+ QPS on standard hardware

**Enterprise Features**:
- [x] Row-Level Security (multi-tenant ready)
- [x] OpenTelemetry (distributed tracing)
- [x] Grafana dashboards (5 pre-built)
- [x] Confiture migrations (zero-downtime)

**Developer Experience**:
- [x] CLI scaffolding (models, resolvers, migrations)
- [x] TypeScript generation (type-safe clients)
- [x] 3 production examples (SaaS, events, real-time)

**Documentation**:
- [x] Complete philosophy (4 docs)
- [x] Architecture guides (5 docs)
- [x] Deployment guides (Kubernetes, Docker)
- [x] API reference (complete)

---

## 💡 Key Architecture Decisions

### **Why Clean Rebuild?**
- v0: 50,000 LOC with accumulated complexity
- v1: 8,000-10,000 LOC with clean architecture
- 80% code reduction, 100% feature coverage
- Enterprise features designed-in (not bolted-on)

### **Why Trinity Identifiers?**
- Production requirement: fast joins + secure API + human URLs
- 10x performance gain (SERIAL vs UUID joins)
- Proven pattern at scale

### **Why PostgreSQL Functions?**
- Production reliability: atomic, reusable, testable
- Team alignment: database-first thinking
- Operational simplicity: single source of truth

### **Why Rust?**
- Production performance: 40x speedup on critical path
- Enables sub-1ms latency at scale
- Graceful degradation (fallback to Python)

---

## 🛠️ Development Workflow

### **Daily Routine**
1. Check VISION.md for current week objectives
2. Review relevant section in ADVANCED_PATTERNS.md
3. Implement/document according to plan
4. Test thoroughly (100% coverage on core)
5. Commit with clear messages

### **Weekly Milestones**
- Week 1-2: Documentation complete
- Week 3-4: v0.1.0 (types)
- Week 5-6: v0.2.0 (CQRS)
- Week 6-7: v0.3.0 (API)
- Week 7-8: v0.4.0 (Rust)
- Week 9-10: v0.5.0 (migrations)
- Week 11: v0.6.0 (enterprise)
- Week 12: v0.7.0 (monitoring)
- Week 13: v0.8.0 (DevEx)
- Week 14-15: v1.0.0 (production!)

### **Git Workflow**
```bash
# Feature branch
git checkout -b feature/week-N-component

# Regular commits
git commit -m "feat: implement X

- Detail 1
- Detail 2

Tests: 50+ added
Coverage: 95%+
"

# Merge when week complete
git checkout main
git merge feature/week-N-component
git tag v0.N.0
```

---

## 📚 Reference Documents

**In this repo**:
- `VISION.md` - Master plan (read first!)
- `ADVANCED_PATTERNS.md` - SQL examples
- `COMPONENT_PRDS.md` - Component specs
- `README.md` - Quick overview

**In parent repo** (`~/code/fraiseql`):
- v0 codebase - Reference when porting
- Don't copy blindly - simplify!
- Port only the best parts

---

## 🚦 Current Status

**Location**: `~/code/fraiseql_v1`
**Git**: Initialized with initial commit
**Week**: 1 of 15
**Phase**: Documentation
**Next**: Write docs/philosophy/WHY_FRAISEQL.md

---

## ⏭️ Your Next Actions

### **Right Now**
```bash
cd ~/code/fraiseql_v1
cat VISION.md  # Read complete plan (30 min)
```

### **Tomorrow (Week 1, Day 1)**
```bash
code docs/philosophy/WHY_FRAISEQL.md
# Start writing: The problem, solution, benchmarks, when to use
```

### **This Week**
- Day 1-2: WHY_FRAISEQL.md
- Day 3-4: CQRS_FIRST.md
- Day 5-6: MUTATIONS_AS_FUNCTIONS.md
- Day 7: RUST_ACCELERATION.md

**Outcome**: Complete production narrative for engineering teams

---

**Goal**: Production-ready v1.0 by February 2026
**Strategy**: Clean rebuild with enterprise features
**Timeline**: 15 weeks, on track

**Let's build production-grade GraphQL!** 🚀
