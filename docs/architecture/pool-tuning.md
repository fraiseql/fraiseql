# Pool Auto-Tuning Architecture

## Overview

FraiseQL includes a `PoolSizingAdvisor` that monitors database connection pool health and emits scaling recommendations. It operates in **advisory mode only** — it does not resize the pool at runtime.

## Why Advisory-Only (v2.1.0)

The `deadpool-postgres` library (v0.14) does not expose a `resize()` or `set_max_size()` API on `Pool`. The auto-tuner infrastructure includes a `resize_fn: Option<Arc<dyn Fn(usize)>>` callback parameter, but it is always passed as `None`.

**Alternatives considered:**

| Option | Description | Trade-off |
|--------|------------|-----------|
| `ArcSwap<Pool>` | Create new pool with new size, swap atomically | Active connections on old pool may leak |
| Migrate to `bb8` | `bb8` supports `set_max_size()` | Large migration effort for one feature |
| **Advisory-only** | Log recommendations, expose metrics | Requires operator restart to apply |

Advisory-only was chosen for v2.1.0 because:
- The auto-tuner already provides actionable metrics and log recommendations
- Graceful restart is a well-understood operational pattern
- Pool sizing changes are infrequent in practice (typically at deploy time)

## How It Works

### Monitoring Loop

`PoolSizingAdvisor::start()` spawns a background task that polls `PoolMetrics` at a configurable interval (default: 30s).

### Scaling Decisions

**Scale-up trigger:** `waiting_requests > target_queue_depth` for N consecutive samples (default: 3).
- Grows by `scale_up_step` (default: 5 connections)
- Capped at `max_pool_size` (default: 50)

**Scale-down trigger:** `idle_ratio > 50%` with zero waiting requests for N consecutive samples.
- Shrinks by `scale_down_step` (default: 2 connections)
- Floored at `min_pool_size` (default: 5)

### Output

- `WARN`-level log with recommended new size
- Prometheus metrics: `fraiseql_pool_tuning_recommended_size`, `fraiseql_pool_tuning_scale_events_total`
- Queryable via `/api/v1/admin/config`

## Operator Workflow

1. Monitor `fraiseql_pool_tuning_recommended_size` in Grafana
2. When recommended size diverges from actual, update `fraiseql.toml`:
   ```toml
   [fraiseql.database]
   max_connections = 30  # was 20
   ```
3. Perform graceful restart (rolling deployment or `SIGTERM` + health check drain)

## Configuration

```toml
[pool_tuning]
enabled = true
poll_interval_secs = 30
min_pool_size = 5
max_pool_size = 50
scale_up_step = 5
scale_down_step = 2
samples_before_action = 3
target_queue_depth = 2
scale_down_idle_ratio = 0.5
```

## Future Work

Runtime resize support is tracked for v2.2.0. The most likely path is adding a `resize()` method upstream to `deadpool-postgres`, or implementing a pool wrapper that can drain-and-swap transparently.
