# FraiseQL v1.0 Roadmap - UPDATED with Confiture

**Date**: October 11, 2025
**Current Version**: 0.11.0
**Major Update**: Confiture migration system now available as separate project

---

## 🎉 Major Change: Confiture Available

**Confiture** (PostgreSQL migration tool) is now being developed as an **independent project** that FraiseQL will integrate with.

### Impact on FraiseQL Roadmap

**Before** (Original Phase 1 Priority 1):
- ❌ Build custom migration system inside FraiseQL (4-6 weeks)
- ❌ High complexity, maintenance burden
- ❌ Delays v1.0 release

**After** (With Confiture):
- ✅ Integrate existing Confiture (1-2 weeks)
- ✅ FraiseQL gets best-in-class migrations
- ✅ Faster path to v1.0
- ✅ Can focus on GraphQL-specific features

---

## 📊 Updated Gap Analysis

### ✅ **RESOLVED: Database Migration System**

**Status**: ~~0% complete~~ → **90% complete via Confiture**

What Confiture provides out of the box:
- ✅ Build from DDL (fresh databases in <1s)
- ✅ Incremental migrations (up/down)
- ✅ Schema diff detection (auto-generate migrations)
- ✅ Version tracking
- ✅ CLI commands (`confiture build`, `confiture migrate`)
- ✅ Production data sync
- ✅ Zero-downtime migrations (schema-to-schema FDW)

**Remaining FraiseQL-specific work** (10%):
1. **GraphQL schema → DDL generation** (2-3 days)
   - Map GraphQL types to PostgreSQL types
   - Generate DDL from `@model` decorators
   - Sync GraphQL schema changes to `db/schema/`

2. **FraiseQL CLI integration** (1-2 days)
   - `fraiseql db build` → wraps `confiture build`
   - `fraiseql db migrate` → wraps `confiture migrate`
   - `fraiseql schema sync` → GraphQL-specific helper

3. **Documentation** (2-3 days)
   - FraiseQL + Confiture integration guide
   - Migration workflows for GraphQL developers
   - Examples with `@model` decorators

**Timeline**: 1-2 weeks (vs 4-6 weeks building from scratch)

---

## 🎯 Revised Roadmap Phases

### **Phase 1: Foundation Completion** (3-4 weeks) ⏰ Faster!

**Priority 1: Confiture Integration** ✅ NEW (replaces custom migration system)
- GraphQL schema → DDL generation
- FraiseQL CLI wrapper commands
- Integration tests
- **Timeline**: 1-2 weeks (vs 4-6 weeks original)

**Priority 2: Grafana Dashboards** (Unchanged)
- Create 5 production dashboard JSON files
- Import automation
- **Timeline**: 1 week

**Priority 3: Cache Invalidation Automation** (Unchanged)
- Event-driven cache clearing
- Trigger-based invalidation
- **Timeline**: 1-2 weeks

**Total Phase 1**: 3-4 weeks (vs 4-6 weeks original)
**Savings**: 1-2 weeks!

---

### **Phase 2: Enterprise Features** (3-4 weeks) - Unchanged

**Priority 1: Row-Level Security Helpers**
- RLS policy generators
- Multi-tenant patterns
- `@require_rls` decorator

**Priority 2: OpenTelemetry Full Integration**
- Automatic instrumentation
- Context propagation
- Span enrichment

**Priority 3: Advanced Mutation Patterns**
- Batch operations
- Optimistic locking
- Saga patterns

---

### **Phase 3: Developer Experience Polish** (3-4 weeks) - ENHANCED

**Priority 1: CLI Scaffolding Enhancement**
- `fraiseql generate model` - CRUD scaffolding
- `fraiseql generate resolver` - Query/mutation templates
- ~~`fraiseql generate migration`~~ → **Use `confiture migrate generate`** ✅

**Priority 2: TypeScript Type Generation**
- Complete type generation
- React hooks (optional)
- Type-safe query builders

**Priority 3: Production Examples**
- Multi-tenant SaaS (using Confiture migrations)
- Event sourcing example
- Real-time subscriptions

---

