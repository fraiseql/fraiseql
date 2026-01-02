# Observability: Monitoring & Debugging Federation

This guide explains how to monitor and debug federation performance in production.

## Overview

Federation operates efficiently when batching works well. **You need visibility** into:
1. **How many requests** are hitting your resolver
2. **How much batching** is happening (dedup rate)
3. **Cache effectiveness** (hit rate)
4. **Database query load** (batch count)

FraiseQL provides built-in statistics. Use them to measure, alert, and optimize.

---

## Part 1: DataLoaderStats - Available Metrics

Every `EntityDataLoader` instance tracks these metrics:

### Core Metrics

```python
stats = loader.stats

# Total requests
print(f"Requests: {stats.total_requests}")

# Cache hits (entity loaded twice in same request)
print(f"Cache hits: {stats.cache_hits}")

# Cache misses (entity not in cache)
print(f"Cache misses: {stats.cache_misses}")

# Deduplication hits (duplicate request reused Future)
print(f"Dedup hits: {stats.dedup_hits}")

# Database queries executed
print(f"Batch count: {stats.batch_count}")
```

### Calculated Rates

```python
# Cache hit rate (0.0 to 1.0)
print(f"Cache hit rate: {stats.cache_hit_rate:.1%}")

# Deduplication rate (0.0 to 1.0)
print(f"Dedup rate: {stats.dedup_rate:.1%}")
```

### Example Output

```
Requests: 100
Cache hits: 30
Cache misses: 70
Dedup hits: 50
Batch count: 2
Cache hit rate: 30.0%
Dedup rate: 50.0%
```

**Interpretation**:
- 100 total entity requests
- 30 were in cache (same entity loaded twice in request)
- 70 were new
- 50 were duplicates reused via dedup
- Only 2 database queries (excellent batching!)

---

## Part 2: Basic Logging

### Log Stats After Each Request

```python
import logging

logger = logging.getLogger("federation")

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []
        for rep in representations:
            entity = await loader.load(
                rep["__typename"],
                "id",
                rep["id"]
            )
            results.append(entity)

        # Log stats after resolution
        stats = loader.stats
        logger.info(
            f"Federation resolved {len(representations)} entities | "
            f"requests={stats.total_requests} "
            f"cache_hit={stats.cache_hit_rate:.1%} "
            f"dedup={stats.dedup_rate:.1%} "
            f"queries={stats.batch_count}"
        )

        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

**Log output**:
```
Federation resolved 20 entities | requests=45 cache_hit=30.0% dedup=55.5% queries=2
```

### Enable Debug Logging

```python
import logging

# Enable all federation debug logs
logging.basicConfig(level=logging.DEBUG)
logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)

# Now you'll see:
# - Each load() call
# - When batch flushes
# - Database queries executed
# - Timing information
```

---

## Part 3: Production Monitoring

### Key Metrics to Track

**Per Request**:
- `federation.requests` - Total entity requests
- `federation.dedup_rate` - Deduplication rate (target >50%)
- `federation.cache_hit_rate` - Cache hit rate (depends on workload)
- `federation.batch_count` - Database queries executed
- `federation.latency_ms` - Total resolution time

**Per Service** (aggregate over time):
- Average dedup rate across all requests
- Average batch count (should match entity types)
- P95 latency (should be <50ms for interactive)
- Error rate

### Metrics Collection Example

```python
import time

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")
    start_time = time.time()

    async def handle_request(loader):
        results = []
        for rep in representations:
            entity = await loader.load(
                rep["__typename"],
                "id",
                rep["id"]
            )
            results.append(entity)
        return results

    results = await executor.execute_request(handle_request, resolver, db_pool)

    elapsed_ms = (time.time() - start_time) * 1000
    stats = loader.stats

    # Send to monitoring system (Prometheus, Datadog, etc.)
    metrics.timing("federation.latency_ms", elapsed_ms)
    metrics.gauge("federation.total_requests", stats.total_requests)
    metrics.gauge("federation.dedup_rate", stats.dedup_rate)
    metrics.gauge("federation.cache_hit_rate", stats.cache_hit_rate)
    metrics.gauge("federation.batch_count", stats.batch_count)
    metrics.increment("federation.resolver_calls")

    return results
