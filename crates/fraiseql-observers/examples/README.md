# FraiseQL Observer Configuration Examples

This directory contains example configurations for different deployment topologies.

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

**Running:**

```bash
fraiseql-observer --config examples/01-postgresql-only.toml
```

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

# Start observer
fraiseql-observer --config examples/02-postgresql-redis.toml
```

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

# Terminal 3: Start Bridge (see 04-multi-database-bridge.toml)
fraiseql-observer --config examples/04-multi-database-bridge.toml

# Terminal 4-6: Start Workers (3 instances for HA)
fraiseql-observer --config examples/03-nats-distributed.toml
fraiseql-observer --config examples/03-nats-distributed.toml
fraiseql-observer --config examples/03-nats-distributed.toml
```

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

**Running:**

```bash
# Terminal 1: Start NATS cluster
docker-compose -f nats-cluster.yml up

# Terminal 2: Start Redis cluster
docker-compose -f redis-cluster.yml up

# Terminal 3-5: Start Bridges (one per database)
fraiseql-observer --config bridge-db1.toml
fraiseql-observer --config bridge-db2.toml
fraiseql-observer --config bridge-db3.toml

# Terminal 6-8: Start Workers
fraiseql-observer --config worker1.toml
fraiseql-observer --config worker2.toml
fraiseql-observer --config worker3.toml
```

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
enable_concurrent = true     # Concurrent action execution
max_concurrent_actions = 10
concurrent_timeout_ms = 30000
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
export FRAISEQL_ENABLE_CONCURRENT=true
export FRAISEQL_MAX_CONCURRENT_ACTIONS=20
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
# Validate configuration without running
fraiseql-observer --config examples/01-postgresql-only.toml --validate

# Dry-run (connects but doesn't process events)
fraiseql-observer --config examples/02-postgresql-redis.toml --dry-run

# Enable debug logging
RUST_LOG=debug fraiseql-observer --config examples/03-nats-distributed.toml
```

---

## Docker Compose Examples

See `docker-compose.*.yml` files for complete deployment examples:

- `docker-compose.postgres-only.yml` - Topology 1
- `docker-compose.postgres-redis.yml` - Topology 2
- `docker-compose.nats-distributed.yml` - Topology 3
- `docker-compose.multi-database.yml` - Topology 4

---

## Kubernetes Examples

See `k8s/*.yaml` for Kubernetes manifests:

- `k8s/deployment-bridge.yaml` - PostgreSQL → NATS bridge
- `k8s/deployment-worker.yaml` - Observer workers
- `k8s/statefulset-nats.yaml` - NATS JetStream cluster
- `k8s/statefulset-redis.yaml` - Redis cluster

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
# Check backlog
RUST_LOG=info fraiseql-observer --config config.toml

# Metrics output shows:
# - backlog_size: Current event queue depth
# - cache_hit_rate: Action cache effectiveness
# - dedup_hit_rate: Duplicate event rate
```

---

For more information, see:

- [Architecture Documentation](../.claude/REDIS_NATS_INTEGRATION_ARCHITECTURE.md)
- [Implementation Progress](../.claude/IMPLEMENTATION_PROGRESS.md)