### **Phase 4: Performance & Credibility** (2-3 weeks) - Unchanged

**Priority 1: Comprehensive Benchmark Suite**
- vs Strawberry, PostGraphile, Hasura
- Real-world scenarios
- CI automation

**Priority 2: Production Case Studies**
- 3-5 production deployments
- Metrics documentation

**Priority 3: Performance Optimization**
- Query optimization
- Database tuning guides

---

### **Phase 5: Release Preparation** (2 weeks) - Unchanged

**Priority 1: Documentation Audit**
- Review all 28+ docs
- Update to v1.0 APIs

**Priority 2: Security Audit**
- Third-party review
- Dependency audit

**Priority 3: Migration Guide from 0.x**
- Breaking changes
- Automated migration tool

---

## 📅 Updated Timeline

| Phase | Duration | Key Deliverables | Target Date |
|-------|----------|------------------|-------------|
| **Phase 1: Foundation** | **3-4 weeks** ⚡ | **Confiture integration**, Grafana, Cache | **Nov 8, 2025** |
| **Phase 2: Enterprise** | 3-4 weeks | RLS, OpenTelemetry, Mutations | Dec 6, 2025 |
| **Phase 3: Developer DX** | 3-4 weeks | CLI, TS generation, Examples | Jan 3, 2026 |
| **Phase 4: Performance** | 2-3 weeks | Benchmarks, Case studies | Jan 24, 2026 |
| **Phase 5: Release Prep** | 2 weeks | Docs, Security, Migration | Feb 7, 2026 |

**Total**: 13-17 weeks (vs 14-19 weeks original)

**New v1.0 Release Date**: **February 7, 2026** (2 weeks earlier!)

---

## 🚀 NEW Competitive Advantages

With Confiture integration, FraiseQL now has:

### **1. Best-in-Class Migrations**
- Only GraphQL framework with build-from-scratch DDL approach
- Zero-downtime production migrations (schema-to-schema FDW)
- 4 migration strategies (build, migrate, sync, schema-to-schema)

### **2. GraphQL-Native Migration Workflow**
```python
# Define GraphQL model
@model
class User:
    id: int
    username: str
    display_name: str  # Changed from full_name

# Auto-sync to DDL
fraiseql schema sync  # Updates db/schema/10_tables/users.sql

# Auto-generate migration
fraiseql migrate generate  # Detects rename, creates migration

# Apply to production with zero downtime
fraiseql migrate schema-to-schema --strategy fdw
```

### **3. Unified Developer Experience**
```bash
# One tool for everything
fraiseql init                 # Scaffold project
fraiseql schema sync          # GraphQL → DDL
fraiseql db build             # Build database
fraiseql migrate up           # Apply migrations
fraiseql dev                  # Run dev server
```

---

## 🎯 What Makes FraiseQL v1.0 Unique (Updated)

| Feature | Strawberry | PostGraphile | Hasura | **FraiseQL v1.0** |
|---------|------------|--------------|--------|-------------------|
| **Migration System** | Alembic (separate) | Custom SQL | Hasura migrations | **Confiture (integrated)** |
| **Build-from-DDL** | ❌ No | ❌ No | ❌ No | **✅ Yes (<1s)** |
| **Zero-downtime migrations** | ❌ No | ❌ No | ⚠️ Manual | **✅ Built-in (FDW)** |
| **GraphQL → DDL sync** | ❌ No | N/A (DB-first) | N/A (DB-first) | **✅ Yes** |
| **PostgreSQL caching** | ❌ Redis | ❌ Redis | ❌ Redis | **✅ Native** |
| **Error tracking** | ❌ Sentry | ❌ Sentry | ❌ Separate | **✅ Native** |
| **Performance** | Medium | Fast | Fast | **Fastest (0.5-2ms)** |

---

## 💡 New Decisions with Confiture

### **What CHANGED**

1. **Database Migrations** ✅ RESOLVED
   - ~~Build custom migration system~~
   - **Use Confiture + GraphQL integration**
   - Faster to ship, better quality, maintained separately

