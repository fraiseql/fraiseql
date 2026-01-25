# FraiseQL v2: Implementation Plan (Compact)

**Updated**: January 25, 2026 | **Version**: 3.0 | **Focus**: Token-Efficient Reference

---

## 1️⃣ Current Status at a Glance

| Phases | Status | Est. Effort | Key Files |
|--------|--------|-------------|-----------|
| **1-7** | ✅ COMPLETE | — | Core GraphQL engine |
| **8.0-8.7** | 🔄 60% (8/13 subphases) | 3 weeks remaining | See Phase 8 below |
| **9.1-9.8** | ✅ CODE COMPLETE | ~4hrs testing | See Phase 9 below |
| **9.9** | ⏳ PENDING | 4 hours | Testing checklist |
| **9.10 (NEW)** | 📋 PLANNED | 2 weeks | Cross-language SDK |
| **10+** | 📋 PLANNED | TBD | See Future below |

---

## 2️⃣ Phase 8: Observer System Excellence (Continued)

**Status**: ~60% complete (8 of 13 subphases done)

| Sub-Phase | Status | Lines | Notes |
|-----------|--------|-------|-------|
| 8.0-8.4 | ✅ | — | Foundation, execution, caching |
| **8.7** | ✅ | 600 | Prometheus metrics + Grafana dashboard |
| **8.6** | 🟡 Ready | Plan ready | Job queue system (3-4 days) → `.claude/PHASE_8_6_PLAN.md` |
| 8.5, 8.8-13 | 🔵 Planned | — | Elasticsearch, resilience, tooling |

**Next**: Start Phase 8.6 (Job Queue) OR jump to Phase 9 testing
- Plan: `.claude/PHASE_8_6_PLAN.md` (comprehensive, 8 tasks)
- Effort: 3-4 days following step-by-step tasks

---

## 3️⃣ Phase 9: Apache Arrow Flight Integration

**Status**: ✅ CODE-COMPLETE (9.1-9.8), 🔄 TESTING-PENDING (9.9)

| Sub-Phase | Status | Impl. | Key Details |
|-----------|--------|-------|-------------|
| **9.1** | ✅ | 2,637 L | gRPC server, ticket routing, schema registry |
| **9.2** | ✅ | 951 L | GraphQL → Arrow row/column conversion |
| **9.3** | ✅ | 300+ L | NATS JetStream → Arrow streaming, EntityEvent schema |
| **9.4** | ✅ | 552 L | ClickHouse MergeTree sink + materialized views |
| **9.5** | ✅ | ~400 L | DDL generation helpers (Python, TypeScript, CLI) |
| **9.5b** | ✅ | — | Elasticsearch sink for operational search |
| **9.6** | ✅ | ~600 L | Python (PyArrow+Polars), R, Rust clients |
| **9.7** | ✅ | ~810 L | E2E tests, stress tests, benchmarks, chaos tests |
| **9.8** | ✅ | 2,279 L | Complete documentation + 4-phase migration guide |
| **9.9** | ⏳ | — | Pre-release testing checklist (4 hours) → `.claude/PHASE_9_PRERELEASE_TESTING.md` |
| **9.10 (NEW)** | 📋 | — | Cross-language SDK (see below) |

**All 9 crates + docs compile cleanly**: `cargo check --all-features` ✅

**Next Steps** (Priority Order):
1. 🔴 **CRITICAL**: Execute Phase 9.9 testing (~4 hours)
2. 🟢 **Then**: Phase 10 (Security) OR Phase 8.6 (Async)

---

## 4️⃣ Phase 9.10: Language-Agnostic Arrow Schema Authoring (NEW)

**Objective**: Enable Arrow schemas to be authored in ANY programming language

**Architecture Principle**: Authoring (any language) → Compilation → Runtime (Rust)

### Problem
- Arrow schemas currently defined in Rust code only
- Python/TypeScript/Go developers can't define schemas without Rust
- No standard schema format for other languages
- Schema changes require Rust recompilation

### Solution: Language-Neutral Schema Authoring

**Components** (1.5 weeks total):

| Component | Purpose | Output |
|-----------|---------|--------|
| **Python Library** | @schema decorator for Arrow definitions | `fraiseql_arrow` pip package |
| **TypeScript Library** | @Field decorators for Arrow definitions | `@fraiseql/arrow` npm package |
| **Schema Format** | JSON-based, language-neutral schema | `.arrow-schema` standard |
| **Rust Integration** | Load .arrow-schema files, serve via Flight | Schema registry in Arrow Flight server |
| **CLI Tools** | Validate, export, register schemas | `fraiseql arrow-schema` command |

**Implementation Plan**:

1. **Schema Format & Python/TypeScript Libraries** (2 days)
   - Define `.arrow-schema` JSON format (scalars, fields, constraints)
   - Python: @schema decorator → JSON export
   - TypeScript: @Field decorators → JSON export
   - Example:
     ```python
     @schema(namespace="fraiseql.events", version="1.0")
     class EntityEvent(Schema):
         event_id: String(required=True)
         timestamp: Timestamp(required=True, index=True)
         data: String(required=True)

     EntityEvent.to_schema_file("EntityEvent.arrow-schema")
     ```

