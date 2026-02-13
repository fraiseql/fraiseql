# Metrics & Observability Guide

fraiseql-wire provides comprehensive metrics collection for production observability. All metrics are compatible with **Prometheus**, **OpenTelemetry**, and other standard metrics collection systems.

---

## Quick Start

Enable metrics collection by accessing the `metrics` module:

```rust
use fraiseql_wire::metrics;

// Metrics are automatically recorded by fraiseql-wire
// No explicit configuration needed - just use the client normally
```

Metrics are emitted via the `metrics` crate using standard instrumentation, which can be collected by any compatible backend (Prometheus, OpenTelemetry, Grafana, etc.).

---

## Metrics Overview

fraiseql-wire records **17 metrics** across 6 query execution stages:

| Stage | Counters | Histograms | Purpose |
|-------|----------|-----------|---------|
| **Submission** | 1 | 0 | Query requests |
| **Authentication** | 3 | 1 | Auth mechanism, duration, success/failure |
| **Startup** | 0 | 1 | Time to first result |
| **Row Processing** | 2 | 3 | Chunks, errors, timing |
| **Filtering** | 1 | 1 | Rust predicate performance |
| **Deserialization** | 2 | 1 | Per-type latency and errors |

---

## Counter Metrics

Counters track events that only increase over time.

### Query Submission

#### `fraiseql_queries_total`

- **Type**: Counter
- **Labels**: `entity`, `has_where_sql`, `has_where_rust`, `has_order_by`
- **Purpose**: Track query submissions with predicate details
- **Example**: Count queries by entity and predicate type
- **Cardinality**: Low (entities × 2³ predicates)

```rust
// Recorded when QueryBuilder::execute() is called
fraiseql_queries_total{entity="users", has_where_sql="true", has_where_rust="false", has_order_by="false"} 42
```

---

### Authentication

#### `fraiseql_authentications_total`

- **Type**: Counter
- **Labels**: `mechanism`
- **Values**: `cleartext`, `scram`
- **Purpose**: Count authentication attempts by mechanism
- **Use Case**: Monitor auth mechanism usage patterns

```rust
fraiseql_authentications_total{mechanism="scram"} 1250
fraiseql_authentications_total{mechanism="cleartext"} 15
```

#### `fraiseql_authentications_successful_total`

- **Type**: Counter
- **Labels**: `mechanism`
- **Purpose**: Count successful authentications
- **Use Case**: Calculate auth success rate: `successful / total`

```rust
fraiseql_authentications_successful_total{mechanism="scram"} 1248
fraiseql_authentications_successful_total{mechanism="cleartext"} 15
```

#### `fraiseql_authentications_failed_total`

- **Type**: Counter
- **Labels**: `mechanism`, `reason`
- **Values (reason)**: `invalid_password`, `server_error`, `timeout`
- **Purpose**: Track authentication failures
- **Use Case**: Alert on auth failure patterns

```rust
fraiseql_authentications_failed_total{mechanism="scram", reason="invalid_password"} 2
fraiseql_authentications_failed_total{mechanism="scram", reason="server_error"} 1
```

---

### Query Execution

#### `fraiseql_query_completed_total`

- **Type**: Counter
- **Labels**: `entity`, `status`
- **Values (status)**: `success`, `error`, `cancelled`
- **Purpose**: Track query completion by outcome
- **Use Case**: Query success rate dashboard

```rust
fraiseql_query_completed_total{entity="users", status="success"} 9875
fraiseql_query_completed_total{entity="users", status="error"} 22
fraiseql_query_completed_total{entity="users", status="cancelled"} 3
```

#### `fraiseql_query_error_total`

- **Type**: Counter
- **Labels**: `entity`, `error_category`
- **Values (error_category)**: `server_error`, `protocol_error`, `connection_error`, `json_parse_error`
- **Purpose**: Categorize query failures
- **Use Case**: Error debugging and alerting