2. **CLI Scaffolding** ✅ SIMPLIFIED
   - ~~`fraiseql generate migration`~~ → Use `confiture migrate generate`
   - FraiseQL CLI focuses on GraphQL-specific commands

3. **Production Examples** ✅ ENHANCED
   - All examples will demonstrate Confiture integration
   - Show zero-downtime migration workflows

### **What STAYS THE SAME**

- Grafana dashboards
- Cache invalidation automation
- Row-level security helpers
- OpenTelemetry integration
- TypeScript generation
- Performance benchmarks
- Security audit

---

## 📊 Risk Assessment Updates

| Risk | Before | After (with Confiture) | Mitigation |
|------|--------|------------------------|------------|
| **Migration system too complex** | High | **Low** ✅ | Confiture handles complexity |
| **Timeline slips** | Medium | **Low** ✅ | 2 weeks saved in Phase 1 |
| **Maintenance burden** | High | **Low** ✅ | Confiture maintained separately |
| **Integration complexity** | N/A | Low | Confiture designed for integration |

---

## 🎉 Benefits of Confiture Separation

### **For FraiseQL**
1. ✅ **Faster v1.0 release** (2 weeks earlier)
2. ✅ **Better migration system** (battle-tested, optimized)
3. ✅ **Reduced maintenance** (separate project)
4. ✅ **Unique selling point** ("Only framework with Confiture")
5. ✅ **Can focus on GraphQL features** (not database tooling)

### **For Users**
1. ✅ **Best-in-class migrations** (4 strategies)
2. ✅ **Works outside FraiseQL too** (Django, FastAPI, etc.)
3. ✅ **Active development** (dedicated project)
4. ✅ **Rust performance** (Phase 2: 10-50x faster)

### **For Ecosystem**
1. ✅ **Two complementary products** (FraiseQL + Confiture)
2. ✅ **Broader market reach** (Confiture for all Python/PostgreSQL)
3. ✅ **Network effects** (FraiseQL users drive Confiture adoption)

---

## 🚀 Immediate Next Steps (UPDATED)

### **Week 1-2: Confiture Integration**

**Milestone 1.1: GraphQL Schema → DDL Generation**
- Map GraphQL types to PostgreSQL types
- Generate DDL from `@model` decorators
- Tests: 20+ type mapping scenarios

**Milestone 1.2: FraiseQL CLI Integration**
- `fraiseql db build` wraps `confiture build`
- `fraiseql db migrate` wraps `confiture migrate`
- `fraiseql schema sync` (GraphQL-specific)
- Tests: 15+ CLI integration tests

**Milestone 1.3: Documentation**
- FraiseQL + Confiture guide
- Migration workflow examples
- GraphQL schema → DDL patterns

**Deliverable**: FraiseQL v0.12.0 with Confiture integration

---

### **Week 3-4: Grafana Dashboards + Cache Invalidation**

**Milestone 1.4: Grafana Dashboards**
- Create 5 dashboard JSON files
- Import automation script
- Documentation

**Milestone 1.5: Cache Invalidation**
- Event-driven clearing
- Trigger-based invalidation
- Documentation

**Deliverable**: FraiseQL v0.13.0 with observability complete

---

## 📊 Success Metrics (Updated)

### **Phase 1 Complete** (Nov 8, 2025)
- ✅ Confiture integrated (not custom migration system)
- ✅ GraphQL → DDL generation working
- ✅ 5 Grafana dashboards shipped
- ✅ Cache invalidation automated
- ✅ 100+ new tests passing

### **v1.0 Release** (Feb 7, 2026)
- ✅ All 5 phases complete
- ✅ 4,500+ tests passing
- ✅ Best-in-class migrations (via Confiture)
- ✅ Production-ready observability
- ✅ 1,000+ GitHub stars
- ✅ 5+ production deployments

---

## 🎯 What Else Does FraiseQL Need? (Analysis)

With **Confiture handling migrations**, FraiseQL can now focus on what makes it unique:

### **Core GraphQL Features** (Already Strong ✅)
- Type-safe schema generation ✅
- CQRS pattern ✅
- N+1 elimination ✅
- JSONB queries ✅

### **Gaps to Fill** (Prioritized)