```

### Prometheus Format

```python
from prometheus_client import Counter, Histogram, Gauge

# Histograms (distributions)
federation_latency = Histogram(
    "federation_latency_ms",
    "Federation resolution latency",
    buckets=(1, 5, 10, 25, 50, 100, 250)
)

# Gauges (point-in-time values)
federation_dedup_rate = Gauge(
    "federation_dedup_rate",
    "Federation deduplication rate"
)

federation_batch_count = Gauge(
    "federation_batch_count",
    "Number of database queries per request"
)

# Counters (cumulative)
federation_resolver_calls = Counter(
    "federation_resolver_calls_total",
    "Total federation resolver calls"
)

# Usage
federation_latency.observe(elapsed_ms)
federation_dedup_rate.set(stats.dedup_rate)
federation_batch_count.set(stats.batch_count)
federation_resolver_calls.inc()
```

### Datadog Integration

```python
from datadog import initialize, api

options = {
    "api_key": "YOUR_API_KEY",
    "app_key": "YOUR_APP_KEY"
}
initialize(**options)

async def resolve_entities(info, representations):
    # ... your resolver code ...

    # Send metrics
    api.Metric.send(
        metric="federation.latency",
        points=elapsed_ms,
        tags=["service:users", "endpoint:_entities"]
    )
    api.Metric.send(
        metric="federation.dedup_rate",
        points=stats.dedup_rate,
        tags=["service:users"]
    )

    return results
```

---

## Part 4: Alerting

### Alerts to Set Up

```python
# Alert 1: Low dedup rate (under-batching)
if stats.dedup_rate < 0.5:
    logger.warning(
        f"Low dedup rate ({stats.dedup_rate:.1%}) - "
        f"consider increasing batch window"
    )
    # Send to alerting system
    alerts.warn("federation.low_dedup_rate", tags=["service:users"])

# Alert 2: High batch count (possible inefficiency)
if stats.batch_count > 10:
    logger.warning(
        f"High batch count ({stats.batch_count}) - "
        f"check database performance or batching strategy"
    )
    alerts.warn("federation.high_batch_count", tags=["service:users"])

# Alert 3: Slow request
if elapsed_ms > 100:
    logger.warning(
        f"Slow federation request ({elapsed_ms}ms) - "
        f"check database latency or batch window"
    )
    alerts.warn("federation.slow_request", tags=["service:users"])

# Alert 4: Many cache misses (possible memory pressure)
if stats.cache_hit_rate < 0.1 and stats.total_requests > 100:
    logger.warning(
        f"Low cache hit rate ({stats.cache_hit_rate:.1%}) - "
        f"may need larger cache"
    )
```

### Threshold Recommendations

| Metric | Good | Warning | Critical |
|--------|------|---------|----------|
| **Dedup rate** | >70% | 50-70% | <50% |
| **Batch count** | ~2-5* | 5-10 | >10 |
| **Cache hit rate** | >30% | 10-30% | <10% |
| **Latency (P95)** | <20ms | 20-50ms | >100ms |

*Batch count should roughly match number of entity types

---

## Part 5: Debugging Federation Issues

### Problem: Low Dedup Rate

**Symptom**: Dedup rate < 50%, many database queries

**Root causes**:
1. Batch window too short (requests don't overlap)
2. Each request has unique entities
3. Different entity types not batched together

**Debugging steps**:
```python
# Step 1: Check batch window
executor = PerRequestBatchExecutor(batch_window_ms=1.0)  # Default
# If low: increase to 5.0 or 10.0

# Step 2: Log batch composition
async def handle_request(loader):
    # Log before requests
    initial_requests = len(loader._pending_requests)

    results = [await loader.load(...) for ...]

    # Log after requests
    final_requests = len(loader._pending_requests)
    logger.debug(f"Batch grew from {initial_requests} to {final_requests}")
    return results

# Step 3: Check if resolvers load concurrently
# This is CRITICAL - requests must be fired in parallel:

# ❌ WRONG (sequential, no dedup opportunity):
async def handle_request(loader):
    user1 = await loader.load("User", "id", "1")  # Waits
    user2 = await loader.load("User", "id", "2")  # Waits
    return [user1, user2]

# ✅ RIGHT (concurrent, dedup works):
async def handle_request(loader):
    futures = [
        loader.load("User", "id", "1"),
        loader.load("User", "id", "2"),
    ]
    return await asyncio.gather(*futures)
```

### Problem: High Latency

**Symptom**: Requests taking 50+ ms

**Root causes**:
1. Batch window too long (waiting unnecessarily)
2. Database queries are slow
3. N+1 queries (batching not working)

**Debugging steps**:
```python
# Step 1: Check batch window
executor = PerRequestBatchExecutor(batch_window_ms=1.0)  # Default
# If high latency: decrease to 0.1ms

# Step 2: Measure database latency
import time

async def handle_request(loader):
    db_start = time.time()
    results = [...]
    await loader.flush()  # Force flush to measure DB time
    db_elapsed = (time.time() - db_start) * 1000

    logger.info(f"Database query took {db_elapsed}ms for {stats.batch_count} queries")
    # If high: check database performance
    # If normal: batch window is the bottleneck

    return results

# Step 3: Check request size
stats = loader.stats
logger.info(f"Resolved {stats.total_requests} requests in {stats.batch_count} queries")
# If batch_count = total_requests: no batching happening
# Check dedup rate and cache hit rate
```

### Problem: Memory Issues

**Symptom**: Cache size growing, memory pressure

**Root causes**:
1. Cache size too large for workload
2. Cache_size set too high
3. Entities not being evicted (access patterns)

**Debugging steps**:
```python
# Step 1: Check cache size
loader = EntityDataLoader(resolver, db_pool, cache_size=1000)  # Default
# If memory pressure: reduce to 100 or 500

# Step 2: Monitor evictions
# DataLoader uses LRU - oldest entries evicted when full
# If cache hit rate low: evictions too frequent
logger.debug(f"Cache hit rate: {stats.cache_hit_rate:.1%}")
# If <10%: increase cache_size

# Step 3: Profile memory usage
import tracemalloc

tracemalloc.start()
# ... run request ...
current, peak = tracemalloc.get_traced_memory()
logger.info(f"Memory usage: {peak / 1024 / 1024:.1f}MB")
```

---

## Part 6: Production Dashboard

### Example Grafana/Datadog Dashboard

**Panels to include**:

1. **Federation Latency** (timeseries)
   - X-axis: Time
   - Y-axis: Milliseconds
   - Metrics: P50, P95, P99 latency
   - Alert: P95 > 50ms

2. **Dedup Rate** (gauge or line chart)
   - Current: percentage
   - Target: >70%
   - Alert: <50%

3. **Batch Count** (line chart)
   - X-axis: Time
   - Y-axis: Queries per request
   - Expected: 2-5 (one per entity type)
   - Alert: >10

4. **Cache Hit Rate** (line chart)
   - Expected: 0-50% depending on workload
   - Low rate (0-20%): normal for most queries
   - High rate (>50%): indicates repeated entities

5. **Request Volume** (counter)
   - Total federation resolver calls
   - Requests per second
   - Track growth over time

6. **Error Rate** (counter)
   - Federation errors
   - Database errors
   - Timeout errors

### Sample Alert Rules

```yaml
# Prometheus/Grafana alert rules
- alert: FederationLowDedupRate
  expr: federation_dedup_rate < 0.5
  for: 5m
  annotations:
    summary: "Low deduplication rate in federation"
    description: "Dedup rate {{ $value | humanizePercentage }} - check batch window"

- alert: FederationHighBatchCount
  expr: federation_batch_count > 10
  for: 5m
  annotations:
    summary: "High database query count per federation request"
    description: "{{ $value }} queries - check entity batching"

- alert: FederationSlowRequest
  expr: federation_latency_ms{quantile="0.95"} > 50
  for: 5m
  annotations:
    summary: "Slow federation requests"
    description: "P95 latency {{ $value }}ms - check database or batch window"
