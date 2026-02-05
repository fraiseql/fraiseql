# Arrow Flight Architecture

## Overview

FraiseQL's Arrow Flight integration provides a **dual-dataplane architecture** optimized for different access patterns:

1. **Analytics Dataplane**: Arrow Flight → ClickHouse (facts, metrics, aggregations)
2. **Operational Dataplane**: HTTP/JSON → Elasticsearch (search, debugging)

Both dataplanes consume the same source data (NATS JetStream) and serve different purposes.

## Complete Data Flow

```
DATABASE WRITES
    ↓
┌─────────────────────────────────────────────────────────────┐
│                    Observer System                          │
│  PostgreSQL NOTIFY → NATS JetStream (durable, at-least-once)│
└─────────────┬───────────────────────────┬───────────────────┘
              │                           │
    ┌─────────▼──────────┐      ┌────────▼──────────┐
    │ Analytics Dataplane│      │ Operational       │
    │ (Arrow → ClickHouse)      │ Dataplane         │
    │                    │      │ (JSON → ES)       │
    ├────────────────────┤      ├───────────────────┤
    │ Arrow Bridge       │      │ JSONB Indexer    │
    │ (NATS → Arrow)     │      │ (NATS → JSON)     │
    │                    │      │                   │
    │ ClickHouse Sink    │      │ Elasticsearch     │
    │ (Batch insert)     │      │ Sink              │
    │ 1M+ events/sec     │      │ Bulk index        │
    │                    │      │                   │
    └────────────────────┘      └───────────────────┘
              │                           │
    ┌─────────▼────────────────┬────────▼──────────┐
    │  ClickHouse              │ Elasticsearch     │
    │  fraiseql_events table   │ fraiseql-events-* │
    │  (columnar, 90d TTL)     │ (JSONB, 90d ILM)  │
    └──────────────────────────┴───────────────────┘
              │                           │
    ┌─────────▼──────────────────────────▼──────────┐
    │         Arrow Flight Server (port 50051)      │
    │  Serves GraphQL results and Observer events   │
    └───────────┬────────────────────────┬──────────┘
                │                        │
    ┌───────────▼──────────┐   ┌────────▼──────────┐
    │  Arrow Flight Client │   │  Arrow Flight     │
    │  (Python/R/Java)     │   │  Client (Python)  │
    │  Analytics pipeline  │   │  Streaming events │
    │  ML feature eng      │   │  Real-time agg    │
    └──────────────────────┘   └───────────────────┘
```

## GraphQL Queries (Dual Transport)

Same GraphQL query, different transports:

```
┌──────────────────────────────────────────────────────┐
│  Client Request: "{ users { id name email } }"       │
└──────────────────────────────────────────────────────┘
        │
        ├─────────────────────────┬────────────────────┐
        │                         │                    │
        ▼                         ▼                    ▼
    HTTP:8080              Arrow Flight          Future: WebSocket
    (JSON)                 (gRPC, Binary)         (Server-sent events)
        │                         │                    │
    ┌───▼────────────┐      ┌────▼──────────┐    ┌────▼──────────┐
    │ HTTP Handler   │      │ Arrow Flight  │    │ WebSocket     │
    └───┬────────────┘      │ Handler       │    │ Handler       │
        │                   │               │    │               │
    ┌───▼────────────────────────────────────────────────────┐
    │         fraiseql-core                                 │
    │  1. Parse GraphQL                                     │
    │  2. Validate (permissions, schema)                   │
    │  3. Execute SQL                                       │
    └───┬────────────────────────┬──────────────────────────┘
        │                        │
    ┌───▼──────────┐      ┌─────▼────────────┐
    │ Row → JSON   │      │ Row → Arrow      │
    │ Serialization│      │ RecordBatch      │
    └───┬──────────┘      └─────┬────────────┘
        │                        │
    ┌───▼──────────────┐    ┌────▼──────────────┐
    │ HTTP Response    │    │ Arrow gRPC Stream │
    │ Content-Type:    │    │ Content-Type:     │
    │ application/json │    │ application/       │
    │ Size: 10MB       │    │ x-protobuf        │
    │ Time: 30s        │    │ Size: 1MB         │
    └──────────────────┘    │ Time: 2s          │
    (Web clients)           └───────────────────┘
                            (Analytics clients)
```

## Observer Events (Dual Sink)

Events flow through NATS to both dataplanes:

```
DATABASE MUTATION
    ↓
PostgreSQL NOTIFY (trigger-based)
    ↓
NATS JetStream (durable, at-least-once semantics)
    ├──► EntityEvent message (Rust struct)
    │    ├─ id: UUID
    │    ├─ event_type: enum (Created, Updated, Deleted)
    │    ├─ entity_type: string (Order, User, Product)
    │    ├─ entity_id: UUID
    │    ├─ timestamp: datetime
    │    ├─ data: JSON (arbitrary event data)
    │    ├─ user_id: string (who triggered it)
    │    └─ org_id: string (which org)
    │
    ├──► Arrow Bridge
    │    └─ Convert EntityEvent → Arrow RecordBatch
    │       ├─ 8-column schema
    │       ├─ Columnar format (efficient)
    │       └─ RecordBatch size: ~10k rows
    │
    ├──► ClickHouse Sink
    │    ├─ Batch events: 10k per insert
    │    ├─ Insert to fraiseql_events table
    │    └─ Materialized views update automatically
    │       ├─ fraiseql_events_hourly (aggregations)
    │       ├─ fraiseql_org_daily (org stats)
    │       └─ fraiseql_event_type_stats (distribution)
    │
    └──► Elasticsearch Sink
         ├─ Bulk index API (efficient)
         ├─ Index: fraiseql-events-YYYY.MM
         ├─ Document: JSONB serialized
         └─ ILM policy: hot → warm → delete (90d)
```

## Component Responsibilities

### fraiseql-arrow

- **Flight Server**: gRPC server implementing Apache Arrow Flight protocol
- **Schema Registry**: Generates Arrow schemas from GraphQL types
- **RecordBatch Streaming**: Converts SQL rows to Arrow columnar format
- **Ticket Encoding**: Encodes/decodes Flight ticket protocol

### fraiseql-core

- **Query Execution** (unchanged): Parse GraphQL, execute SQL
- **Row → Arrow Converter** (NEW): Converts database rows to Arrow RecordBatch
- **Row → JSON Converter** (unchanged): Existing HTTP/JSON path

### fraiseql-observers

- **NATS Integration** (unchanged): Event sourcing infrastructure
- **Arrow Bridge** (NEW): Converts EntityEvent → RecordBatch
- **ClickHouse Sink** (NEW): Batches and inserts to ClickHouse
- **Elasticsearch Sink** (NEW): Bulk indexes to Elasticsearch
- **Observer Executor** (unchanged): Actions (webhooks, emails, etc.)

## Why Two Dataplanes?

### Analytics Dataplane (Arrow Flight + ClickHouse)

**Optimized for**: Aggregations, time-series, ML pipelines

```
Use Cases:

- "How many orders per hour?" → Materialized views
- "Top 10 products by revenue?" → GROUP BY aggregations
- "Daily active users trend?" → Time-series aggregations
- "Extract features for ML model" → Arrow → NumPy/TensorFlow

Characteristics:

- Data format: Columnar binary (Arrow)
- Query language: SQL aggregations (SUM, COUNT, GROUP BY)
- Performance: 1M+ events/sec ingestion
- Retention: 90 days (TTL in ClickHouse)
- Clients: Python, R, Java (via Arrow libraries)
```

### Operational Dataplane (HTTP/JSON + Elasticsearch)

**Optimized for**: Full-text search, flexible filtering

```
Use Cases:

- "Find all failed orders with error_code PAYMENT_DECLINED"
- "Show me events for user-123 in the last hour"
- "Search all events containing 'refund'"
- "Incident response: all errors in past 10 minutes"

Characteristics:

- Data format: JSONB documents
- Query language: Elasticsearch DSL (match, term, range, bool)
- Performance: <100ms search queries
- Retention: 90 days (ILM policy)
- Clients: Kibana, web dashboards, support tools
```

## Example: Choose the Right Dataplane

| Question | Best Dataplane | Why |
|----------|---|---|
| "How many orders were created per hour this month?" | **ClickHouse** | Needs aggregations and time-series window functions |
| "Find all failed orders with PAYMENT_DECLINED" | **Elasticsearch** | Needs flexible text + term filtering |
| "What's the average order value by region?" | **ClickHouse** | Requires complex aggregations and GROUP BY |
| "Show me events for customer-123 in the last 24 hours" | **Elasticsearch** | Needs fast document retrieval with filtering |
| "Extract ML features from events" | **Arrow Flight** | Needs fast bulk data export to Python/R |
| "Build a real-time revenue dashboard" | **ClickHouse** | Materialized views update every second |

## Deployment Topologies

### Topology 1: HTTP-Only (Simple)
```
fraiseql-server (HTTP:8080)
    ↓
PostgreSQL
```