#### **Critical (Must-Have for v1.0)**

1. **Grafana Dashboards** (Week 3-4)
   - Status: 50% complete (queries documented)
   - Need: Actual JSON files + import automation
   - Impact: Completes observability story

2. **Cache Invalidation** (Week 3-4)
   - Status: 30% complete (manual patterns)
   - Need: Automatic event-driven clearing
   - Impact: Production reliability

3. **Row-Level Security** (Phase 2)
   - Status: 0% complete
   - Need: RLS policy generators, `@require_rls` decorator
   - Impact: Multi-tenant SaaS apps

4. **OpenTelemetry Enhancement** (Phase 2)
   - Status: 40% complete
   - Need: Auto-instrumentation, context propagation
   - Impact: Production debugging

#### **Important (Should-Have for v1.0)**

5. **TypeScript Type Generation** (Phase 3)
   - Status: 30% complete
   - Need: Complete client SDK, React hooks
   - Impact: Frontend developer experience

6. **Advanced Mutations** (Phase 2)
   - Status: 60% complete
   - Need: Batch ops, optimistic locking, sagas
   - Impact: Complex business logic

7. **CLI Scaffolding** (Phase 3)
   - Status: 40% complete
   - Need: `fraiseql generate model/resolver`
   - Impact: Developer productivity

8. **Production Examples** (Phase 3)
   - Status: 70% complete
   - Need: Multi-tenant SaaS, event sourcing examples
   - Impact: Learning and adoption

9. **Performance Benchmarks** (Phase 4)
   - Status: 50% complete
   - Need: Comprehensive suite, CI automation
   - Impact: Credibility and marketing

#### **Nice-to-Have (Can Wait for v1.1)**

10. **GraphQL Subscriptions** (v1.1)
    - Status: 20% complete
    - Need: PostgreSQL NOTIFY/LISTEN, WebSocket
    - Impact: Real-time features

11. **Advanced Caching** (v1.1)
    - Status: 30% complete
    - Need: Query result caching, DataLoader
    - Impact: Performance optimization

12. **Monitoring UI** (v1.1+)
    - Status: 0% complete
    - Need: Built-in error/performance viewer
    - Impact: Developer experience (but Grafana covers this)

---

## 🎯 Recommended Focus Areas

With Confiture handling migrations, FraiseQL should focus on:

### **1. Production Readiness** (Phase 1-2)
- Grafana dashboards
- Cache invalidation
- RLS helpers
- OpenTelemetry

**Why**: Makes FraiseQL production-ready for enterprise

### **2. Developer Experience** (Phase 3)
- TypeScript generation
- CLI scaffolding
- Production examples

**Why**: Reduces onboarding time, increases adoption

### **3. Credibility** (Phase 4)
- Performance benchmarks
- Case studies
- Marketing

**Why**: Proves FraiseQL is fastest Python GraphQL framework

---

## 📝 Final Assessment

### **What FraiseQL Needs Most** (in order):

1. ✅ **Database Migrations** → SOLVED by Confiture
2. **Grafana Dashboards** → 2 weeks work
3. **Cache Invalidation** → 2 weeks work
4. **RLS Helpers** → 3 weeks work
5. **OpenTelemetry Enhancement** → 2 weeks work
6. **TypeScript Generation** → 3 weeks work
7. **Performance Benchmarks** → 2 weeks work
8. **Production Examples** → 2 weeks work

**Total remaining work**: 13-17 weeks

**Target v1.0**: **February 7, 2026**

---

## 🚀 Conclusion

**With Confiture available**, FraiseQL's path to v1.0 is:

- ✅ **Faster** (2 weeks saved)
- ✅ **Better** (best-in-class migrations)
- ✅ **Focused** (GraphQL-specific features, not DB tooling)
- ✅ **Unique** (only framework with Confiture integration)

**FraiseQL v1.0 will be production-ready by February 2026!**

---

**Last Updated**: October 11, 2025
**Status**: Ready for Phase 1 with Confiture integration
**Owner**: Lionel Hamayon (@evoludigit)

---

**Let's build the fastest Python GraphQL framework. Together.** 🚀