```

---

## Part 7: Common Observability Patterns

### Pattern 1: Request Tracking

```python
import uuid

async def resolve_entities(info, representations):
    request_id = str(uuid.uuid4())
    logger.info(f"[{request_id}] Federation request started")

    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        logger.debug(f"[{request_id}] Loading {len(representations)} entities")
        results = [...]

        stats = loader.stats
        logger.info(
            f"[{request_id}] Completed: "
            f"requests={stats.total_requests}, "
            f"dedup={stats.dedup_rate:.1%}, "
            f"queries={stats.batch_count}"
        )
        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

### Pattern 2: Service-Level Aggregation

```python
# Aggregate stats across multiple requests

class FederationMetrics:
    def __init__(self):
        self.requests = []
        self.total_requests = 0
        self.total_dedup_rate = 0
        self.request_count = 0

    def record(self, stats):
        self.requests.append(stats)
        self.total_requests += stats.total_requests
        self.total_dedup_rate += stats.dedup_rate
        self.request_count += 1

    @property
    def average_dedup_rate(self):
        if self.request_count == 0:
            return 0.0
        return self.total_dedup_rate / self.request_count

    def summary(self):
        return {
            "avg_dedup_rate": self.average_dedup_rate,
            "total_entity_requests": self.total_requests,
            "federation_calls": self.request_count,
        }

# Usage
metrics = FederationMetrics()

async def resolve_entities(info, representations):
    # ... resolution code ...
    metrics.record(loader.stats)
    logger.info(f"Service metrics: {metrics.summary()}")
    return results
```

### Pattern 3: Contextual Logging

```python
import logging
from contextvars import ContextVar

# Context variable for request ID
federation_request_id = ContextVar("federation_request_id", default=None)

class FederationHandler(logging.Handler):
    """Custom log handler that includes federation context."""

    def emit(self, record):
        request_id = federation_request_id.get()
        if request_id:
            record.msg = f"[{request_id}] {record.msg}"

async def resolve_entities(info, representations):
    request_id = str(uuid.uuid4())
    token = federation_request_id.set(request_id)

    try:
        # All logs in this async context include request ID
        logger.info("Federation request started")
        # ... resolution ...
        logger.info("Federation request completed")
    finally:
        federation_request_id.reset(token)
```

---

## Part 8: Troubleshooting Checklist

- [ ] **Dedup rate low?** Increase batch_window_ms, ensure concurrent loads
- [ ] **Latency high?** Decrease batch_window_ms, check database performance
- [ ] **Cache hit rate low?** Increase cache_size or check access patterns
- [ ] **Many queries?** Check batch_count matches entity types
- [ ] **Memory pressure?** Reduce cache_size or profile entity sizes
- [ ] **Errors?** Enable debug logging, check database connection
- [ ] **Slow database?** Profile queries, add indexes, check connection pool size

---

## Part 9: Key Takeaways

**Observability enables informed decisions**:

1. **Measure First** - Use DataLoaderStats to understand current behavior
2. **Alert on Anomalies** - Set thresholds for dedup, latency, batch count
3. **Debug Systematically** - Use logs and metrics to diagnose issues
4. **Optimize Based on Data** - Don't tune without measurement

**Golden Rule**: If you can't measure it, you can't improve it.

---

## Summary

| Task | Tool | Metric |
|------|------|--------|
| **See request stats** | `loader.stats` | dedup_rate, cache_hit_rate, batch_count |
| **Log performance** | `logger.info()` | Batch composition, latency |
| **Monitor production** | Prometheus/Datadog | Federation metrics over time |
| **Alert on issues** | Alert rules | Low dedup, high latency, high batch count |
| **Debug problems** | Enable logging | Request flow, batch behavior, database timing |

---

**Next Steps**:
- **Integration guide** - See FastAPI/Strawberry integration docs for context passing
- **Performance issues?** See [Performance Tuning Guide](./05-performance-tuning.md)
- **Understanding batching?** See [Request Lifecycle](./02-request-lifecycle.md)
