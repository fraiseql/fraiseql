# FraiseQL Observability - Quick Reference

## One-Minute Setup

```bash
# Start with Docker Compose
docker-compose up -d

# Verify endpoints
curl http://localhost:8000/health       # Health check
curl http://localhost:8000/metrics      # Prometheus metrics
curl http://localhost:8000/metrics/json # JSON metrics

# Open dashboards
open http://localhost:3000   # Grafana
open http://localhost:9090   # Prometheus
```

## Common Tasks

### Access Metrics

```bash
# Prometheus text format (for scraping)
curl http://localhost:8000/metrics

# JSON format (for APIs)
curl http://localhost:8000/metrics/json | jq .

# Specific metrics
curl http://localhost:8000/metrics | grep fraiseql_graphql
```

### Check Health

```bash
# Health check endpoint
curl http://localhost:8000/health

# Response includes:
# - status: "ok" or "error"
# - database_connected: true/false
# - uptime_seconds: number
# - version: string
```

### View Server Logs

```bash
# Real-time logs with debug level
RUST_LOG=debug cargo run -p fraiseql-server

# Specific module logs
RUST_LOG=fraiseql_server=debug,tower_http=info

# Production logs with filtering
RUST_LOG=info cargo run -p fraiseql-server 2>&1 | jq .level
```

### Create Trace Context

```rust
use fraiseql_server::TraceContext;

let trace = TraceContext::new();
let header = trace.to_w3c_traceparent();
println!("traceparent: {}", header);

// Forward to downstream services
client.get(url)
    .header("traceparent", header)
    .send()?;
```

### Record Performance

```rust
use fraiseql_server::{PerformanceMonitor, QueryPerformance};

let monitor = PerformanceMonitor::new(100.0); // 100ms threshold

let perf = QueryPerformance::new(
    45_000,  // duration_us
    2,       // db_queries
    5,       // complexity
    false,   // cached
    30_000   // db_duration_us
);

monitor.record_query(perf);

// Get stats
println!("Avg: {:.2}ms", monitor.avg_duration_ms());
println!("Slow queries: {:.1}%", monitor.slow_query_percentage());
```

### Log Structured Entry

```rust
use fraiseql_server::{StructuredLogEntry, LogLevel, LogMetrics, RequestContext};

let entry = StructuredLogEntry::new(
    LogLevel::Info,
    "Query executed".to_string()
)
.with_request_context(
    RequestContext::new()
        .with_operation("GetUser".to_string())
        .with_user_id("user123".to_string())
)
.with_metrics(
    LogMetrics::new()
        .with_duration_ms(25.0)
        .with_cache_hit(true)
);

println!("{}", entry.to_json_string());
```

## Key Metrics

| Metric | Endpoint | Description |
|--------|----------|-------------|
| `fraiseql_graphql_queries_total` | `/metrics` | Total queries executed |
| `fraiseql_graphql_queries_success` | `/metrics` | Successful queries |
| `fraiseql_graphql_queries_error` | `/metrics` | Failed queries |
| `fraiseql_graphql_query_duration_ms` | `/metrics` | Average query time (ms) |
| `fraiseql_cache_hit_ratio` | `/metrics` | Cache efficiency (0-1) |
| `fraiseql_database_queries_total` | `/metrics` | Database operations |

## Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check and status |
| `/metrics` | GET | Prometheus text format |
| `/metrics/json` | GET | JSON format metrics |
| `/introspection` | POST | GraphQL schema info |
| `/graphql` | POST | GraphQL queries |

## Log Levels

| Level | When to Use |
|-------|------------|
| TRACE | Extremely detailed debugging |
| DEBUG | Detailed debugging info |
| INFO | General informational (DEFAULT) |
| WARN | Warning conditions |
| ERROR | Error conditions |

## Common Queries

### Prometheus Queries

```promql
# Query rate (per second)
rate(fraiseql_graphql_queries_total[5m])

# Error rate percentage
(rate(fraiseql_graphql_queries_error[5m]) / rate(fraiseql_graphql_queries_total[5m])) * 100

# p95 latency (if histogram)
histogram_quantile(0.95, fraiseql_graphql_query_duration_ms)

# Cache hit rate
fraiseql_cache_hit_ratio * 100

# Database time percentage
(rate(fraiseql_database_queries_total[5m]) / rate(fraiseql_graphql_queries_total[5m])) * 100
```

## Docker Compose Commands

