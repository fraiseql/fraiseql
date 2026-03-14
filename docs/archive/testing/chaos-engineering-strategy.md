# Chaos Engineering Testing Strategy

**Last Updated**: December 21, 2025
**Status**: ✅ Implemented and Active

## Overview

FraiseQL uses a **two-tier testing strategy** that separates correctness validation (standard CI/CD) from resilience validation (chaos engineering tests):

```
Quality Gate CI/CD (Standard)           Chaos Engineering Tests (Separate)
├── Speed: 15-20 minutes                ├── Speed: 45-60 minutes
├── Purpose: Correctness               ├── Purpose: Resilience
├── Trigger: Every PR                  ├── Trigger: Manual + Weekly Schedule
├── Environment: Lightweight           ├── Environment: Real Docker PostgreSQL
└── Blocks Merges: YES ✅              └── Blocks Merges: NO (informational)
```

## Why Separate Chaos Tests?

### Chaos Tests Are Different

**Characteristics that make them unsuitable for standard CI/CD:**

| Aspect | Standard Tests | Chaos Tests |
|--------|----------------|------------|
| **Execution Time** | 15-20 min | 45-60 min |
| **Resource Usage** | Lightweight | Heavy (Docker containers) |
| **Dependencies** | Python only | Docker, PostgreSQL, testcontainers |
| **Deterministic** | Yes (fast feedback) | No (chaos is random by nature) |
| **Purpose** | Verify correctness | Validate resilience |
| **Failure Mode** | "Feature is broken" | "System handles failure gracefully" |

### The Problem with Including Chaos Tests in Standard CI

1. **Too Slow** (45-60 min)
   - Developers would wait 1+ hour per PR
   - Defeats the purpose of fast feedback loops
   - Slows down development velocity

2. **Non-Deterministic Results**
   - Chaos by nature introduces randomness
   - Same test might pass or fail on different runs
   - Creates flaky CI that developers don't trust

3. **Resource Hungry**
   - Full Docker setup per test
   - Multiple containers running concurrently
   - Heavy I/O and network simulation
   - Expensive to run every PR

4. **Different Purpose**
   - Standard CI validates: "Does the feature work?"
   - Chaos tests validate: "Does the feature survive failures?"
   - Mixing them creates unclear feedback

## Implementation

### Quality Gate Workflow (Standard CI/CD)

**File**: `.github/workflows/quality-gate.yml`

Runs on every PR to dev/main branches:

```yaml
# All integration tests, but EXCLUDE chaos tests
uv run pytest tests/integration/ \
  -m 'not enterprise and not chaos' \
  # ... other pytest options
```

**What it validates:**

- Unit tests (core functionality)
- Integration tests (with PostgreSQL)
- Code quality (ruff lint/format)
- Security scanning
- Performance regression detection

**Execution time**: ~15-20 minutes

**Result**: Blocks PR merges if failing ❌ Strict requirement

### Chaos Engineering Workflow (Separate)

**File**: `.github/workflows/chaos-engineering-tests.yml`

Runs chaos tests separately via:

1. **Manual Trigger**: `workflow_dispatch` (on-demand)
2. **Scheduled**: Weekly Sunday 2 AM UTC

```yaml
on:
  workflow_dispatch:          # Manual trigger
  schedule:
    - cron: '0 2 * * 0'      # Weekly: Sunday 2 AM UTC
```

**What it validates:**

- Network chaos (latency, packet loss injection)
- Database chaos (deadlocks, constraints, connection failures)
- Cache chaos (invalidation, corruption, stampede prevention)
- Authentication chaos (token expiration, service outages)
- Resource chaos (memory/CPU exhaustion, I/O contention)
- Concurrency chaos (race conditions, deadlock prevention)

**Execution time**: ~45-60 minutes (includes Docker container startup)

**Result**: Informational only ℹ️ Doesn't block merges

## Test Categories and Markers

### Pytest Markers Used

All chaos tests are marked with multiple markers for granular control:

```python
@pytest.mark.asyncio
@pytest.mark.chaos                    # Base chaos marker
@pytest.mark.chaos_real_db           # Uses real PostgreSQL
@pytest.mark.chaos_<category>        # Specific category
```

**Available markers**:

