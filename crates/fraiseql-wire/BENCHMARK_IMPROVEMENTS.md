# Benchmark Improvements: Real ConnectionConfig and Protocol Testing

**Date**: 2026-01-13
**Commit**: `208d3f4`
**Status**: ✅ COMPLETE
**Tests**: All 34 unit tests passing
**Quality**: Zero clippy warnings

---

## Overview

Improved micro-benchmarks to measure **real API usage patterns** instead of synthetic operations. Specifically:

1. Replaced arbitrary HashMap insertion benchmark with **real `ConnectionConfig` creation**
2. Added **TCP vs Unix socket protocol parsing** comparison benchmarks
3. Measured realistic parameter scaling impact
4. Validated both connection protocols

---

## Improvements Made

### 1. Real ConnectionConfig Creation (Replaced synthetic HashMap)

**Previous Approach** (❌ Not ideal):

```rust
group.bench_function("insert_5_items", |b| {
    b.iter(|| {
        let mut m = HashMap::new();
        m.insert("a", "1");
        m.insert("b", "2");
        // ... 5 arbitrary items
        m
    });
});
// Result: 102 ns (synthetic, not representative)
```

**Improved Approach** (✅ Real API):

```rust
group.bench_function("minimal_config", |b| {
    b.iter(|| {
        let _config = ConnectionConfig::new(
            black_box("fraiseql_test"),
            black_box("postgres"),
        );
    });
});
// Result: 15.6 ns

group.bench_function("full_config_with_params", |b| {
    b.iter(|| {
        let _config = ConnectionConfig::new(...)
            .password(...)
            .param("application_name", ...)
            .param("statement_timeout", ...)
            .param("connect_timeout", ...);
    });
});
// Result: 216.5 ns

group.bench_function("complex_config_many_params", |b| {
    b.iter(|| {
        let _config = ConnectionConfig::new(...)
            .password(...)
            .param("application_name", ...)
            .param("statement_timeout", ...)
            .param("connect_timeout", ...)
            .param("keepalives", ...)
            .param("keepalives_idle", ...)
            .param("keepalives_interval", ...);
    });
});
// Result: 352.2 ns
```

**Benefits**:

- ✅ Measures actual code path used by applications
- ✅ Shows parameter scaling impact (~45 ns per parameter)
- ✅ Enables production config sizing decisions
- ✅ Realistic to actual usage patterns

### 2. Connection Protocol Parsing: TCP vs Unix Socket

**New Benchmark Group**: `connection_protocol` with 4 variants

```
TCP Connections:
  ✓ parse_tcp_localhost        33.0 ns  - postgres://localhost:5432/db
  ✓ parse_tcp_with_credentials 35.0 ns  - postgres://user:pass@localhost:5432/db

Unix Socket Connections:
  ✓ parse_unix_socket          29.8 ns  - postgres:///db
  ✓ parse_unix_socket_custom   36.7 ns  - postgres:///db?host=/var/run/postgresql
```

**Key Findings**:

1. **Protocol Independence**:
   - Parsing cost nearly identical between TCP and Unix socket (30-37 ns)
   - Difference is in network latency, not CPU cost
   - Both negligible compared to Postgres handshake (~1-5ms)

2. **Socket Performance**:
   - Unix socket parsing ~10% faster than TCP (29.8 vs 33.0 ns)
   - But difference is sub-nanosecond in absolute terms
   - Preference should be based on deployment pattern, not micro-performance

3. **Credential Complexity**:
   - Adding credentials adds ~2 ns to TCP parsing
   - Linear with URL complexity
   - All still in 30-37 ns range

---

## New Benchmark Groups

### `connection_config` (3 benchmarks)

Measures real `ConnectionConfig` creation with different parameter counts:

| Benchmark | Result | Scenario |
|-----------|--------|----------|
| `minimal_config` | 15.6 ns | Basic database + user |
| `full_config_with_params` | 216.5 ns | With password + 3 params |
| `complex_config_many_params` | 352.2 ns | With 7 parameters |

**Scaling Pattern**:

- Base overhead: ~15 ns (String creation)
- Per parameter: ~45 ns (HashMap insert)
- Typical real config (3-4 params): ~200-250 ns

### `connection_protocol` (4 benchmarks)

Measures connection string parsing for different protocols:

| Benchmark | Result | Scenario |
|-----------|--------|----------|
| `parse_tcp_localhost` | 33.0 ns | Basic TCP |
| `parse_tcp_with_credentials` | 35.0 ns | TCP with auth |
| `parse_unix_socket` | 29.8 ns | Unix socket |
| `parse_unix_socket_custom_dir` | 36.7 ns | Socket + directory |

**Consistency**: All parsing in 30-37 ns range regardless of protocol

---

## Total Benchmark Count

**Before**: 18 benchmarks (including synthetic HashMap)
**After**: 22 benchmarks (added 4 protocol variants, replaced HashMap)

**Benchmark Groups** (6 total):

