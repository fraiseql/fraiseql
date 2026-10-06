# FraiseQL Observer Configuration Examples

This directory contains example configurations for different deployment topologies.

> **What these files are.** Each `.toml` here is an `ObserverRuntimeConfig` document for code
> built on this crate (`toml::from_str::<fraiseql_observers::config::ObserverRuntimeConfig>`,
> then `fraiseql_observers::factory`). `tests/config_documents.rs` parses and validates every
> one. This repository ships no standalone observer binary: load these documents from your own
> binary. To run observers inside `fraiseql-server`, configure them in `fraiseql.toml` under
> `[observers]` instead ([Operating Observers](../../../docs/operations/observers.md)).

## Deployment Topologies

### 1. PostgreSQL-Only (`01-postgresql-only.toml`)

**When to use:**

- Single PostgreSQL database
- Low event volume (<1000 events/sec)
- Simple deployment (no additional infrastructure)
- Development/testing

**Architecture:**

```
PostgreSQL (LISTEN/NOTIFY) → Observer Workers (in-process)
```

**Pros:**

- ✅ Simplest deployment
- ✅ No additional infrastructure (Redis, NATS)
- ✅ Low operational overhead

**Cons:**

- ❌ No deduplication (at-most-once delivery)
- ❌ No action caching (slower repeated operations)
- ❌ No horizontal scaling
- ❌ Single point of failure

**Running:** under `fraiseql-server` this is the default transport
(`[observers.runtime.transport] transport = "postgres"`).

---

### 2. PostgreSQL + Redis (`02-postgresql-redis.toml`)

**When to use:**

- Single PostgreSQL database
- Medium event volume (1000-10000 events/sec)
- Needs reliability (deduplication)
- Needs performance (action result caching)

**Architecture:**

```
PostgreSQL (LISTEN/NOTIFY) → Observer Workers
                              ↓
                            Redis (dedup + cache)
```

**Pros:**

- ✅ Event deduplication (at-least-once delivery)
- ✅ Action result caching (100x faster for cache hits)
- ✅ Simple deployment (2 services)

**Cons:**

- ❌ No horizontal scaling
- ❌ Single database only

**Running:**

```bash
# Start Redis
docker run -d -p 6379:6379 redis:7
```

`fraiseql-server` runs no Redis deduplication or result cache, so this topology applies only
to code built on this crate.

---

### 3. NATS Distributed (`03-nats-distributed.toml`)

**When to use:**

- Multiple observer workers (horizontal scaling)
- High availability (worker failures tolerated)
- High event volume (10000+ events/sec)
- Geographic distribution

**Architecture:**

```
PostgreSQL → Bridge → NATS JetStream → Worker 1
                                      → Worker 2
                                      → Worker 3
                                          ↓
                                        Redis
```

**Pros:**

- ✅ Horizontal scaling (add workers on-demand)
- ✅ High availability (workers can fail/restart)
- ✅ At-least-once delivery (NATS + Redis dedup)
- ✅ Geographic distribution
- ✅ Load balancing across workers

**Cons:**

- ❌ Complex deployment (PostgreSQL + NATS + Redis)
- ❌ Higher operational overhead

**Running:**

```bash
# Terminal 1: Start NATS
docker run -d -p 4222:4222 nats:latest -js

# Terminal 2: Start Redis
docker run -d -p 6379:6379 redis:7
```

