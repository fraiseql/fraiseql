# Phase 9: Apache Arrow Flight Integration

**Date**: January 24, 2026
**Duration**: 4-5 weeks
**Priority**: ⭐⭐⭐⭐⭐ Strategic Foundation

---

## Executive Summary

Phase 9 establishes **Apache Arrow Flight as FraiseQL's high-performance data delivery layer**, enabling 50x faster analytics queries, zero-copy cross-language integration, and direct data warehouse connectivity.

### Vision: Two Complementary Dataplanes

```
┌─────────────────────────────────────────────────────────────┐
│                  FraiseQL Dual Dataplane                     │
└─────────────────────────────────────────────────────────────┘

Analytics Dataplane (NEW)              Operational Dataplane (Existing)
────────────────────────                ──────────────────────────────
Arrow Flight + ClickHouse               JSONB + Elasticsearch
- High-throughput analytics             - Fast human-facing search
- Streaming aggregations                - Flexible JSON querying
- Time-series metrics                   - Incident response
- Bulk exports (1M+ rows/sec)           - Debugging workflows
- Zero-copy to Python/R                 - Request/event inspection

Use cases:                              Use cases:
✓ Business intelligence dashboards      ✓ "Find events where X happened"
✓ Real-time monitoring                  ✓ Debug failed GraphQL requests
✓ Data science pipelines                ✓ Search error logs
✓ ML feature extraction                 ✓ Support ticket investigation
✓ Compliance reporting                  ✓ Audit trail search
```

**Key Principle**: Avoid redundant storage by keeping responsibilities clear.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│                    fraiseql-server                           │
│                  (HTTP + gRPC endpoints)                     │
└────────────┬─────────────────────────────┬───────────────────┘
             │                             │
             ▼                             ▼
   ┌─────────────────┐          ┌─────────────────────┐
   │  HTTP/JSON API  │          │ Arrow Flight Server │
   │  (GraphQL)      │          │ (gRPC + Arrow)      │
   └────────┬────────┘          └──────────┬──────────┘
            │                              │
            │                              │
            ▼                              ▼
   ┌─────────────────────────────────────────────────┐
   │           fraiseql-core (Execution)             │
   │  ┌─────────────────────────────────────────┐   │
   │  │  Query Executor                         │   │
   │  │  - SQL execution                        │   │
   │  │  - Row → JSON (existing)                │   │
   │  │  - Row → Arrow RecordBatch (NEW)        │   │
   │  └─────────────────────────────────────────┘   │
   └──────┬──────────────────────────────┬───────────┘
          │                              │
          ▼                              ▼
   ┌──────────────┐              ┌──────────────────┐
   │ Elasticsearch│              │   ClickHouse     │
   │ (Operational)│              │   (Analytics)    │
   └──────────────┘              └──────────────────┘
          │                              │
          ▼                              ▼
   "Find the thing"              "Analyze patterns"
   Fast search/debug             Aggregations/metrics