- `chaos_network` - Network failure scenarios
- `chaos_database` - Database failure scenarios
- `chaos_cache` - Cache failure scenarios
- `chaos_auth` - Authentication failure scenarios
- `chaos_resources` - Resource exhaustion scenarios
- `chaos_concurrency` - Concurrent execution scenarios
- `chaos_validation` - Success criteria validation
- `chaos_verification` - Infrastructure verification

### Running Chaos Tests Locally

```bash
# All chaos tests
pytest tests/chaos -m chaos_real_db -v

# Specific category
pytest tests/chaos/cache -m chaos_real_db -v

# Single test
pytest tests/chaos/cache/test_cache_chaos_real.py::test_cache_invalidation_storm -xvs
```

## Test Statistics

### Overall Suite

```
Total Tests: 6,220
├── Unit Tests: ~5,000 (1-2 minutes)
├── Integration Tests: ~1,000 (5-10 minutes)
└── Chaos Tests: 71 (45-60 minutes)

Standard CI/CD: ~15-20 minutes
Chaos Tests (Separate): ~45-60 minutes
```

### Chaos Test Breakdown

71 chaos tests across 6 categories:

| Category | Tests | Focus |
|----------|-------|-------|
| **Network Chaos** | 6 | Latency, packet loss, connection timeouts |
| **Database Chaos** | 6 | Deadlocks, constraint violations, connection failures |
| **Cache Chaos** | 4 | Invalidation storms, corruption, backend failures |
| **Auth Chaos** | 4 | Token expiration, RBAC policy failures, outages |
| **Resource Chaos** | 6 | Memory/CPU exhaustion, I/O contention |
| **Concurrency Chaos** | 6 | Race conditions, deadlocks, concurrent load |
| **Validation** | 23 | Success criteria, phase-specific validations |
| **Verification** | 6 | Infrastructure setup, fixture isolation |
| **Total** | **71** | |

## Test Execution Patterns

### Three-Phase Test Pattern

All chaos tests follow a consistent pattern:

```python
@pytest.mark.asyncio
@pytest.mark.chaos_<category>
async def test_scenario(chaos_db_client, chaos_test_schema, baseline_metrics):
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.<operation>()

    # PHASE 1: Baseline (no chaos)
    baseline_times = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        baseline_times.append(result.execution_time)

    # PHASE 2: Chaos injection
    chaos_db_client.inject_latency(100)  # or other chaos methods

    chaos_times = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        chaos_times.append(result.execution_time)

    # PHASE 3: Recovery validation
    chaos_db_client.reset_chaos()

    recovery_times = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        recovery_times.append(result.execution_time)

    # Assertions: Verify degradation and recovery
    baseline_avg = statistics.mean(baseline_times)
    chaos_avg = statistics.mean(chaos_times)
    recovery_avg = statistics.mean(recovery_times)

    assert chaos_avg >= baseline_avg, "Chaos should impact performance"
    assert abs(recovery_avg - baseline_avg) < 5, "Should recover to baseline"
```

## Chaos Injection Methods

### Network Chaos

```python
chaos_db_client.inject_latency(milliseconds)      # Simulate slow network
chaos_db_client.inject_packet_loss(percentage)    # Simulate packet loss
chaos_db_client.inject_connection_failure()       # Simulate connection drop
```

### Database Chaos

```python
chaos_db_client.inject_deadlock()                 # Simulate deadlock scenario
chaos_db_client.inject_constraint_violation()     # Violate DB constraints
chaos_db_client.inject_connection_pool_exhaustion() # Exhaust pool
```

### Cache Chaos

```python
chaos_db_client.inject_cache_miss_rate(percentage)  # Force cache misses
chaos_db_client.inject_cache_corruption()           # Corrupt cached values
chaos_db_client.inject_cache_backend_failure()      # Cache service down
```

### Recovery Methods

```python
chaos_db_client.reset_chaos()                     # Clear all chaos
chaos_db_client.reset_metrics()                   # Reset metrics
```

## Running Tests

### In Standard CI/CD (Excluded)

```bash
# Quality gate excludes chaos tests
pytest tests/integration/ -m 'not enterprise and not chaos'
```

### On-Demand

```bash
# Manual workflow trigger via GitHub Actions web UI
# Or: Click "Actions" → "Chaos Engineering Tests" → "Run workflow"
```

### Scheduled (Weekly)

```bash
# Automatically runs: Sunday 2 AM UTC
# Via cron: 0 2 * * 0
```

### Local Development