Under `fraiseql-server`, the workers are server replicas with `transport = "nats"` and the
same `consumer_name`, which compete for messages; the server does not run the bridge
(see [Operating Observers - Scaling](../../../docs/operations/observers.md#scaling)).

---

### 4. Multi-Database Bridge (`04-multi-database-bridge.toml`)

**When to use:**

- Multiple PostgreSQL databases
- Centralized event bus (NATS)
- Separate bridge and worker processes

**Architecture:**

```
Database 1 → Bridge 1 ┐
Database 2 → Bridge 2 ├→ NATS → Worker 1
Database 3 → Bridge 3 ┘          Worker 2
                                 Worker 3
```

**Pros:**

- ✅ Multi-database support
- ✅ Centralized monitoring (NATS)
- ✅ Independent scaling (bridges vs workers)
- ✅ Fault isolation

**Cons:**

- ❌ Most complex deployment
- ❌ Highest operational overhead

---

## Configuration Sections

### Transport

```toml
[transport]
transport = "postgres" | "nats" | "in_memory"
run_bridge = false     # Run PostgreSQL → NATS bridge
run_executors = true   # Run observer workers
```

### Redis

```toml
[redis]
url = "redis://localhost:6379"
pool_size = 10
connect_timeout_secs = 5
command_timeout_secs = 2
dedup_window_secs = 300
cache_ttl_secs = 60
```

### Performance

```toml
[performance]
enable_dedup = true          # Event deduplication (requires Redis)
enable_caching = true        # Action result caching (requires Redis)
```

### Observers

```toml
[[observers]]
event_type = "INSERT" | "UPDATE" | "DELETE" | "CUSTOM"
entity = "Order"
condition = "data.status == 'shipped'"  # Optional JMESPath filter

[[observers.actions]]
type = "webhook"
url = "https://example.com/webhook"
body_template = "{{ event.data }}"

[observers.retry]
max_attempts = 3
initial_delay_ms = 100
max_delay_ms = 30000
backoff_strategy = "exponential"
```

---

## Environment Variable Overrides

All configuration values can be overridden via environment variables:

```bash
# Transport
export FRAISEQL_OBSERVER_TRANSPORT=nats
export FRAISEQL_NATS_URL=nats://nats-cluster:4222
export FRAISEQL_NATS_ENABLE_BRIDGE=true
export FRAISEQL_NATS_RUN_EXECUTORS=false

# Redis
export FRAISEQL_REDIS_URL=redis://redis-cluster:6379
export FRAISEQL_REDIS_POOL_SIZE=20
export FRAISEQL_REDIS_DEDUP_WINDOW_SECS=300
export FRAISEQL_REDIS_CACHE_TTL_SECS=60

# Performance
export FRAISEQL_ENABLE_DEDUP=true
export FRAISEQL_ENABLE_CACHING=true
```

---

## Performance Comparison

| Topology | Throughput | Latency (p50) | Latency (p99) | HA | Horizontal Scaling |
|----------|------------|---------------|---------------|----|--------------------|
| PostgreSQL-Only | 1K events/s | 10ms | 50ms | ❌ | ❌ |
| PostgreSQL + Redis | 5K events/s | 8ms (cache hit: <1ms) | 40ms | ❌ | ❌ |
| NATS Distributed | 50K events/s | 15ms | 100ms | ✅ | ✅ |
| Multi-Database | 100K+ events/s | 20ms | 150ms | ✅ | ✅ |

*Benchmarks assume:*

- PostgreSQL on SSD
- Redis in-memory
- NATS JetStream with 3-node cluster
- 10 observer workers

---

## Choosing a Topology

```
                          START
                            |
                   ┌────────┴────────┐
                   │ Single DB?      │
                   └────────┬────────┘
                      Yes ┌─┴─┐ No
                          │   └──────────────────┐
                   ┌──────┴──────┐              │
                   │ Event volume?│              │
                   └──────┬──────┘              │
                    <1K ┌─┴─┐ >1K               │
                        │   │                   │
                  ┌─────┘   └──────┐            │
                  │                │            │
            [PostgreSQL-Only]  [PostgreSQL     │
                               + Redis]        │
                                                │
                                    ┌───────────┘
                                    │
                             ┌──────┴──────┐
                             │ HA required?│
                             └──────┬──────┘
                              Yes ┌─┴─┐ No
                                  │   │
                    ┌─────────────┘   └──────────────┐
                    │                                 │
            [NATS Distributed]              [Multi-Database
                                              Bridge]
```

---

## Testing Configurations

```bash
# Parse and validate every example document
cargo test -p fraiseql-observers --test config_documents
```

---

## Kubernetes

There is no `k8s/` directory. The four manifests this section used to list —
`deployment-bridge.yaml`, `deployment-worker.yaml`, `statefulset-nats.yaml`,
`statefulset-redis.yaml` — have never existed in this repository (#1218).

The supported deployment artifact is the Helm chart at
`deploy/kubernetes/helm/fraiseql`, which CI deploys and queries.
`helm template ./deploy/kubernetes/helm/fraiseql` renders plain manifests from it
if you need them.

---

## Troubleshooting

**Bridge not publishing events:**

```bash
# Check checkpoint table
SELECT * FROM tb_observer_checkpoint WHERE transport_name = 'pg_to_nats';

# Reset checkpoint (re-publishes all events)
DELETE FROM tb_observer_checkpoint WHERE transport_name = 'pg_to_nats';
```

**Workers not receiving events:**

```bash
# Check NATS consumer
nats consumer info fraiseql_events fraiseql_observer_worker_group_1

# Check lag
nats consumer report fraiseql_events
```

**Redis connection issues:**

```bash
# Test Redis connectivity
redis-cli -u redis://localhost:6379 PING

# Check dedup keys
redis-cli -u redis://localhost:6379 KEYS "event:*"

# Check cache keys
redis-cli -u redis://localhost:6379 KEYS "action_result:*"
```

**High latency:**

```bash
# Check action latency and cache/dedup effectiveness on /metrics
curl -s "$METRICS_URL" | grep -E "fraiseql_observer_(action_duration_seconds|cache_|dedup_)"
```

---

For more information, see:

- [Architecture Documentation](../.claude/REDIS_NATS_INTEGRATION_architecture.md)
- [Implementation Progress](../.claude/IMPLEMENTATION_PROGRESS.md)
