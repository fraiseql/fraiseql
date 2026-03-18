# Runbook: Federation Circuit Breaker Tripped

## Symptoms

- `fraiseql_federation_circuit_breaker_state{entity="..."}` gauge = 1 (OPEN)
- Entity resolution requests returning HTTP 503 with `Retry-After` header
- GraphQL errors with `"category": "CIRCUIT_BREAKER"` and message: "Federation entity '...' is temporarily unavailable"
- Health endpoint (`/health`) shows federation subgraph state as `"open"`
- Upstream subgraph errors visible in logs prior to breaker trip

## Impact

- All GraphQL queries referencing the affected federation entity type fail with 503
- Other entity types and non-federated queries are **unaffected** (breakers are per-entity)
- Downstream consumers receive partial data or errors for federated fields
- If multiple entity types trip simultaneously, federation queries may become fully unavailable

## Investigation

### 1. Identify Which Entity Types Are Open

```bash
# Check Prometheus metrics for breaker state
# 0=closed (normal), 1=open (rejecting), 2=half_open (probing recovery)
curl -s http://localhost:8815/metrics | grep fraiseql_federation_circuit_breaker_state

# Example output:
# fraiseql_federation_circuit_breaker_state{entity="Product"} 1
# fraiseql_federation_circuit_breaker_state{entity="User"} 0
# → Product breaker is OPEN, User breaker is healthy
```

### 2. Check Health Endpoint for Federation Status

```bash
curl -s http://localhost:8815/health | jq '.federation'

# Example output when breaker is open:
# {
#   "subgraphs": [
#     { "subgraph": "Product", "state": "open" },
#     { "subgraph": "User", "state": "closed" }
#   ]
# }
```

### 3. Check Upstream Subgraph Health

```bash
# Review recent logs for the failing entity type
docker logs fraiseql-server 2>&1 | grep -i "circuit\|federation\|entity" | tail -30

# Test upstream subgraph connectivity directly
# (replace URL with the subgraph endpoint from your federation config)
curl -v https://subgraph-service.internal/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ _service { sdl } }"}'
```

### 4. Review Recent Deployment Changes

```bash
# Check if a recent deployment changed federation config or subgraph URLs
git log --oneline -10

# Check current circuit breaker configuration
jq '.federation.circuit_breaker' /etc/fraiseql/schema.compiled.json
```

### 5. Check Network Connectivity

```bash
# DNS resolution for subgraph host
dig subgraph-service.internal

# TCP connectivity
nc -zv subgraph-service.internal 443

# Check for network policy or firewall changes
kubectl get networkpolicies -A  # if running on Kubernetes
```

## Mitigation

### Automatic Recovery (wait and monitor)

The circuit breaker follows a 3-state recovery cycle:

1. **OPEN** → After `recovery_timeout_secs` (default: 30s), transitions to **HALF_OPEN**
2. **HALF_OPEN** → Allows one probe request at a time
3. If `success_threshold` (default: 2) consecutive probes succeed → **CLOSED** (normal)
4. If any probe fails in HALF_OPEN → back to **OPEN**

```bash
# Monitor for automatic recovery
watch -n 5 'curl -s http://localhost:8815/metrics | grep circuit_breaker_state'

# Watch for state transitions in logs
docker logs -f fraiseql-server 2>&1 | grep -i "circuit"
```

### Manual Intervention

**If upstream is down (expected behavior):**
- The breaker is protecting your service from cascading failures — no action needed
- Contact the upstream subgraph team to restore their service
- Monitor for automatic recovery once upstream is healthy

**If upstream recovered but breaker appears stuck:**
- Restart the FraiseQL server to reset all circuit breaker state:

```bash
docker restart fraiseql-server
# or
systemctl restart fraiseql-server
```

**If tripping is a false positive (transient network blip):**
- Increase `failure_threshold` to require more consecutive failures before tripping:

```toml
# fraiseql.toml
[federation.circuit_breaker]
enabled = true
failure_threshold = 10        # default: 5
recovery_timeout_secs = 30
success_threshold = 2
```

- For a specific entity type that is more flaky:

```toml
[[federation.circuit_breaker.per_database]]
database = "Product"
failure_threshold = 15
recovery_timeout_secs = 15
```

- Recompile the schema and reload:

```bash
fraiseql-cli compile schema.json -c fraiseql.toml -o schema.compiled.json
# Then restart or hot-reload the server
```

## Resolution

### Root Cause Checklist

| Cause | Evidence | Fix |
|-------|----------|-----|
| Upstream subgraph down | Subgraph health check fails | Restore upstream service |
| Network partition | `nc -zv` fails, DNS issues | Fix network/DNS |
| Subgraph URL changed | Config mismatch, 404 errors | Update federation config |
| Upstream rate limiting | 429 responses in logs | Reduce request rate or coordinate with upstream |
| TLS certificate expired | TLS handshake errors | Rotate certificates (see Runbook 10) |
| Threshold too low | Trips on minor transient errors | Increase `failure_threshold` |

### Post-Incident Verification

```bash
# 1. Confirm all breakers are closed
curl -s http://localhost:8815/metrics | grep circuit_breaker_state
# All values should be 0 (closed)

# 2. Verify federation queries work end-to-end
curl -s http://localhost:8815/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ _entities(representations: [{__typename: \"Product\", id: \"1\"}]) { ... on Product { name } } }"}'

# 3. Check health endpoint
curl -s http://localhost:8815/health | jq '.federation.subgraphs'
```

## Prevention

### Monitoring and Alerting

```yaml
# Prometheus alerting rules
groups:
  - name: fraiseql_federation
    rules:
      - alert: CircuitBreakerOpen
        expr: fraiseql_federation_circuit_breaker_state == 1
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "Circuit breaker open for entity {{ $labels.entity }}"
          description: "Federation entity {{ $labels.entity }} breaker has been open for >1m. Check upstream subgraph health."

      - alert: CircuitBreakerHalfOpen
        expr: fraiseql_federation_circuit_breaker_state == 2
        for: 5m
        labels:
          severity: info
        annotations:
          summary: "Circuit breaker stuck in half-open for entity {{ $labels.entity }}"
          description: "Entity {{ $labels.entity }} breaker is half-open for >5m — recovery probes may be failing intermittently."
```

### Configuration Best Practices

- Set `failure_threshold` high enough to tolerate transient errors (5–10 for stable upstreams, 10–20 for flaky ones)
- Use `per_database` overrides for entity types with different reliability profiles
- Keep `recovery_timeout_secs` short enough for fast recovery (15–60s) but long enough to avoid hammering a recovering upstream
- Monitor upstream subgraph health independently — don't rely solely on the circuit breaker

### Grafana Dashboard Panels

Recommended panels for a federation health dashboard:

- **Circuit breaker state timeline** — `fraiseql_federation_circuit_breaker_state` per entity
- **Entity resolution error rate** — correlate with breaker trips
- **Upstream subgraph latency** — early warning before breaker trips
- **Recovery success rate** — track how often HALF_OPEN probes succeed

## Escalation

- **Upstream subgraph down**: Contact upstream subgraph team
- **Network/DNS issues**: Infrastructure / Network team
- **Configuration questions**: Application team (review `fraiseql.toml` federation section)
- **Repeated false positives**: Performance team (tune thresholds, investigate transient failures)

## Related Runbooks

- [03 - High Latency](./03-high-latency.md) — federation fan-out can cause latency spikes before breaker trips
- [12 - Incident Response](./12-incident-response.md) — escalation procedures for major incidents
- [13 - Schema Hot-Reload Failure](./13-schema-hot-reload-failure.md) — if schema reload changes circuit breaker config