- **Best for**: Simple web applications
- **Trade-offs**: No Arrow Flight, no analytics benefits
- **Setup time**: 5 minutes
- **Infrastructure cost**: Minimal

### Topology 2: Dual Transport + Analytics (Recommended for Production)
```
fraiseql-server (HTTP:8080 + Arrow:50051)
    ↓
PostgreSQL
    ↓
NATS JetStream
    ├─→ ClickHouse (analytics)
    └─→ Elasticsearch (operational)
```

- **Best for**: Production applications with analytics needs
- **Trade-offs**: More infrastructure (but purpose-built)
- **Setup time**: 1-2 hours
- **Infrastructure cost**: $500-2000/month (adds ClickHouse + ES)
- **Performance gain**: 15-50x faster analytics queries

### Topology 3: Arrow-Only (Future)
```
fraiseql-server (Arrow:50051)
    ↓
PostgreSQL
```

- **Best for**: Pure analytics workloads
- **Trade-offs**: No web client support
- **Status**: Not yet implemented

## Performance Characteristics

### GraphQL Query Performance

| Rows | HTTP/JSON | Arrow Flight | Delta | Benefit |
|---|---|---|---|---|
| 100 | 50ms | 10ms | -40ms | Negligible |
| 1,000 | 200ms | 50ms | -150ms | Small |
| 10,000 | 3s | 300ms | -2.7s | Significant |
| 100,000 | 30s | 2s | -28s | **Major** ⚡ |
| 1,000,000 | 5min | 10s | -290s | **Transformational** ⚡⚡ |

**Key Insight**: Arrow Flight benefit increases with dataset size. Use Arrow for queries returning 10k+ rows.

### Observer Events Streaming

| Metric | Value |
|---|---|
| **Ingestion Throughput** | 1M+ events/sec |
| **Arrow → RecordBatch** | <10ms conversion |
| **RecordBatch → ClickHouse** | Batch of 10k, insert <50ms |
| **Memory (streaming)** | Constant (10k × row_width) |
| **Memory (buffering)** | O(total_events) - avoid! |

### Resource Usage

| Component | CPU | Memory | Notes |
|---|---|---|---|
| **Arrow Flight Server** | 2-3 threads | <100MB | Minimal, minimal overhead |
| **ClickHouse Sink** | Low | <500MB | Batches events, efficient |
| **Elasticsearch Sink** | Low | <200MB | Bulk API, efficient |
| **NATS JetStream** | Low | Variable | Depends on retention policy |

## Security Considerations

### Authentication

- Current: Open (for Phase 9, suitable for internal networks)
- Phase 10: gRPC mTLS for Arrow Flight (mutual TLS)
- Phase 10: Same JWT validation as HTTP/JSON API

### Authorization

- Arrow Flight inherits GraphQL permissions
- Same role-based access control (RBAC) applies
- Query still validated before Arrow conversion

### Network

- **Recommendation**: Arrow Flight should be internal-only
  - Not exposed to public internet
  - Bind to internal network interface
  - Use VPN or private networks
- HTTPS/TLS added in Phase 10

### Encryption

- **In Transit**: Will add TLS in Phase 10
- **At Rest**: ClickHouse/Elasticsearch handle encryption
- **Data**: No sensitive data in Arrow batches (just query results)

## Known Limitations

- ✅ Arrow Flight server available
- ✅ GraphQL queries work
- ✅ Observer events streaming works
- ❌ Authentication: Not yet implemented
- ❌ Authorization: Not yet implemented
- ❌ TLS: Not yet implemented
- ❌ Rate limiting: Not yet implemented

## Phase Roadmap

| Phase | Feature | Status |
|---|---|---|
| 9.1 | Arrow Flight Foundation | ✅ Complete |
| 9.2 | GraphQL → Arrow Conversion | ✅ Complete |
| 9.3 | Observer Events → Arrow Bridge | ✅ Complete |
| 9.4 | ClickHouse Analytics Sink | ✅ Complete |
| 9.5 | Elasticsearch Operational Sink | ✅ Complete |
| 9.6 | Client Examples (Python/R/Rust) | ✅ Complete |
| 9.7 | Integration & Performance Testing | ✅ Complete |
| 9.8 | Documentation & Migration (This Phase) | 🔄 In Progress |
| 10 | Production Hardening (Auth, TLS, Rate Limit) | 📋 Planned |

---

## Troubleshooting

### "Arrow Flight server not listening on port 50051"

**Cause:** Server not started or bound to different port.

**Diagnosis:**
1. Check if process is running: `ps aux | grep fraiseql`
2. Verify port: `netstat -tuln | grep 50051`
3. Check logs: `docker logs fraiseql | grep "Arrow Flight"`