```bash
# Start all services
docker-compose up -d

# Stop services
docker-compose down

# View logs
docker-compose logs -f fraiseql
docker-compose logs -f prometheus
docker-compose logs -f grafana

# Rebuild container
docker-compose build fraiseql
docker-compose up -d fraiseql

# Access database
docker-compose exec postgres psql -U postgres -d fraiseql
```

## Environment Variables

```bash
# Logging
RUST_LOG=debug

# Server
FRAISEQL_BIND_ADDR=0.0.0.0:8000
FRAISEQL_CONFIG=/path/to/config.toml
DATABASE_URL=postgresql://user:pass@localhost/db

# Performance
RUST_BACKTRACE=1
```

## Monitoring Checklist

- [ ] `/health` returns 200 OK
- [ ] `/metrics` returns valid Prometheus format
- [ ] Prometheus targets show "UP"
- [ ] Grafana dashboard loads without errors
- [ ] Query latency p95 < 200ms
- [ ] Error rate < 1%
- [ ] Cache hit rate > 60%
- [ ] Database connections < pool max

## Debugging Checklist

1. **Check Health**
   ```bash
   curl http://localhost:8000/health
   ```

2. **View Metrics**
   ```bash
   curl http://localhost:8000/metrics
   ```

3. **Check Logs**
   ```bash
   RUST_LOG=debug cargo run
   ```

4. **Verify Database**
   ```bash
   psql $DATABASE_URL -c "SELECT NOW();"
   ```

5. **Test Prometheus Scrape**
   ```bash
   curl http://prometheus:9090/api/v1/targets
   ```

6. **View Grafana Logs**
   ```bash
   docker-compose logs grafana
   ```

## Performance Tuning

### Slow Query Threshold
```rust
let monitor = PerformanceMonitor::new(50.0); // 50ms instead of 100ms
```

### Connection Pool
```toml
[pool]
min_size = 5
max_size = 20
timeout_secs = 30
```

### Logging Volume
```bash
# Reduce log volume in production
RUST_LOG=info,tower_http=warn,axum=warn
```

## Alert Conditions

| Alert | Condition |
|-------|-----------|
| High Error Rate | `errors/total > 5%` for 5 min |
| High Latency | `p95_duration > 200ms` for 10 min |
| Low Cache Hit | `cache_hit_ratio < 50%` for 30 min |
| Database Issues | `db_time > 80%` of total |
| Server Error | Any 5xx in last minute |

## Files and Locations

| File | Purpose |
|------|---------|
| `monitoring/grafana-dashboard.json` | Grafana dashboard |
| `docker-compose.yml` | Development environment |
| `k8s/deployment.yaml` | Kubernetes manifests |
| `docs/OBSERVABILITY.md` | Full documentation |
| `docs/STRUCTURED_LOGGING.md` | Logging guide |
| `docs/DISTRIBUTED_TRACING.md` | Tracing guide |
| `docs/PERFORMANCE_MONITORING.md` | Performance guide |

## Useful Links

- [Prometheus Docs](https://prometheus.io/docs/)
- [Grafana Docs](https://grafana.com/docs/)
- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
- [OpenTelemetry](https://opentelemetry.io/)
- [Observability Engineering Book](https://www.oreilly.com/library/view/observability-engineering/9781492076438/)

## Performance Targets

```
Metric                  Target      Warning     Critical
────────────────────────────────────────────────────────
Query Latency p95       < 100ms     > 200ms     > 500ms
Query Error Rate        < 0.1%      > 1%        > 5%
Cache Hit Rate          > 60%       < 50%       < 30%
Database Time %         < 60%       > 80%       > 90%
Memory Usage            < 200MB     > 400MB     > 600MB
Connection Pool Usage   < 80%       > 90%       > 95%
```

## Common Issues

| Issue | Solution |
|-------|----------|
| Metrics not appearing | Check `/metrics` endpoint, verify Prometheus config |
| High memory | Monitor trace context size, check for leaks |
| Missing logs | Set `RUST_LOG=debug`, check log sink |
| Slow queries | Use performance monitoring, check database |
| Trace lost | Verify `traceparent` header propagation |

## Support Resources

- **Documentation**: See `docs/OBSERVABILITY.md` for full guide
- **Issues**: Check GitHub issues for known problems
- **Community**: Ask on discussion forums
- **Debugging**: Enable `RUST_LOG=debug` for detailed logs