2. **Rust Server Integration** (2 days)
   - Schema registry to load `.arrow-schema` files
   - Update Flight GetFlightInfo to serve loaded schemas
   - Auto-discovery from schemas directory

3. **CLI & Validation** (1 day)
   - `fraiseql arrow-schema validate` - Check schema validity
   - `fraiseql arrow-schema export` - Export to Arrow proto format
   - `fraiseql arrow-schema register` - Register with server

4. **Examples & Documentation** (1 day)
   - Python → JSON → Rust → Flight example
   - TypeScript → JSON → Rust → Flight example
   - YAML schema definition support
   - Schema versioning guide

5. **Integration Testing** (1 day)
   - Python-authored schema works with Rust server
   - TypeScript-authored schema works with Rust server
   - E2E: Author in Python → Load in Rust → Query with any client

---

## 5️⃣ Phase 10: Production Hardening

**Scope**: Security, deployment, reliability (starting AFTER Phase 9 testing)

| Sub-Phase | Effort | Key Features |
|-----------|--------|--------------|
| **10.1** | 3 days | Admission control, backpressure, rate limiting |
| **10.2** | 2 days | Deployment patterns (K8s, Docker, multi-region) |
| **10.3** | 3 days | Circuit breakers, retry logic, graceful degradation |
| **10.4** | 2 days | Performance optimization, profiling, tuning |

**Total**: 2-3 weeks

---

## 6️⃣ Key Architectural Decisions

| Decision | Why This Choice |
|----------|-----------------|
| **Arrow Flight (not gRPC/Protobuf alone)** | Zero-copy, columnar, built-in streaming |
| **Dual Dataplanes** | Analytics (ClickHouse) + Operational (Elasticsearch) |
| **Feature Flags** | Optional components, zero overhead when disabled |
| **NATS JetStream** | Distributed event delivery, persistent storage |
| **Redis** | Fast caching, deduplication, state |
| **Language-Agnostic Authoring** | Python/TypeScript for schema, Rust for runtime |
| **Compile-Time Optimization** | All SQL generated at build time (no runtime interpretation) |

---

## 7️⃣ Roadmap at a Glance

```
Q1 2026 (Now)
├─ Phase 8.7: Metrics ✅ DONE
├─ Phase 9.1-9.8: Arrow Flight ✅ CODE-COMPLETE
├─ Phase 9.9: Testing (4 hours)
└─ START: Phase 8.6 or Phase 9.9 testing

Q2 2026
├─ Phase 9.9: Testing ✅
├─ Phase 9.10: Cross-language SDK (2 weeks)
├─ Phase 10: Production hardening (2-3 weeks)
└─ Phase 8.5-13: Observer features (if time permits)

Q3+ 2026
├─ Phase 11: Advanced features (TBD)
├─ Performance tuning & optimization
└─ Enterprise features (RBAC, audit, etc.)
```

---

## 8️⃣ Detailed Phase Files

For deep dives, see:

| Phase | File | Content |
|-------|------|---------|
| **8.6** | `.claude/PHASE_8_6_PLAN.md` | 8 implementation tasks, detailed specs |
| **8.7** | `.claude/PHASE_8_7_PLAN.md` | Metrics, Grafana, PromQL queries |
| **9.1-9.8** | `.claude/PHASE_9_*.md` | Implementation summaries |
| **9.9** | `.claude/PHASE_9_PRERELEASE_TESTING.md` | 10-phase testing checklist |
| **Work** | `.claude/WORK_STATUS.md` | Current session progress |
| **Docs** | `docs/README.md` | All user documentation |

---

## 9️⃣ Immediate Next Steps

### Option A: Phase 9 Testing (Recommended for Release)
```bash
# See PHASE_9_PRERELEASE_TESTING.md
./run_prerelease_tests.sh  # ~4 hours
# Output: PHASE_9_RELEASE_RESULTS.md (go/no-go decision)
```

### Option B: Phase 8.6 Implementation (Recommended for Async)
```bash
# See PHASE_8_6_PLAN.md, follow 8 tasks
# Task 1: Job definitions (1 day)
# Task 2: Redis queue (1 day)
# Task 3: Executor (1 day)
# Tasks 4-8: Integration (1 day)
```

### Option C: Phase 9.10 Design (Recommended for Polyglot)
```bash
# Design cross-language Arrow SDK
# Week 1: IDL + code generators
# Week 2: Examples + integration tests
```

**Recommendation**: Do **Phase 9.9 testing** this session (unblocks everything)

---

## 🔟 Success Criteria

- ✅ All tests passing (255+ observer tests)
- ✅ Zero clippy warnings
- ✅ Code compiles with all features
- ✅ Documentation complete and accurate
- ✅ Performance targets met (15-50x Arrow vs HTTP/JSON)
- ✅ No time estimates given (focus on work, not time)

---

**Last Updated**: January 25, 2026 | **Next Review**: After Phase 9.9 testing