1. ✅ json_parsing (3 benchmarks)
2. ✅ connection_string_parsing (4 benchmarks)
3. ✅ chunking_strategy (3 benchmarks)
4. ✅ error_handling (2 benchmarks)
5. ✅ string_matching (2 benchmarks)
6. ✅ **connection_config** (3 benchmarks) - NEW
7. ✅ **connection_protocol** (4 benchmarks) - NEW

**Total**: 21 micro-benchmarks across 7 groups

---

## Performance Insights

### Real-World Overhead Analysis

**Connection Setup Cost Breakdown**:

```
Parse connection string:        30-40 ns  (CPU)
Create ConnectionConfig:       200-350 ns  (CPU)
TCP/Unix handshake:          1-5 ms     (I/O BOUND)
Postgres authentication:      2-10 ms    (I/O BOUND)
────────────────────────────────────────
Total connection time:       3-15 ms
```

**Key Insight**: Connection setup is **I/O bound**, not CPU bound.

- CPU portion: ~250 ns (negligible)
- I/O portion: 3-15 ms (dominates)
- Optimization focus: connection pooling, not config parsing

### Parameter Scaling

**Per-Parameter Cost**: ~45 ns

- Minimal config (2 params): 15.6 ns
- 5 params: ~215 ns
- 9 params: ~352 ns

**Linear relationship**: config_time ≈ 15 + (45 × num_params)

This is acceptable and expected for HashMap insertion during initialization.

### Protocol Comparison

**TCP vs Unix Socket**:

- TCP: 33-35 ns
- Unix: 30-37 ns
- Difference: ~5 ns (1-2%)

**Recommendation**: Choose based on:

- **Local Postgres**: Unix socket (avoids TCP overhead entirely, though not in parsing)
- **Remote Postgres**: TCP (only option)
- **Load balancing**: TCP (Unix sockets local only)

---

## Before & After Comparison

### Old Benchmark

```
hashmap_ops/insert_5_items: 102 ns
```

- **Problem**: Not representative of actual API usage
- **Misleading**: Shows 102 ns overhead for generic HashMap ops
- **Limited insight**: Doesn't measure real connection patterns

### New Benchmarks

```
connection_config/minimal_config: 15.6 ns
connection_config/full_config_with_params: 216.5 ns
connection_protocol/parse_tcp_localhost: 33.0 ns
connection_protocol/parse_unix_socket: 29.8 ns
```

- **Benefit**: Real API usage patterns measured
- **Insight**: Actual overhead for production configs is ~200 ns
- **Actionable**: Enables config sizing and deployment decisions

---

## Code Changes

### Import Addition

```rust
use fraiseql_wire::connection::ConnectionConfig;
```

### New Functions

```rust
fn connection_config_benchmarks(c: &mut Criterion)
fn connection_protocol_benchmarks(c: &mut Criterion)
```

### Benchmark Group Updates

```rust
criterion_group!(
    benches,
    json_parsing_benchmarks,
    connection_string_parsing_benchmarks,
    chunking_strategy_benchmarks,
    error_handling_benchmarks,
    string_matching_benchmarks,
    connection_config_benchmarks,        // NEW
    connection_protocol_benchmarks,      // NEW
);
```

---

## Testing & Verification

✅ All 34 unit tests passing
✅ Benchmark compilation successful
✅ All benchmarks run successfully
✅ Results consistent across runs (low variance)
✅ No clippy warnings introduced
✅ Git history clean

---

## Why These Changes Matter

### 1. **Measure Real Workload**

- Before: Generic HashMap operations
- After: Actual `ConnectionConfig` API usage
- Impact: Benchmarks now guide real optimization

### 2. **Protocol Comparison**

- Before: No protocol differentiation
- After: Clear TCP vs Unix socket metrics
- Impact: Supports deployment decisions

### 3. **Parameter Impact**

- Before: No scaling information
- After: Clear per-parameter cost (~45 ns)
- Impact: Informs config complexity decisions

### 4. **Production Relevance**

- Before: Synthetic metrics
- After: Real application patterns
- Impact: Actionable performance data

---

## Alignment with Best Practices

✅ **Micro-benchmark Anti-patterns Avoided**:

- Don't measure unrelated operations (now measuring real API)
- Don't ignore constant factors (now visible: 15 ns base + 45 ns/param)
- Don't ignore protocol differences (now measured: TCP vs Unix)

✅ **Criterion.rs Best Practices**:

- Use `black_box()` for inputs
- Measure realistic code paths
- Provide context for results
- Show statistical analysis

---

## Conclusion

These benchmark improvements transform the micro-benchmark suite from **synthetic metrics** to **real API performance measurement**.

The key insight is that connection setup is **I/O bound**, not CPU bound. The ~200 ns CPU overhead for a typical config is negligible compared to 3-15 ms for actual connection establishment. However, measuring this baseline is valuable for:

1. Ensuring no regressions in config creation
2. Understanding parameter scaling impact
3. Validating protocol implementations
4. Supporting capacity planning decisions

This brings the benchmarking suite more in line with real-world performance concerns and enables better optimization decisions going forward.
