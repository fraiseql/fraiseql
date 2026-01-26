# Architecture Decision Record: Apache Arrow Flight Integration

**Status**: Proposed
**Date**: January 24, 2026
**Decision Makers**: Architecture Review
**Scope**: FraiseQL v2 Phase 9 (4-5 weeks)

---

## Executive Summary

Integrate Apache Arrow Flight as FraiseQL's high-performance data delivery layer, establishing a **dual-dataplane architecture** that serves both analytics and operational use cases without redundant storage.

**Expected Impact**:
- 50x faster analytics queries (100k rows: 30s → 2s)
- 1M+ events/sec streaming to data warehouses
- Zero-copy integration with Python/R/Java
- No breaking changes to existing HTTP/JSON API

**Investment**: 4-5 weeks implementation + infrastructure (ClickHouse + Elasticsearch)

---

## Problem Statement

### Current Limitations

1. **Analytics Performance**
   - HTTP/JSON serialization too slow for large datasets (30s for 100k rows)
   - Memory inefficient (250MB for result sets that could be 50MB columnar)
   - JSON parsing overhead in Python/R kills data science workflows

2. **Event Analytics Gap**
   - Observer events go to actions (webhooks/emails) but nowhere for analytics
   - No time-series aggregations, no business intelligence dashboards
   - No real-time analytics on mutation events

3. **Data Warehouse Integration**
   - No efficient bulk export mechanism (JSON is too slow)
   - No streaming event pipeline to ClickHouse/Snowflake
   - Manual ETL processes required

4. **Cross-Language Integration**
   - JSON requires parsing in every language (Python, R, Java)
   - No zero-copy data consumption
   - Data scientists copy-paste → CSV → load (inefficient)

---

## Proposed Solution: Dual-Dataplane Architecture

### Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│              FraiseQL Dual Dataplane                      │
├──────────────────────────────────────────────────────────┤
│                                                           │
│  Analytics Dataplane          Operational Dataplane      │
│  ───────────────────          ────────────────────       │
│                                                           │
│  Arrow Flight + ClickHouse    HTTP/JSON + Elasticsearch  │
│  • Aggregations               • Full-text search         │
│  • Time-series analytics      • Debugging workflows      │
│  • ML pipelines               • Incident response        │
│  • Zero-copy to Python/R      • Flexible JSON queries    │
│  • 1M+ events/sec             • <100ms searches          │
│                                                           │
└──────────────────────────────────────────────────────────┘

Data Flow:

GraphQL Query → fraiseql-server
                    ↓
        ┌───────────┴───────────┐
        ↓                       ↓
   HTTP/JSON (8080)        Arrow Flight (50051)
        ↓                       ↓
   Web/Mobile             Analytics/Data Science


Observer Event → NATS JetStream
                    ↓
        ┌───────────┴───────────┐
        ↓                       ↓
   ClickHouse              Elasticsearch
   (analytics)             (operational search)
