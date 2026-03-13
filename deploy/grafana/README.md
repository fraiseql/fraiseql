# FraiseQL Grafana Dashboard

Pre-built Grafana 10+ dashboard for monitoring FraiseQL server performance.

## Import via API (recommended)

```bash
# 1. Fetch the dashboard from your running FraiseQL server
curl -s http://localhost:8080/api/v1/admin/grafana-dashboard \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -o /tmp/fraiseql-dashboard.json

# 2. Import into Grafana
curl -s -X POST http://admin:admin@localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d "{\"dashboard\": $(cat /tmp/fraiseql-dashboard.json), \"overwrite\": true, \"folderId\": 0}"
```

## Import via Grafana UI

1. Open **Dashboards → Import** in your Grafana instance.
2. Upload `fraiseql-dashboard.json` or paste its contents.
3. Select your Prometheus datasource when prompted.
4. Click **Import**.

## Panels

| # | Panel | Metric |
|---|-------|--------|
| 1 | Requests / sec | `fraiseql_graphql_queries_total` |
| 2 | Query Latency (P50/P95/P99) | `fraiseql_request_duration_seconds` |
| 3 | Error Rate % | `fraiseql_graphql_queries_total{status="error"}` |
| 4 | Cache Hit Ratio | `fraiseql_cache_hits_total` / total |
| 5 | Cache Hits vs Misses / sec | `fraiseql_cache_hits_total`, `fraiseql_cache_misses_total` |
| 6 | APQ Stored Queries | `fraiseql_apq_stored_total` |
| 7 | Connection Pool (total/active/idle) | `fraiseql_db_pool_connections_*` |
| 8 | Requests Waiting for Connection | `fraiseql_db_pool_requests_waiting` |
| 9 | Pool Tuning Adjustments / sec | `fraiseql_pool_tuning_adjustments_total` |
| 10 | Multi-Root Query Rate | `fraiseql_multi_root_queries_total` |
| 11 | Recommended vs Current Pool Size | `fraiseql_pool_recommended_size` |
| 12 | Database Error Rate | `fraiseql_db_errors_total` |