```rust
fraiseql_query_error_total{entity="projects", error_category="json_parse_error"} 5
fraiseql_query_error_total{entity="projects", error_category="server_error"} 2
```

#### `fraiseql_json_parse_errors_total`

- **Type**: Counter
- **Labels**: `entity`
- **Purpose**: Track JSON deserialization failures at row level
- **Use Case**: Data quality monitoring

```rust
fraiseql_json_parse_errors_total{entity="events"} 7
```

---

### Row Processing

#### `fraiseql_rows_filtered_total`

- **Type**: Counter
- **Labels**: `entity`
- **Purpose**: Count rows removed by Rust predicates
- **Use Case**: Monitor filter effectiveness

```rust
fraiseql_rows_filtered_total{entity="tasks"} 320
```

---

### Deserialization

#### `fraiseql_rows_deserialized_total`

- **Type**: Counter
- **Labels**: `entity`, `type_name`
- **Purpose**: Count successful type conversions
- **Use Case**: Per-type deserialization tracking

```rust
fraiseql_rows_deserialized_total{entity="users", type_name="User"} 5000
fraiseql_rows_deserialized_total{entity="users", type_name="serde_json::Value"} 150
```

#### `fraiseql_rows_deserialization_failed_total`

- **Type**: Counter
- **Labels**: `entity`, `type_name`, `reason`
- **Values (reason)**: `serde_error`, `missing_field`, `type_mismatch`
- **Purpose**: Track deserialization failures per type
- **Use Case**: Type compatibility validation

```rust
fraiseql_rows_deserialization_failed_total{entity="users", type_name="User", reason="missing_field"} 3
fraiseql_rows_deserialization_failed_total{entity="users", type_name="User", reason="type_mismatch"} 1
```

---

## Histogram Metrics

Histograms track distributions of values (timing, sizes, counts).

### Query Timing

#### `fraiseql_query_startup_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `entity`
- **Purpose**: Time from query submit to first DataRow
- **Use Case**: Monitor query planning latency
- **Typical Range**: 1-500ms

```rust
// Recorded when first DataRow is received
fraiseql_query_startup_duration_ms_bucket{entity="users", le="10"} 500
fraiseql_query_startup_duration_ms_bucket{entity="users", le="100"} 2100
fraiseql_query_startup_duration_ms_bucket{entity="users", le="500"} 2200
```

#### `fraiseql_query_total_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `entity`
- **Purpose**: Total time from query start to completion
- **Use Case**: SLO monitoring
- **Typical Range**: 10-5000ms

```rust
fraiseql_query_total_duration_ms_bucket{entity="projects", le="100"} 300
fraiseql_query_total_duration_ms_bucket{entity="projects", le="1000"} 8500
fraiseql_query_total_duration_ms_bucket{entity="projects", le="5000"} 9000
```

---

### Row Processing

#### `fraiseql_chunk_processing_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `entity`
- **Purpose**: Time to process each chunk of rows
- **Use Case**: Identify backpressure and bottlenecks
- **Typical Range**: 1-100ms

```rust
fraiseql_chunk_processing_duration_ms_bucket{entity="events", le="10"} 500
fraiseql_chunk_processing_duration_ms_bucket{entity="events", le="50"} 1200
fraiseql_chunk_processing_duration_ms_bucket{entity="events", le="100"} 1300
```

#### `fraiseql_chunk_size_rows`

- **Type**: Histogram (row count)
- **Labels**: `entity`
- **Purpose**: Distribution of rows per chunk
- **Use Case**: Monitor streaming buffer efficiency
- **Typical Range**: 1-1024 rows

```rust
fraiseql_chunk_size_rows_bucket{entity="users", le="64"} 500
fraiseql_chunk_size_rows_bucket{entity="users", le="256"} 980
fraiseql_chunk_size_rows_bucket{entity="users", le="1024"} 1000
```

---

### Filtering & Deserialization