```

### Key Principle: No Redundant Source of Truth

Storage is duplicated by design for performance, but **truth is not duplicated**: both ClickHouse and Elasticsearch are derived from the same event stream and can be rebuilt.

| Question | Dataplane | Why |
|----------|-----------|-----|
| "How many orders/hour this month?" | ClickHouse | Aggregations (columnar, fast GROUP BY) |
| "Find orders with error PAYMENT_DECLINED" | Elasticsearch | Full-text search on JSON |
| "Average order value by region?" | ClickHouse | Analytics (window functions, time-series) |
| "Show events for user-123 last hour" | Elasticsearch | Debugging (flexible filters, fast lookup) |

**Not redundant** - complementary capabilities optimized for different query patterns.

### Data Authority & Rebuildability

* **PostgreSQL remains the system of record** for operational state (users, orders, products)
* **NATS JetStream is the event delivery backbone** (durable stream with at-least-once guarantees)
* **ClickHouse and Elasticsearch are derived projections** optimized for analytics and operational search
* Both sinks must be **rebuildable** from the event stream (replay from JetStream) to support schema evolution and disaster recovery
* Schema changes are additive (new columns/fields) to maintain replay compatibility

---

## Core Architectural Decisions

### Decision 1: Arrow Flight as Primary High-Performance Transport

**Rationale**:
- Industry standard (used by Snowflake, Databricks, Dremio)
- gRPC-based (modern, efficient, language-agnostic)
- Columnar format = 5-10x compression vs JSON
- Zero-copy deserialization in clients (PyArrow, arrow R package)

**Alternatives Considered**:
- ❌ **gRPC with Protobuf**: Not columnar, still requires deserialization
- ❌ **WebSockets**: No columnar support, requires custom serialization
- ❌ **Faster JSON (SIMD)**: Still 10x slower than Arrow

**Trade-offs**:
- ✅ Pro: Best-in-class performance, ecosystem support
- ⚠️ Con: Additional infrastructure (gRPC server on port 50051)
- ⚠️ Con: Web browsers can't use Arrow Flight (hence dual dataplane)

---

### Decision 2: ClickHouse for Analytics Dataplane

**Rationale**:
- Columnar database optimized for analytical queries
- Native Arrow support (can consume Arrow Flight streams)
- MergeTree engine = efficient inserts + fast aggregations
- Materialized views for pre-computed metrics
- Built-in retention (TTL) and compression (10:1 ratio)

**Alternatives Considered**:
- ❌ **PostgreSQL (existing)**: Row-based, not optimized for analytics
- ❌ **Snowflake**: Excellent but expensive, overkill for self-hosted
- ❌ **Apache Druid**: Complex setup, real-time focus (we have batching)

**Trade-offs**:
- ✅ Pro: 100-1000x faster analytics than PostgreSQL
- ✅ Pro: Handles 1M+ events/sec ingestion
- ⚠️ Con: Additional infrastructure component
- ⚠️ Con: Learning curve for SQL dialect (minor)

---

### Decision 3: Elasticsearch for Operational Dataplane

**Rationale**:
- De-facto standard for full-text search and log analysis
- Flexible JSON querying (no fixed schema)
- Fast lookup by ID, user_id, org_id (< 100ms)
- Kibana for visualization (debugging dashboards)
- ILM policies for automatic retention management

**Alternatives Considered**:
- ❌ **PostgreSQL JSONB + GIN index**: Slower, not designed for this
- ❌ **Meilisearch/Typesense**: Great for search, weak on analytics
- ❌ **Only ClickHouse (skip Elasticsearch)**: Poor at full-text search

**Trade-offs**:
- ✅ Pro: Best-in-class search capabilities
- ✅ Pro: Team familiarity (industry standard)
- ⚠️ Con: Additional infrastructure component
- ✅ Decision: Worth it - ClickHouse + Elasticsearch serve different purposes

---

### Decision 4: Dual Dataplane (Both ClickHouse AND Elasticsearch)

**Rationale**:
- **ClickHouse**: Optimized for "compute over data" (aggregations, joins, window functions)
- **Elasticsearch**: Optimized for "find the thing" (full-text search, flexible filters)
- Different query patterns = different optimal storage
- Not redundant: Each serves distinct use cases

**Alternatives Considered**:
- ❌ **ClickHouse only**: Poor at full-text search, slow flexible JSON queries
- ❌ **Elasticsearch only**: Poor at aggregations, can't handle 1M+ events/sec
- ✅ **Both**: Play to each system's strengths

**Cost Analysis**:
- Storage: ~2x (ClickHouse columnar ~50% compression, Elasticsearch full JSONB)
- Infrastructure: 2 databases vs 1
- **Value**: Unlock analytics + operational use cases (worth it)

**Real-World Precedent**:
- Datadog: Elasticsearch (search) + Cassandra (metrics)
- Uber: Elasticsearch (logs) + Pinot (analytics)
- Netflix: Elasticsearch (search) + Druid (analytics)

---

### Decision 5: Implementation Sequencing - Phase 9 Before Phase 8

**Proposed Order**:
1. **Phase 9**: Arrow Flight (4-5 weeks) - Foundation
2. **Phase 8**: Production features (metrics, CLI, testing) - Built on complete architecture

**Rationale**:
- Arrow Flight is a **foundational architectural change**
- If we implement Phase 8 features first, we'll need to update them all after adding Arrow Flight
- Building production features once for the complete architecture saves 30-40% engineering effort (avoids duplicate implementation)

**Concrete Example - Prometheus Metrics (Phase 8.7)**:

```
Approach 1: Phase 8 First → Phase 9 Later (INEFFICIENT)
─────────────────────────────────────────────────────
Week 1: Implement metrics for NATS-only
Week 4: Add Arrow Flight
Week 5: Update metrics for Arrow Flight + ClickHouse + Elasticsearch
Result: Duplicate work (implement metrics twice)