```

---

## Phase Breakdown

### Week 1: Foundation
- **Phase 9.1**: Arrow Flight Foundation (5-7 days)
  - New `fraiseql-arrow` crate
  - Flight server trait + gRPC lifecycle
  - Basic DoGet/DoPut/GetSchema/ListFlights
  - Unit tests + benchmarks

### Week 2: GraphQL Integration
- **Phase 9.2**: GraphQL Results → Arrow Conversion (5-7 days)
  - SQL Row → Arrow RecordBatch converter
  - GraphQL type → Arrow schema mapping
  - Streaming result batches
  - Null handling + nested objects

### Week 3: Observer Events + ClickHouse
- **Phase 9.3**: Observer Events → Arrow Streaming (5-7 days)
  - EntityEvent → Arrow schema
  - NATS → Arrow Flight bridge
  - Event batching (10k events/batch)
  - Backpressure handling

- **Phase 9.4**: ClickHouse Integration (3-4 days)
  - ClickHouse Arrow Flight sink
  - MergeTree table creation
  - Materialized views for aggregations
  - Retention policies

### Week 4: Elasticsearch + Client Examples
- **Phase 9.5**: Elasticsearch Integration (3-4 days)
  - Elasticsearch JSONB sink (parallel to ClickHouse)
  - Index templates for events/requests
  - Search queries for debugging
  - Retention policies

- **Phase 9.6**: Cross-Language Client Examples (2-3 days)
  - Python client (PyArrow + Polars)
  - R client (arrow package)
  - Rust client example
  - ClickHouse direct consumption

### Week 5: Testing + Documentation
- **Phase 9.7**: Integration & Performance Testing (3-4 days)
  - End-to-end pipeline tests
  - Performance benchmarks (HTTP vs Arrow)
  - Stress testing (1M+ rows)
  - Chaos testing (ClickHouse/ES failures)

- **Phase 9.8**: Documentation & Migration Guide (2-3 days)
  - Architecture documentation
  - API reference
  - Client integration guides
  - Migration from HTTP-only to dual-dataplane

---

## Performance Targets

| Metric | Target | Current (HTTP/JSON) | Improvement |
|--------|--------|---------------------|-------------|
| **Large result sets** (100k rows) | 2 seconds | 30 seconds | **15x** |
| **Bulk exports** (1M rows) | 10 seconds | 5 minutes | **30x** |
| **Streaming throughput** | 1M+ events/sec | 50k events/sec | **20x** |
| **Memory efficiency** | 50 MB | 250 MB | **5x** |
| **Cross-language latency** | < 1 ms (zero-copy) | 100+ ms (serialization) | **100x** |

---

## Success Criteria

### Analytics Dataplane (Arrow Flight + ClickHouse)
- ✅ GraphQL query results stream via Arrow Flight
- ✅ ClickHouse consumes Arrow streams (zero-copy)
- ✅ Python/R clients consume Arrow (zero-copy)
- ✅ 50x faster than HTTP/JSON for 100k+ row queries
- ✅ 1M+ events/sec streaming to ClickHouse

### Operational Dataplane (JSONB + Elasticsearch)
- ✅ Observer events indexed in Elasticsearch
- ✅ GraphQL request logs searchable
- ✅ Fast full-text search (< 100ms for typical queries)
- ✅ Flexible JSON querying for debugging
- ✅ Incident response workflows supported

### Integration
- ✅ Both dataplanes operate independently
- ✅ No redundant storage (clear separation of concerns)
- ✅ Client can choose dataplane based on use case
- ✅ All tests passing (unit + integration + performance)
- ✅ Zero regressions in existing HTTP/JSON API

---

## Dependencies

### New Rust Crates
```toml
[dependencies]
arrow = "53"              # Arrow data structures
arrow-flight = "53"       # Flight RPC
arrow-schema = "53"       # Schema definitions
tonic = "0.12"            # gRPC framework
prost = "0.13"            # Protocol buffers
clickhouse = "0.12"       # ClickHouse client
elasticsearch = "8.15"    # Elasticsearch client
```

### Infrastructure
- **ClickHouse**: Analytics database (Docker: `clickhouse/clickhouse-server:24`)
- **Elasticsearch**: Search/debugging database (Docker: `elasticsearch:8.15.0`)
- **NATS**: Event streaming (existing)
- **Redis**: Caching/dedup (existing)

---

## Token Efficiency Rationale

### Why Phase 9 Before Phase 8 Production Features?

**Scenario 1: Phase 8 First → Phase 9 Later** (INEFFICIENT)
```
1. Implement Prometheus metrics for NATS-only
2. Implement CLI tools for NATS debugging
3. Implement tests for current architecture
4. Implement Elasticsearch for events
5. --- Add Arrow Flight ---
6. Update metrics for Arrow Flight
7. Update CLI tools for Arrow Flight debugging
8. Update tests for dual-dataplane
9. Add ClickHouse (now redundant with Elasticsearch?)

Total: ~8-9 weeks, duplicate work on steps 6-8
```

**Scenario 2: Phase 9 First → Phase 8 Later** (EFFICIENT)
```
1. Implement Arrow Flight foundation
2. Implement dual-dataplane (ClickHouse + Elasticsearch)
3. Implement metrics for NATS + Arrow + ClickHouse + ES (once)
4. Implement CLI tools for all transports (once)
5. Implement tests for complete architecture (once)

Total: ~6-7 weeks, no duplicate work
```

**Token Savings**: ~30-40% across Phase 8 features.

---

## Phase-Specific Plans

Each subphase has a detailed implementation plan in this directory:

1. **[phase-9.1-arrow-flight-foundation.md](./phase-9.1-arrow-flight-foundation.md)** - Week 1
2. **[phase-9.2-graphql-to-arrow.md](./phase-9.2-graphql-to-arrow.md)** - Week 2
3. **[phase-9.3-observer-events-arrow.md](./phase-9.3-observer-events-arrow.md)** - Week 3
4. **[phase-9.4-clickhouse-integration.md](./phase-9.4-clickhouse-integration.md)** - Week 3-4
5. **[phase-9.5-elasticsearch-integration.md](./phase-9.5-elasticsearch-integration.md)** - Week 4
6. **[phase-9.6-client-examples.md](./phase-9.6-client-examples.md)** - Week 4
7. **[phase-9.7-integration-testing.md](./phase-9.7-integration-testing.md)** - Week 5
8. **[phase-9.8-documentation.md](./phase-9.8-documentation.md)** - Week 5

---

## Next Steps

**Start with Phase 9.1**: [Arrow Flight Foundation](./phase-9.1-arrow-flight-foundation.md)

This establishes the gRPC server infrastructure and basic Flight RPC methods. Low-risk, validates the approach before committing to the full 4-5 weeks.

---

## Key Principles

1. **Dual Dataplane, Not Redundant Storage**
   - ClickHouse = Analytics facts/metrics at scale
   - Elasticsearch = Searchable documents for debugging

2. **Foundation First, Production Features Second**
   - Complete architectural changes before building on top
   - Avoid rework by implementing metrics/CLI/testing once

3. **Zero-Copy Where Possible**
   - Arrow Flight enables zero-copy to Python/R/Java
   - Massive performance gain for data science workflows

4. **Backwards Compatibility**
   - HTTP/JSON API remains fully functional
   - Clients opt-in to Arrow Flight when beneficial

5. **Clear Use Case Separation**
   - Analytics queries → Arrow Flight
   - Operational searches → HTTP/JSON + Elasticsearch
   - Let the client choose based on their needs

---

**Ready to begin? Start with [Phase 9.1: Arrow Flight Foundation →](./phase-9.1-arrow-flight-foundation.md)**