**Solutions:**
- Start FraiseQL with Arrow Flight enabled: `fraiseql --enable-arrow-flight`
- Check fraiseql.toml for port configuration: `[arrow_flight] bind_port = 50051`
- Verify firewall allows port 50051
- Try different port if 50051 in use: `[arrow_flight] bind_port = 50052`

### "Client connection refused: 'Connection refused on 127.0.0.1:50051'"

**Cause:** Server listening on different host or client connecting to wrong address.

**Diagnosis:**
1. Check server bind address: `grep bind_address /path/to/fraiseql.toml`
2. Verify server is accessible: `telnet localhost 50051`
3. Check if client using correct host/port: `nc -zv host.com 50051`

**Solutions:**
- Ensure server bound to 0.0.0.0 for remote access: `bind_address = "0.0.0.0:50051"`
- For Docker: map port in docker-compose: `ports: ["50051:50051"]`
- Use correct hostname: `arrow.example.com` not just `localhost`
- For Kubernetes: create Service exposing port 50051

### "Arrow schema mismatch: received schema doesn't match expected"

**Cause:** Schema definition changed between client and server.

**Diagnosis:**
1. Print server schema: Server logs schema on startup
2. Print client schema: `print(flight_client.get_flight_info(...))`
3. Compare schemas - look for missing/renamed fields

**Solutions:**
- Regenerate client code if schema changed
- Restart both client and server with matching schema versions
- Use schema versioning in Arrow metadata
- Check schema.compiled.json wasn't modified unexpectedly

### "Arrow Flight query returns empty result or wrong data"

**Cause:** Query execution issue or schema mismatch.

**Diagnosis:**
1. Verify query works in HTTP/JSON: `curl -X POST http://localhost:8000/graphql`
2. Compare Arrow vs JSON results row counts
3. Check if filtering applies to Arrow plane

**Solutions:**
- Arrow queries go through same execution engine as JSON
- Verify data exists in database
- Check WHERE clause applies correctly in Arrow context
- Enable query logging to see actual SQL executed

### "Arrow Flight performance not better than HTTP/JSON"

**Cause:** Small dataset or not utilizing columnar advantages.

**Diagnosis:**
1. Check result set size: `EXPLAIN SELECT ...` shows row count
2. Measure actual performance: Time both endpoints
3. Look for network bottleneck vs data processing

**Solutions:**
- Arrow Flight benefits with large datasets (>10K rows)
- For small results, HTTP/JSON is fine
- Use ClickHouse for analytics (10-100x faster aggregations)
- Verify network between client and server (gRPC more efficient than HTTP)
- Check if result is CPU-bound or I/O-bound

### "ClickHouse integration: data not appearing in analytics table"

**Cause:** Observer not configured to send to ClickHouse or ingestion failing.

**Diagnosis:**
1. Check Observer configuration: `grep "ClickHouse" fraiseql.toml`
2. Verify ClickHouse is running: `curl http://localhost:8123/ping`
3. Check if mutations are triggering observers: `SELECT COUNT(*) FROM system.events WHERE event = 'InsertedRows';`

**Solutions:**
- Configure Observer to output to ClickHouse
- Verify ClickHouse table exists and schema matches
- Check network connectivity between FraiseQL and ClickHouse
- Review ClickHouse logs for insert errors

### "Elasticsearch integration: query debugging logs not appearing"

**Cause:** Elasticsearch not configured or debug logging disabled.

**Diagnosis:**
1. Check Elasticsearch running: `curl http://localhost:9200/_cluster/health`
2. Verify debug index exists: `curl http://localhost:9200/_cat/indices | grep debug`
3. Check logging level in fraiseql.toml: `[logging] level = "debug"`

**Solutions:**
- Enable debug logging: Set `level = "debug"` in fraiseql.toml
- Ensure Elasticsearch configured in fraiseql.toml
- Create debug index if missing: `curl -X PUT http://localhost:9200/debug-logs`
- Verify network allows queries from FraiseQL to Elasticsearch port 9200

### "Authentication/TLS not working in Arrow Flight"

**Cause:** These features not yet implemented (known limitation).

**Diagnosis:**
1. Confirm from Known Limitations section above
2. Check FraiseQL version: `fraiseql --version` (should indicate phase)

**Current Workaround:**
- Use Arrow Flight only in trusted internal networks
- Implement TLS at reverse proxy layer (nginx, Envoy)
- Use API key validation at proxy level
- Scheduled for Phase 10 hardening

---

**Next**: [Getting Started Tutorial](./getting-started.md)