```bash
# Run all chaos tests locally
pytest tests/chaos -m chaos_real_db -v

# Run specific category
pytest tests/chaos/cache -m chaos_real_db -v

# Run with custom timeout
pytest tests/chaos -m chaos_real_db -v --timeout=120
```

## Workflow Files

### Quality Gate (`quality-gate.yml`)

**Purpose**: Standard PR validation
**Trigger**: Every PR to dev/main
**Duration**: 15-20 minutes
**Status**: Blocks merges

**Key steps**:

1. Unit tests (no external deps)
2. Integration tests (PostgreSQL, exclude chaos & enterprise)
3. Lint & format checks
4. Security scanning
5. Performance regression detection

### Chaos Engineering (`chaos-engineering-tests.yml`)

**Purpose**: Resilience validation
**Trigger**: Manual + Weekly schedule
**Duration**: 45-60 minutes
**Status**: Informational only

**Key steps**:

1. Environment setup (Python, Rust, Docker)
2. Run 71 chaos tests with real PostgreSQL
3. Generate test report
4. Upload results as artifacts

## CI/CD Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    Developer Pushes to GitHub                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
                    ┌────────┴─────────┐
                    │                  │
         ┌──────────▼────────┐   ┌────▼────────────────┐
         │  Quality Gate CI  │   │  Chaos Engineering  │
         │  (Automatic)      │   │  (Manual + Weekly)  │
         └──────────┬────────┘   └────────────────────┘
                    │
         ┌──────────▼──────────┐
         │  Tests Run:         │
         │  • Unit tests       │
         │  • Integration      │
         │  • Exclude chaos    │
         │  • Lint/security    │
         │  • Performance      │
         └──────────┬──────────┘
                    │
         ┌──────────▼──────────┐
         │  Duration:          │
         │  15-20 minutes      │
         └──────────┬──────────┘
                    │
         ┌──────────▼──────────┐
         │  Result:            │
         │  ✅ Blocks Merges   │
         │  (Required Gate)    │
         └─────────────────────┘
```

## Best Practices

### For Developers

1. **Run standard tests locally before pushing**

   ```bash
   make test      # Runs unit + integration (excludes chaos)
   make qa        # Runs tests + lint + format
   ```

2. **Never expect chaos tests in PR feedback**
   - They're intentionally separate
   - Use them manually for resilience validation

3. **Monitor scheduled chaos tests weekly**
   - Check Actions tab Sunday mornings
   - Review any failures that appear

### For Maintainers

1. **Weekly chaos test results review**
   - Check GitHub Actions weekly
   - Document any resilience issues found

2. **Use chaos tests for regression detection**
   - When deploying to staging/production
   - When making architectural changes

3. **Iterate on resilience patterns**
   - Chaos tests are your resilience validators
   - Use findings to improve error handling

## Troubleshooting

### Tests Timing Out

Chaos tests take 45-60 minutes due to Docker container startup. If a test times out:

1. Check Docker is running: `docker ps`
2. Increase timeout if needed: `pytest --timeout=180`
3. Check system resources: `docker stats`

### Container Startup Failures

If PostgreSQL containers fail to start:

1. Verify Docker is running
2. Check available disk space
3. Review testcontainers logs

### Flaky Test Results

Chaos tests may have occasional failures due to random chaos injection:

1. This is expected behavior
2. Re-run failed tests to confirm
3. Check if pattern is consistent

## References

- **[Chaos Engineering Best Practices](https://principlesofchaos.org/)**
- **[GCP Chaos Engineering Guide](https://cloud.google.com/architecture/chaos-engineering-practices)**
- **[Netflix Chaos Monkey](https://netflix.github.io/chaosmonkey/)**
- **[Gremlin: Chaos Engineering Platform](https://www.gremlin.com/)**

## Summary

FraiseQL's chaos engineering strategy:

✅ **Standard CI/CD**: Fast (15-20 min), validates correctness, blocks merges
✅ **Chaos Tests**: Slow (45-60 min), validates resilience, on-demand/scheduled
✅ **Two-Tier Model**: Balances development speed with resilience validation
✅ **71 Test Scenarios**: Comprehensive coverage of failure modes
✅ **Real Infrastructure**: Uses actual PostgreSQL, Docker for authenticity

**Result**: Developers get fast feedback (PRs don't stall), while resilience is validated separately (no compromise on quality).