#### `fraiseql_filter_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `entity`
- **Purpose**: Per-row Rust filter execution time
- **Use Case**: Identify slow predicates
- **Typical Range**: 0.01-10ms

```rust
fraiseql_filter_duration_ms_bucket{entity="tasks", le="0.1"} 8000
fraiseql_filter_duration_ms_bucket{entity="tasks", le="1"} 9500
fraiseql_filter_duration_ms_bucket{entity="tasks", le="10"} 10000
```

#### `fraiseql_deserialization_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `entity`, `type_name`
- **Purpose**: Per-row deserialization latency by type
- **Use Case**: Type performance comparison
- **Typical Range**: 0.1-50ms

```rust
fraiseql_deserialization_duration_ms_bucket{entity="users", type_name="User", le="1"} 4500
fraiseql_deserialization_duration_ms_bucket{entity="users", type_name="User", le="10"} 4990
fraiseql_deserialization_duration_ms_bucket{entity="users", type_name="User", le="50"} 5000
```

#### `fraiseql_auth_duration_ms`

- **Type**: Histogram (milliseconds)
- **Labels**: `mechanism`
- **Purpose**: Authentication latency by mechanism
- **Use Case**: Auth performance comparison
- **Typical Range**: 5-500ms

```rust
fraiseql_auth_duration_ms_bucket{mechanism="scram", le="50"} 100
fraiseql_auth_duration_ms_bucket{mechanism="scram", le="100"} 1200
fraiseql_auth_duration_ms_bucket{mechanism="scram", le="500"} 1250
```

---

## Cardinality Considerations

All metrics use **low-cardinality labels** to prevent metrics explosion:

| Label | Cardinality | Examples |
|-------|-------------|----------|
| `entity` | Low (~10-100) | `users`, `projects`, `tasks` |
| `mechanism` | Very Low (2) | `cleartext`, `scram` |
| `status` | Very Low (3) | `success`, `error`, `cancelled` |
| `error_category` | Low (~5) | `server_error`, `protocol_error`, etc. |
| `type_name` | Medium (~5-50) | `User`, `Project`, `serde_json::Value` |
| `reason` | Low (~5-10) | `missing_field`, `type_mismatch`, etc. |

**Best Practice**: Keep entity count reasonable. High cardinality entities will impact storage/memory.

---

## Query Execution Flow & Metrics

### Successful Query

```
User Application
    ↓
QueryBuilder::execute()
    → fraiseql_queries_total{entity="users", has_where_sql="true", ...}
    ↓
Connection::authenticate()
    → fraiseql_authentications_total{mechanism="scram"}
    → fraiseql_auth_duration_ms{mechanism="scram"}
    → fraiseql_authentications_successful_total{mechanism="scram"}
    ↓
Connection::streaming_query() - Startup
    → fraiseql_query_startup_duration_ms{entity="users"}
    ↓
Background Task - Process 5 chunks of 256 rows each
    → fraiseql_chunk_size_rows{entity="users"}
    → fraiseql_chunk_processing_duration_ms{entity="users"}
    ↓
FilteredStream - Apply Rust predicates
    → fraiseql_filter_duration_ms{entity="users"}
    → fraiseql_rows_filtered_total{entity="users"}
    ↓
TypedJsonStream - Deserialize to User struct
    → fraiseql_deserialization_duration_ms{entity="users", type_name="User"}
    → fraiseql_rows_deserialized_total{entity="users", type_name="User"}
    ↓
Query Complete
    → fraiseql_rows_processed_total{entity="users", status="ok"}
    → fraiseql_query_total_duration_ms{entity="users"}
    → fraiseql_query_completed_total{entity="users", status="success"}
    ↓
Consumer Application (1280 rows)
```

### Error Scenario

