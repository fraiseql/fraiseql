# Phase 08: Studio Metrics Backend

## Objective

Wire the `GET /admin/v1/metrics/summary` endpoint to real metric collectors,
replacing the `MetricsSummary::zero()` placeholder. The response shape is
already agreed with the Luxen UI author — only the data source changes.

## Success Criteria

- [ ] `summary_handler` reads from `AppState.metrics` (the live `MetricsCollector`)
- [ ] `latency` fields are computed from `http_request_duration` histogram (P50/P95/P99)
- [ ] `errors.rate_*` fields are computed from rolling counters over 5m/1h/24h windows
- [ ] `cache.hit_rate` is computed from `cache_hits / (cache_hits + cache_misses)`
- [ ] `pool.*` fields are populated from the deadpool stats API
- [ ] `subscriptions.active` reflects the live subscription count
- [ ] Integration test verifies non-zero values after real requests

## Background

### Existing data sources in `AppState`

| `MetricsSummary` field | Source | Notes |
|----------------------|--------|-------|
| `latency.p50/p95/p99` | `MetricsCollector.http_request_duration` (HDR histogram) | P-quantile via `value_at_quantile()` |
| `errors.rate_5m/1h/24h` | `queries_error` / `queries_total` | Need sliding window — see Cycle 1 |
| `cache.hit_rate` | `cache_hits` / (`cache_hits` + `cache_misses`) | Simple ratio |
| `cache.entries` | Not yet in `MetricsCollector` — needs addition | `AppState` has executor → schema |
| `pool.active/idle/max` | `deadpool::managed::Pool::status()` | Already exposed on `AppState.executor` |
| `pool.utilization` | Computed: `active / max` | |
| `subscriptions.active` | `AppState.realtime` presence tracker | `presence.active_rooms_count()` or subscription count |

### Error rate sliding window

`MetricsCollector` has cumulative counters only. For rolling error rates, two options:

1. **Simple**: snapshot counters at endpoint call time and compute rate from deltas
   across a background ticker that records periodic snapshots (ring buffer of 288
   5-minute buckets for 24h). Add to `MetricsCollector`.
2. **Approximate**: expose only the instantaneous ratio `errors/total` as a proxy.
   Label clearly as "lifetime rate, not windowed". Lower effort.

Recommend option 2 for v2.3.0 (accurate shape, approximate value) with a
`// TODO: replace with windowed rate in v2.4.0` comment.

## TDD Cycles

### Cycle 1: Read Latency from Histogram

- **RED**: Write test asserting `summary_handler` returns non-zero `p50_ms` after
  recording requests into `MetricsCollector.http_request_duration`
- **GREEN**: Read `http_request_duration.value_at_quantile(0.5 / 0.95 / 0.99)`;
  convert µs → ms; populate `latency` struct
- **REFACTOR**: Extract `fn latency_from_collector(c: &MetricsCollector) -> LatencyStats`
- **CLEANUP**: Unit test for edge case (empty histogram → zero)

### Cycle 2: Cache and Error Rates

- **RED**: Write test asserting `hit_rate` is correct after recording hits/misses
- **GREEN**: Compute `cache.hit_rate = hits / (hits + misses)` (0.0 when both zero);
  compute error rates as lifetime ratio (approximate); add `// TODO` comment
- **REFACTOR**: Extract helpers; ensure division-by-zero is handled
- **CLEANUP**: Unit test for zero-counters edge case

### Cycle 3: Pool Stats

- **RED**: Write test asserting pool fields are non-zero when pool exists in state
- **GREEN**: Call `executor.adapter().pool().status()` to get `deadpool::Status`;
  map to `PoolStats`; set zero if pool is not accessible (e.g. `FailingAdapter`)
- **REFACTOR**: Keep pool access behind an optional method on the adapter trait,
  or fall back gracefully when pool is unavailable
- **CLEANUP**: Confirm zero-pool fallback works in unit tests with `FailingAdapter`

### Cycle 4: Active Subscription Count

- **RED**: Write test asserting `subscriptions.active` reflects the presence tracker
- **GREEN**: Read from `AppState.realtime` presence tracker
  (`presence.total_active_connections()` or equivalent)
- **REFACTOR**: Handle case where realtime is not configured (`subscriptions.active = 0`)
- **CLEANUP**: Integration test with full pipeline: subscribe → query summary → assert ≥ 1

## Dependencies

- Requires: Phase 07 (fraiseql-storage repair, so workspace builds cleanly)
- Blocks: Phase 10 (finalize)

## Status

[ ] Not Started
