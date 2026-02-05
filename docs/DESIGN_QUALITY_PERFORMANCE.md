<!-- Skip to main content -->
---
title: Design Quality Performance Guide
description: FraiseQL's design quality analysis is optimized for fast feedback during development and CI/CD pipelines.
keywords: ["performance"]
tags: ["documentation", "reference"]
---

# Design Quality Performance Guide

## Performance Characteristics

FraiseQL's design quality analysis is optimized for fast feedback during development and CI/CD pipelines.

### Latency Targets (SLOs)

| Operation | p50 | p95 | p99 | Target |
|-----------|-----|-----|-----|--------|
| Design audit (minimal schema) | <5ms | <10ms | <20ms | ✅ <10ms p95 |
| Design audit (typical schema) | <10ms | <30ms | <50ms | ✅ <50ms p95 |
| Design audit (100+ types) | <20ms | <50ms | <100ms | ✅ <100ms p95 |
| CLI `lint` command | <50ms | <100ms | <200ms | ✅ <100ms p95 |

### Throughput

- **API Server**: 10,000+ concurrent design audit requests
- **CLI Performance**: Analyze typical schema in <100ms
- **Batch Operations**: Process 1000 schemas in <60s

### Memory Usage

| Schema Size | Memory | Limit |
|------------|--------|-------|
| Minimal (1 type) | <1MB | ✅ |
| Typical (3 subgraphs, 10 types) | <5MB | ✅ |
| Large (100+ types) | <50MB | ✅ |
| Enterprise (1000+ types) | <100MB | ✅ |

## Performance Optimization Tips

### For CLI Usage

```bash
<!-- Code example in BASH -->
# Fastest: Single category audit
FraiseQL lint schema.json --federation  # <50ms

# Typical: Multiple categories
FraiseQL lint schema.json --federation --cost  # <80ms

# Complete: All categories
FraiseQL lint schema.json  # <100ms for typical schema

# Batch processing: JSON output for scripts
for schema in schemas/*.json; do
  FraiseQL lint "$schema" --json | jq '.data.overall_score'
done
```text
<!-- Code example in TEXT -->

### For API Server

```bash
<!-- Code example in BASH -->
# Design audit endpoints are optimized for real-time feedback
POST /api/v1/design/federation-audit    # ~15ms
POST /api/v1/design/cost-audit          # ~20ms
POST /api/v1/design/cache-audit         # ~10ms
POST /api/v1/design/auth-audit          # ~15ms
POST /api/v1/design/audit               # ~50ms (all categories)
```text
<!-- Code example in TEXT -->

### For Large Schemas

If analyzing very large schemas (500+ types):

1. **Split analysis**: Process by subgraph instead of complete schema
2. **Cache results**: Store audit results for unchanged schemas
3. **Async processing**: Use background jobs for non-critical audits
4. **Streaming**: Process one category at a time

Example:

```bash
<!-- Code example in BASH -->
# Instead of complete audit
# FraiseQL lint huge-schema.json  # Might take 150-200ms

# Process by category
FraiseQL lint huge-schema.json --federation --json | jq '.data'
FraiseQL lint huge-schema.json --cost --json | jq '.data'
FraiseQL lint huge-schema.json --cache --json | jq '.data'
```text
<!-- Code example in TEXT -->

## Benchmarking & Profiling

### Run Benchmarks

```bash
<!-- Code example in BASH -->
# Run Criterion benchmarks
cargo bench -p FraiseQL-core --bench design_analysis

# Output: Detailed latency distributions for each schema size
```text
<!-- Code example in TEXT -->

### Profile Memory Usage

```bash
<!-- Code example in BASH -->
# Using valgrind (Linux)
valgrind --tool=massif FraiseQL lint schema.json
ms_print massif.out.<pid>  # View results

# Using Instruments (macOS)
time FraiseQL lint schema.json  # Shows memory usage
```text
<!-- Code example in TEXT -->

### Monitor Performance