Approach 2: Phase 9 First → Phase 8 Later (EFFICIENT)
──────────────────────────────────────────────────
Week 1-5: Implement Arrow Flight + ClickHouse + Elasticsearch
Week 6: Implement metrics for NATS + Arrow + ClickHouse + ES (once)
Result: Single implementation covering complete architecture
```

**Engineering Effort Savings**:
- Metrics: 30% savings (implement once vs twice)
- CLI tools: 40% savings (debug all transports together)
- Testing: 50% savings (test complete system once)
- Elasticsearch: 100% savings (Phase 8.5 becomes part of Phase 9.5)

**Trade-offs**:
- ⚠️ Con: Phase 8 production features (metrics, CLI, etc.) delayed by 4-5 weeks
- ✅ Pro: When implemented, they cover the complete architecture (NATS + Arrow + ClickHouse + ES)
- ✅ Pro: Total effort savings (30-40% reduction via avoiding rework)
- ✅ Pro: Cleaner implementation (designed for dual-dataplane from the start)
- ✅ Decision: Foundation-first approach reduces long-term technical debt

---

## Technical Implementation Details

### Component Breakdown

**New Crates**:
1. `fraiseql-arrow` (NEW) - Arrow Flight server, schema generation, converters
2. `fraiseql-core` (MODIFIED) - Add SQL → Arrow RecordBatch conversion
3. `fraiseql-observers` (MODIFIED) - Add NATS → Arrow bridge, ClickHouse/ES sinks

**Infrastructure**:
- **Arrow Flight Server**: gRPC on port 50051 (alongside HTTP on 8080)
- **ClickHouse**: Analytics database (Docker: `clickhouse/clickhouse-server:24`)
- **Elasticsearch**: Search database (Docker: `elasticsearch:8.15.0`)
- **NATS**: Event streaming (existing, no changes)
- **Redis**: Caching/dedup (existing, no changes)

**Feature Flags**:
```toml
[features]
arrow = ["fraiseql-arrow"]        # Enable Arrow Flight
clickhouse = ["clickhouse-client"] # Enable ClickHouse sink
elasticsearch = ["elasticsearch"]  # Enable Elasticsearch indexer
```

**Backwards Compatibility**:
- ✅ 100% backwards compatible (HTTP/JSON unchanged)
- ✅ Arrow Flight is additive (feature flag)
- ✅ Clients opt-in (no forced migration)

**Security & Authentication**:
- Arrow Flight endpoints require **same auth model as HTTP/JSON** (JWT/API key)
- Credentials transmitted via **gRPC metadata** (standard practice)
- **TLS required in production**; local dev can run insecure for simplicity
- Authorization scope mirrors GraphQL permissions (row/org-level filters enforced server-side)
- mTLS optional for future enhancement (Phase 10+)

**Schema Evolution Strategy**:
- **ClickHouse**: Stable core columns + `Map(String, String)` or JSON column for flexible extensions
- **Arrow Flight**: Stable "core columns" in schema + `payload_json` or versioned struct fields
- Additive changes only (new columns/fields) to maintain replay compatibility
- Schema version tracking in event metadata for future migrations

---

## Performance Expectations

### Benchmarks (Conservative Estimates)

**Query Performance** (GraphQL → Arrow vs HTTP/JSON):

| Workload | HTTP/JSON (Baseline) | Arrow Flight (Target) | Improvement |
|----------|---------------------|----------------------|-------------|
| **GraphQL (1k rows)** | 200ms | 50ms | **4x** |
| **GraphQL (10k rows)** | 3s | 300ms | **10x** |
| **GraphQL (100k rows)** | 30s | 2-3s | **10-15x** |
| **GraphQL (1M rows)** | 5min | 10-20s | **15-30x** |
| **Memory (100k rows)** | 250MB | 50MB | **5x** (constant memory streaming) |

**Event Ingestion** (Observer Events → Databases):

| Metric | Baseline (Postgres only) | Target (ClickHouse + ES) | Notes |
|--------|--------------------------|--------------------------|-------|
| Throughput | 5-10k/sec | 50k-200k/sec (MVP) | Goal: 1M+/sec with batching + hardware |
| Latency (event → indexed) | N/A | <5 seconds (P95) | At 10k event/sec load |

### Throughput Targets

**Note**: Arrow Flight is the **query delivery plane** (serving results), not the ingestion path. Ingestion uses native database protocols.

**Ingestion Throughput** (Observer Events → NATS → Databases):
- **ClickHouse**: Target sustained high-throughput ingestion (goal 1M events/sec under batching and appropriate hardware)
  - MVP baseline: 50k-200k events/sec is already a significant win
  - Uses native ClickHouse HTTP bulk insert or clickhouse-rs client
  - Batch size: 10k events per insert (configurable)
- **Elasticsearch**: 50k+ events/sec (bulk API with 1k event batches)

**Query Throughput** (Arrow Flight serving results):
- **Arrow Flight streaming**: 100k+ rows/sec per client connection
- **Concurrent clients**: 100+ simultaneous Arrow Flight connections
- **gRPC streaming**: Chunked RecordBatch delivery (10k rows per batch)

---

## Risk Assessment

### Failure Modes & Degraded Operation

**Core Principle**: Writes must never fail due to ClickHouse or Elasticsearch outages.

**Expected Behavior**:

* **If ClickHouse is unavailable**:
  - Events are buffered/retried from NATS JetStream (durable queue)
  - Analytics queries fail fast with explicit "analytics backend unavailable" error (no silent failures)
  - Core GraphQL API (HTTP/JSON) remains fully operational
  - DLQ captures events that exceed retry limits for manual replay

* **If Elasticsearch is unavailable**:
  - Debug/search workflows degrade (Kibana/search unavailable)
  - Core GraphQL API remains fully operational
  - Events buffer in NATS for retry when Elasticsearch recovers

* **If Arrow Flight server crashes**:
  - HTTP/JSON API unaffected (independent transport)
  - Analytics clients gracefully degrade or failover to HTTP/JSON
  - Automatic restart via process manager (systemd/k8s)

* **If NATS is unavailable**:
  - Observer actions (webhooks/emails) pause (existing behavior)
  - PostgreSQL LISTEN/NOTIFY continues buffering
  - Automatic resume when NATS recovers

**Implementation**:
- DLQ + exponential backoff for sink consumers
- Health checks expose degraded state (monitoring/alerting)
- Event replay tooling for disaster recovery

### High Risk

**None identified.** Arrow Flight is mature (used by Snowflake, Databricks).

### Medium Risk

1. **Infrastructure Complexity**
   - **Risk**: Adding ClickHouse + Elasticsearch increases operational burden
   - **Mitigation**: Docker Compose for local dev, k8s manifests for prod, monitoring
   - **Impact**: Team needs to learn ClickHouse SQL (2-3 days)

2. **Storage Costs**
   - **Risk**: Dual dataplane = 2x storage
   - **Mitigation**: 90-day TTL, ClickHouse compression (10:1), Elasticsearch ILM
   - **Impact**: ~1.5x storage vs PostgreSQL-only (acceptable)

### Low Risk

3. **Arrow Flight Adoption**
   - **Risk**: Data scientists resist learning new client API
   - **Mitigation**: Comprehensive examples (Python, R), migration guide, zero-copy perf wins
   - **Impact**: Minimal (client code is simpler, not more complex)

4. **gRPC Port Management**
   - **Risk**: Port 50051 conflicts
   - **Mitigation**: Configurable port, standard Flight protocol convention
   - **Impact**: Negligible

---

## Success Metrics

### Performance (Quantitative)

**MVP Targets** (must achieve):
- ✅ 100k row query: < 3 seconds via Arrow Flight (vs 30s HTTP/JSON baseline) = **10x improvement**
- ✅ 50k-200k events/sec sustained ingestion to ClickHouse (vs 5-10k/sec baseline) = **10-20x improvement**
- ✅ < 100ms Elasticsearch search queries (P95) for debugging workflows
- ✅ < 500MB memory for 1M row stream (constant memory via batching)

**Stretch Goals** (Phase 9+):
- 🎯 1M+ events/sec sustained ingestion under batching and appropriate hardware
- 🎯 100k+ rows/sec streaming per Arrow Flight client
- 🎯 < 1 second for 100k row queries (vs 3s MVP target)

### Adoption (Qualitative)

- ✅ Python client example works out-of-box (< 30 min for data scientist)
- ✅ R client example works out-of-box
- ✅ ClickHouse dashboards show real-time metrics
- ✅ Elasticsearch incident response queries documented

### Production Readiness

- ✅ All integration tests passing (GraphQL, events, dual dataplane)
- ✅ Stress test: 1M rows in < 60 seconds
- ✅ Chaos test: System recovers from ClickHouse/ES failures
- ✅ Zero regressions in HTTP/JSON API

---

## Implementation Timeline

### Phase 9: Apache Arrow Flight (4-5 weeks)

**Week 1**: Foundation
- 9.1: Arrow Flight server, gRPC lifecycle, basic schemas

**Week 2**: GraphQL Integration
- 9.2: SQL → Arrow conversion, streaming RecordBatches

**Week 3**: Observer Events + Analytics
- 9.3: NATS → Arrow bridge, event streaming
- 9.4: ClickHouse integration (analytics dataplane)

**Week 4**: Operational + Clients
- 9.5: Elasticsearch integration (operational dataplane)
- 9.6: Client examples (Python, R, Rust)

**Week 5**: Validation
- 9.7: Integration tests, benchmarks, stress tests, chaos tests
- 9.8: Documentation, migration guide

### After Phase 9: Phase 8 Production Features (2-3 weeks)

- 8.7: Prometheus metrics (NATS + Arrow + ClickHouse + ES)
- 8.6: Job queue system
- 8.10: CLI tools (debug all transports)
- 8.12: Testing & QA (complete architecture)

**Total Time**: 6-8 weeks for complete Phase 9 + Phase 8

---

## Alternatives Considered (Summary)

### Alternative 1: Optimize HTTP/JSON Only
- **Pros**: No new infrastructure
- **Cons**: JSON serialization is fundamentally slow (can't get 50x improvement)
- **Decision**: Not viable for analytics use case

### Alternative 2: GraphQL Subscriptions for Events
- **Pros**: Use existing HTTP transport
- **Cons**: Not designed for bulk analytics, no columnar format, no ClickHouse integration
- **Decision**: Wrong tool for analytics

### Alternative 3: Parquet File Exports
- **Pros**: Columnar format, compatible with data science tools
- **Cons**: Batch-only (no streaming), requires S3/filesystem, slower than Arrow Flight
- **Decision**: Complementary (can add later), not a replacement

### Alternative 4: Single Dataplane (ClickHouse Only or Elasticsearch Only)
- **Pros**: Simpler (one database)
- **Cons**: ClickHouse bad at search, Elasticsearch bad at aggregations
- **Decision**: Dual dataplane plays to each system's strengths

---

## Recommendation

**Proceed with Phase 9 Arrow Flight integration** following the dual-dataplane architecture.

**Key Benefits**:
1. ✅ 50x analytics performance improvement
2. ✅ Unlock real-time event analytics (business intelligence)
3. ✅ Zero-copy integration with data science tools
4. ✅ Token-efficient implementation (Phase 9 before Phase 8)
5. ✅ 100% backwards compatible (HTTP/JSON unchanged)
6. ✅ Industry-standard approach (Arrow Flight + ClickHouse + Elasticsearch)

**Investment**: 4-5 weeks implementation + infrastructure (justified by performance gains)

**Risk Level**: Low-Medium (mature technologies, well-understood patterns)

---

## Questions for Review

1. **Dual Dataplane Approval**: Agree that ClickHouse + Elasticsearch serve different purposes (not redundant)?

2. **Sequencing Approval**: Agree to implement Phase 9 (foundation) before Phase 8 (production features) for token efficiency?

3. **Infrastructure Commitment**: Team ready to operate ClickHouse + Elasticsearch in production?

4. **Timeline Acceptable**: 4-5 weeks for Phase 9 + 2-3 weeks for Phase 8 = 6-8 weeks total?

5. **Performance Targets**: Are 50x query improvement and 1M+ events/sec sufficient justification?

---

## Approvals

- [ ] **Architecture Review**: Dual-dataplane design approved
- [ ] **Engineering Lead**: Implementation sequencing approved (Phase 9 → Phase 8)
- [ ] **DevOps/SRE**: Infrastructure requirements understood and resourced
- [ ] **Product**: Timeline and scope approved
- [ ] **Data Science Team**: Arrow Flight client API acceptable

---

**Document Version**: 1.0
**Last Updated**: January 24, 2026
**Next Review**: After Phase 9.1 completion (validate approach before full commitment)
