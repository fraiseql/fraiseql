# Federation Circuit Breaker

FraiseQL includes per-entity-type circuit breakers that protect federation queries
from cascading failures when a subgraph becomes slow or unavailable.

The implementation lives in `fraiseql-server/src/federation/circuit_breaker.rs`.

## State Machine

```
               ≥ failure_threshold consecutive failures
    CLOSED ──────────────────────────────────────────► OPEN
      ▲                                                  │
      │                          recovery_timeout_secs   │
      │                   ┌──────────────────────────────┘
      │                   ▼
      │              HALF-OPEN  (one probe request allowed)
      │                   │
      │   ≥ success_threshold │   < success_threshold consecutive failures
      └───────────────────┘ └──────────────────────────────────────────► OPEN
           (circuit closed)                              (back to open)
```

- **CLOSED**: Normal operation. All requests pass through.
- **OPEN**: Circuit tripped. All requests immediately return HTTP 503 with a
  `Retry-After` header. No upstream calls are made.
- **HALF-OPEN**: One probe request is allowed through. If it succeeds (and
  `success_threshold` more), the circuit closes. If it fails, back to OPEN.

## Configuration

Configuration lives in the `federation` section of your compiled schema, driven by
`fraiseql.toml`:

```toml
# fraiseql.toml

[federation.circuit_breaker]
enabled = true

# Open circuit after this many consecutive failures
failure_threshold = 5

# How long (seconds) to keep the circuit OPEN before probing HALF-OPEN
recovery_timeout_secs = 30

# Number of consecutive successes in HALF-OPEN required to close the circuit
success_threshold = 2
```

The configuration is validated at compile time (`fraiseql compile`) and embedded in
`schema.compiled.json`. The `fraiseql-cli` diagnostics will warn on invalid values
(e.g., `failure_threshold = 0`).

## Per-Entity Overrides

Each entity type name gets an independent circuit breaker. All share the global defaults
unless a per-entity override is specified (planned feature — currently all entities share
global config). Track progress at the `fraiseql-federation` issue tracker.

Workaround: deploy separate FraiseQL instances for different entity tier criticality.

## Prometheus Metrics

| Metric | Labels | Description |
|--------|--------|-------------|
| `fraiseql_federation_circuit_breaker_state` | `entity` | 0=closed, 1=open, 2=half-open |
| `fraiseql_federation_circuit_breaker_opens_total` | `entity` | Times breaker tripped open |
| `fraiseql_federation_circuit_breaker_rejections_total` | `entity` | Requests rejected while open |

Monitor with:
```promql
# Alert when any entity breaker has been open for > 5 minutes
fraiseql_federation_circuit_breaker_state == 1
  unless on(entity) (changes(fraiseql_federation_circuit_breaker_state[5m]) > 0)
```

## Grafana Dashboard

The built-in Grafana dashboard (available at `GET /api/v1/admin/grafana-dashboard`) includes
a federation panel showing:

- Circuit state per entity type (colour-coded: green/red/yellow)
- Opens per minute trend line
- Rejection rate percentage

## Operational Runbook

See [runbooks/14-federation-circuit-breaker.md](../runbooks/14-federation-circuit-breaker.md)
for investigation steps, reset procedures, and escalation guidance.

## Tuning Guide

### Too many false positives (circuit opens on transient errors)

Increase `failure_threshold` and/or shorten `recovery_timeout_secs`:

```toml
[federation.circuit_breaker]
failure_threshold = 10      # Was 5 — tolerate more transient failures
recovery_timeout_secs = 15  # Was 30 — recover faster
success_threshold = 3       # Require more proof before closing
```

### Cascading failures not stopped quickly enough

Lower `failure_threshold`:

```toml
[federation.circuit_breaker]
failure_threshold = 3       # Trip faster on repeated failures
recovery_timeout_secs = 60  # Stay open longer to give subgraph time to recover
```

### Critical entity types (payment, auth)

For business-critical entities, you want to trip early and recover cautiously:

```toml
[federation.circuit_breaker]
failure_threshold = 2       # Trip after just 2 failures
recovery_timeout_secs = 120 # Wait 2 minutes before probing
success_threshold = 5       # Require 5 clean probes before fully reopening
```

### Development / testing environments

Disable the breaker to avoid it tripping during service restarts:

```toml
[federation.circuit_breaker]
enabled = false
```

Or set a very high threshold:
```toml
[federation.circuit_breaker]
failure_threshold = 1000
```

## HTTP Response When Open

When a circuit is OPEN, FraiseQL returns:

```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
Retry-After: 28

{
  "errors": [{
    "message": "Federation entity 'Product' is temporarily unavailable",
    "extensions": {
      "category": "CIRCUIT_BREAKER",
      "entity": "Product",
      "retry_after_secs": 28
    }
  }]
}
```

The `Retry-After` header reflects the remaining `recovery_timeout_secs` so clients
can implement proper backoff.