```bash
<!-- Code example in BASH -->
# CLI performance monitoring
time FraiseQL lint schema.json --verbose

# API endpoint monitoring
curl -w "@format.txt" -o /dev/null -s \
  -X POST http://localhost:8080/api/v1/design/audit \
  -H "Content-Type: application/json" \
  -d @schema.json
```text
<!-- Code example in TEXT -->

## Performance Regression Testing

Design quality analysis performance is monitored for regressions:

- Rule analysis speed must not degrade
- Memory usage must stay within limits
- Large schema handling must stay <100ms p95

Run regression tests:

```bash
<!-- Code example in BASH -->
# Baseline measurements
cargo bench -p FraiseQL-core --bench design_analysis -- --save-baseline phase4

# Later: Compare against baseline
cargo bench -p FraiseQL-core --bench design_analysis -- --baseline phase4
```text
<!-- Code example in TEXT -->

## Scalability

### Scaling with Schema Size

```text
<!-- Code example in TEXT -->
Schema Size | Analysis Time | Memory
1 type      | <5ms         | <1MB
10 types    | <15ms        | <2MB
100 types   | <40ms        | <10MB
1000 types  | <150ms       | <80MB
```text
<!-- Code example in TEXT -->

Linear time complexity: O(n) where n = number of types + relationships

### Scaling with Federation Depth

```text
<!-- Code example in TEXT -->
Subgraph Depth | Federation Audit | Complexity
1-2 levels     | <20ms           | O(subgraphs × entities)
3-5 levels     | <30ms           | Circular detection enabled
6+ levels      | <50ms           | Optimization active
```text
<!-- Code example in TEXT -->

## Deployment Recommendations

### Development

- Run `FraiseQL lint` locally for instant feedback
- Target: <100ms per audit for developer experience

### CI/CD

- Use `--fail-on-critical` for automated gates
- Typical pipeline: 2-5ms per schema analysis
- Concurrent audits: 10+ schemas in parallel

### Production

- Cache audit results for unchanged schemas
- Rate limit: 100 audits per second per instance
- Recommended: 2-4 instances for high-volume usage

## Known Performance Characteristics

### Fast Operations

✅ Single category audits (federation, cost): <20ms
✅ Well-designed schemas: <50ms
✅ Empty/minimal schemas: <5ms

### Slower Operations

⚠️ First-time analysis of new schemas: +10ms (parsing overhead)
⚠️ Enterprise schemas (1000+ types): Up to 150ms
⚠️ Complete audit (all categories): 50-100ms

### Optimization Opportunities

For future optimization:

- [ ] Parallel category analysis
- [ ] Incremental analysis (only changed entities)
- [ ] Rule result caching
- [ ] SIMD-optimized scoring

## Troubleshooting Performance

### Issue: Lint command takes >200ms

**Cause**: Large schema (500+ types) or slow disk I/O
**Solution**:

```bash
<!-- Code example in BASH -->
# Move schema to memory
FraiseQL lint /tmp/schema.json  # Faster than network drive

# Use filtered audit
FraiseQL lint schema.json --federation  # Faster than complete
```text
<!-- Code example in TEXT -->

### Issue: API audit endpoints timing out

**Cause**: Concurrent requests exceeding server capacity
**Solution**:

```bash
<!-- Code example in BASH -->
# Increase timeouts
curl --max-time 5 http://localhost:8080/api/v1/design/audit

# Use rate limiting
# Configure in FraiseQL-server config
```text
<!-- Code example in TEXT -->

### Issue: Memory usage exceeds 100MB

**Cause**: Processing extremely large schema (1000+ types)
**Solution**:

```bash
<!-- Code example in BASH -->
# Process in batches
split -l 100 schema.json schema_part_
for part in schema_part_*; do
  FraiseQL lint "$part" --json | jq '.data.overall_score'
done
```text
<!-- Code example in TEXT -->

## References

- Benchmark code: `crates/FraiseQL-core/benches/design_analysis.rs`
- Performance tests: `tools/performance_test.sh`
- Security tests: `crates/FraiseQL-server/tests/api_design_security_tests.rs`