```
QueryBuilder::execute()
    → fraiseql_queries_total{entity="projects", ...}
    ↓
Connection::authenticate()
    → fraiseql_authentications_total{mechanism="scram"}
    ↓
Query starts, processes 256 rows successfully
    → fraiseql_chunk_size_rows{entity="projects"}
    ↓
JSON Parse Error on row 257
    → fraiseql_json_parse_errors_total{entity="projects"}
    ↓
Query Error Recorded
    → fraiseql_query_error_total{entity="projects", error_category="json_parse_error"}
    → fraiseql_rows_processed_total{entity="projects", status="error"}
    → fraiseql_query_total_duration_ms{entity="projects"}
    → fraiseql_query_completed_total{entity="projects", status="error"}
```

---

## Integration Examples

### Prometheus

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'fraiseql-wire'
    static_configs:
      - targets: ['localhost:9090']  # Metrics exporter endpoint
```

**Query successful queries per entity:**

```promql
sum(increase(fraiseql_query_completed_total{status="success"}[5m])) by (entity)
```

**Query error rate:**

```promql
sum(increase(fraiseql_query_completed_total{status="error"}[5m])) by (entity)
/
sum(increase(fraiseql_query_completed_total[5m])) by (entity)
```

**P95 query latency:**

```promql
histogram_quantile(0.95, fraiseql_query_total_duration_ms)
```

---

### OpenTelemetry

Metrics are compatible with OpenTelemetry metric exporters. Use the `opentelemetry-prometheus` crate to export:

```rust
use opentelemetry::global;
use opentelemetry_prometheus::PrometheusExporter;

let exporter = PrometheusExporter::new(Default::default(), global::meter_provider())?;
// Now metrics will be exported to OpenTelemetry
```

---

### Grafana Dashboards

Example Grafana panels:

**Query Success Rate (over 5m)**

```
sum(increase(fraiseql_query_completed_total{status="success"}[5m])) by (entity)
/
sum(increase(fraiseql_query_completed_total[5m])) by (entity)
```

**P99 Query Latency**

```
histogram_quantile(0.99, rate(fraiseql_query_total_duration_ms_sum[5m]) / rate(fraiseql_query_total_duration_ms_count[5m]))
```

**Error Rate by Category**

```
sum(increase(fraiseql_query_error_total[5m])) by (error_category)
```

---

## Performance Impact

All metrics recording has minimal overhead:

- **Counter increment**: ~0.1μs (atomic operation)
- **Histogram recording**: ~1μs (allocation-free)
- **Total per-query overhead**: < 0.1% for typical workloads
- **No blocking**: Lock-free atomic operations
- **No allocations**: In hot paths

---

## Advanced Topics

### Custom Metrics Integration

To add custom metrics alongside fraiseql-wire metrics:

```rust
use metrics::counter;

// Your custom metrics
counter!("myapp_requests_total", "endpoint" => "/api/users").increment(1);

// fraiseql-wire metrics (automatically recorded)
let stream = client.query::<User>("users").execute().await?;
```

### Metrics with Service Mesh

Metrics work with service meshes (Istio, Linkerd):

1. Export metrics to Prometheus
2. Service mesh scrapes Prometheus endpoint
3. Metrics appear in mesh observability dashboard

### Alert Rules

Example alert rule (for Prometheus):

```yaml
- alert: HighErrorRate
  expr: >
    (sum(increase(fraiseql_query_completed_total{status="error"}[5m])) by (entity)
    /
    sum(increase(fraiseql_query_completed_total[5m])) by (entity))
    > 0.05
  for: 5m
  annotations:
    summary: "High error rate for {{ $labels.entity }}"
```

---

## Summary

fraiseql-wire provides comprehensive, low-overhead observability across the entire query pipeline:

- ✅ 17 metrics across 6 execution stages
- ✅ Low cardinality labels prevent metric explosion
- ✅ < 0.1% overhead for typical workloads
- ✅ Compatible with Prometheus, OpenTelemetry, Grafana
- ✅ No configuration needed - automatic collection

Start monitoring your fraiseql-wire queries today!
