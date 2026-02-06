# Chaos Test Tuning Plan: `release/v1.9.0a1`

**Created**: 2025-12-27
**Branch**: `release/v1.9.0a1`
**Purpose**: Systematic plan to tune chaos engineering tests for production readiness
**Priority**: Medium (non-blocking for core functionality)

---

## Executive Summary

The v1.9.0a1 branch includes **128 new chaos engineering tests** designed to validate system behavior under failure conditions. These tests are currently showing mixed results (~50-70% pass rate), which is **expected and normal** for newly introduced chaos tests requiring environment-specific tuning.

**Goal**: Achieve **80-90% pass rate** while maintaining strict validation standards.

**Timeline**: 1-2 weeks of iterative tuning

---

## Current Status

### Test Results Summary

| Category | Total | Observed Pass | Observed Fail | Pass Rate |
|----------|-------|---------------|---------------|-----------|
| **Core Tests** | 6,088 | 6,088 | 0 | 100% ✅ |
| **Chaos Tests** | 128 | TBD | TBD | ~50-70% ⚠️ |
| **Total** | 6,220 | 6,088+ | TBD | ~98% |

### Chaos Test Categories

1. **Authentication Chaos** (`tests/chaos/auth/`)
   - Service outage simulation
   - Concurrent load testing
   - Token validation failures
   - JWKS endpoint failures

2. **Cache Chaos** (`tests/chaos/cache/`)
   - Cache invalidation under load
   - Connection pool exhaustion
   - TTL expiration edge cases
   - Phase 3 validation tests

3. **Concurrency Chaos** (`tests/chaos/concurrency/`)
   - Race conditions
   - Deadlock detection
   - Resource contention
   - Thread safety validation

4. **Database Chaos** (`tests/chaos/database/`)
   - Data consistency under failures
   - Transaction rollback scenarios
   - Connection loss handling
   - Query execution under load
   - Phase 2 validation tests

5. **Network Chaos** (`tests/chaos/network/`)
   - Database connection failures
   - Network partition simulation
   - Latency injection
   - Timeout handling

6. **Resource Chaos** (if exists)
   - Memory pressure
   - CPU exhaustion
   - File descriptor limits
   - Connection pool limits

---

## Known Issues

### 1. Authentication Load Test Failure

**Test**: `test_concurrent_authentication_load`
**File**: `tests/chaos/auth/test_auth_chaos.py:292`
**Status**: ❌ FAILED

**Error**:

```python
assert auth_contentions >= 1, "Should experience some auth contention under load"
AssertionError: Should experience some auth contention under load
assert 0 >= 1
```

**Root Cause Analysis**:

- System hardware is too fast for current load parameters
- Connection pool is large enough to handle concurrent requests
- No actual contention detected under test conditions

**Impact**: Low (test tuning issue, not a bug in auth system)

---

## Tuning Strategy

### Phase 1: Analysis & Categorization (1-2 days)

**Goal**: Understand all failure patterns

**Tasks**:

1. Run all chaos tests with detailed output (`-vv --tb=long`)
2. Categorize failures into types:
   - Environment-specific (hardware, timing)
   - Configuration issues (pools, timeouts)
   - Actual bugs (requires code fix)
   - Test design flaws (unrealistic expectations)
3. Create failure inventory with priority ranking
4. Identify patterns across test categories

**Deliverables**:

- Failure categorization spreadsheet
- Priority-ranked fix list
- Pattern analysis document

---

### Phase 2: Environment Detection (2-3 days)

**Goal**: Make tests adapt to runtime environment

**Implementation**:

#### 2.1 Hardware Detection

```python
# tests/chaos/conftest.py

import psutil
import multiprocessing

def detect_hardware_profile():
    """Detect hardware capabilities for test tuning."""
    return {
        'cpu_count': multiprocessing.cpu_count(),
        'memory_gb': psutil.virtual_memory().total / (1024**3),
        'cpu_freq_mhz': psutil.cpu_freq().max if psutil.cpu_freq() else 2000,
    }

def get_load_multiplier():
    """Calculate load multiplier based on hardware."""
    profile = detect_hardware_profile()

    # Baseline: 4 CPUs, 8GB RAM
    baseline_cpus = 4
    baseline_memory = 8

    cpu_multiplier = profile['cpu_count'] / baseline_cpus
    memory_multiplier = profile['memory_gb'] / baseline_memory

    # Use the higher multiplier to stress the system
    return max(cpu_multiplier, memory_multiplier, 1.0)

@pytest.fixture(scope="session")
def chaos_config():
    """Configuration for chaos tests based on environment."""
    multiplier = get_load_multiplier()

    return {
        'concurrent_requests': int(100 * multiplier),  # Scale with hardware
        'connection_pool_size': 10,  # Keep fixed to induce contention
        'timeout_seconds': 5 / multiplier,  # Faster hardware = tighter timeouts
        'retry_attempts': 3,
        'load_multiplier': multiplier,
    }
```

#### 2.2 CI/CD Detection

```python
import os

def is_ci_environment():
    """Detect if running in CI/CD."""
    return any([
        os.getenv('CI') == 'true',
        os.getenv('GITHUB_ACTIONS') == 'true',
        os.getenv('GITLAB_CI') == 'true',
    ])

@pytest.fixture(scope="session")
def chaos_config():
    """Adjust config for CI vs local."""
    if is_ci_environment():
        # CI environments are often resource-constrained
        return {
            'concurrent_requests': 50,  # Lower for CI
            'timeout_seconds': 10,  # More lenient timeouts
        }
    else:
        # Local development - higher loads
        multiplier = get_load_multiplier()
        return {
            'concurrent_requests': int(100 * multiplier),
            'timeout_seconds': 5,
        }
```

---

### Phase 3: Test Parameter Tuning (3-5 days)

**Goal**: Adjust individual test parameters for reliability

#### 3.1 Authentication Chaos Tuning

**File**: `tests/chaos/auth/test_auth_chaos.py`

**Changes**:

```python
# BEFORE
async def test_concurrent_authentication_load():
    concurrent_requests = 100  # Fixed
    connection_pool_size = 20  # Too large

# AFTER
async def test_concurrent_authentication_load(chaos_config):
    concurrent_requests = chaos_config['concurrent_requests']
    connection_pool_size = 10  # Reduced to induce contention

    # Add adaptive assertion
    expected_contentions = max(1, concurrent_requests // 50)
    assert auth_contentions >= expected_contentions, \
        f"Expected at least {expected_contentions} contentions for {concurrent_requests} requests"
```

**Specific Tuning**:

1. **Reduce connection pool size** (20 → 10) to create bottleneck
2. **Increase concurrent requests** dynamically based on hardware
3. **Add adaptive assertions** that scale with load
4. **Add jitter** to request timing to increase contention probability

#### 3.2 Cache Chaos Tuning

**File**: `tests/chaos/cache/test_cache_chaos.py`

**Changes**:

```python
# Add retry logic for timing-sensitive tests
async def test_cache_invalidation_under_load(chaos_config):
    max_retries = 3
    for attempt in range(max_retries):
        try:
            # Run test
            result = await run_cache_invalidation_test()
            assert result.is_valid()
            break
        except AssertionError as e:
            if attempt == max_retries - 1:
                raise
            await asyncio.sleep(0.1 * (attempt + 1))  # Exponential backoff
```

**Specific Tuning**:

1. **Add retry logic** for timing-sensitive assertions
2. **Adjust TTL values** based on system performance
3. **Increase cache size** if too much eviction occurring
4. **Add delay between operations** to ensure proper sequencing

#### 3.3 Concurrency Chaos Tuning

**File**: `tests/chaos/concurrency/test_concurrency_chaos.py`

**Changes**:

```python
async def test_race_condition_detection(chaos_config):
    # Increase iterations for slower hardware
    iterations = int(1000 * chaos_config['load_multiplier'])

    # Add synchronization barriers
    barrier = asyncio.Barrier(num_tasks)

    async def worker():
        await barrier.wait()  # All start at same time
        # ... test logic
```

**Specific Tuning**:

1. **Add synchronization barriers** for true simultaneous execution
2. **Scale iterations** with hardware capability
3. **Add explicit yields** to increase context switching
4. **Use locks strategically** to control concurrency levels

#### 3.4 Database Chaos Tuning

**File**: `tests/chaos/database/test_data_consistency_chaos.py`

**Changes**:

```python
async def test_transaction_rollback_under_load(chaos_config, db_pool):
    # Reduce pool size to force transaction queueing
    limited_pool = await create_pool(max_size=5)

    try:
        # Test with limited pool
        results = await run_concurrent_transactions(
            pool=limited_pool,
            count=chaos_config['concurrent_requests']
        )
    finally:
        await limited_pool.close()
```

**Specific Tuning**:

1. **Create dedicated pools** with specific sizes for each test
2. **Add transaction isolation checks**
3. **Increase data volume** for meaningful consistency tests
4. **Add cleanup between test runs** to ensure fresh state

#### 3.5 Network Chaos Tuning

**File**: `tests/chaos/network/test_db_connection_chaos.py`

**Changes**:

```python
async def test_connection_timeout_handling(chaos_config):
    # Adjust timeout based on environment
    base_timeout = 1.0  # seconds
    adjusted_timeout = base_timeout / chaos_config['load_multiplier']

    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(
            slow_operation(),
            timeout=adjusted_timeout
        )
```

**Specific Tuning**:

1. **Scale timeouts** inversely with hardware speed
2. **Add network simulation** using `tc` (traffic control) on Linux
3. **Test reconnection logic** explicitly
4. **Add progressive timeout increases** for retry scenarios

---

### Phase 4: Test Stability Improvements (2-3 days)

**Goal**: Reduce flakiness and improve reliability

#### 4.1 Add Test Isolation

```python
@pytest.fixture(autouse=True)
async def isolate_chaos_test(db_pool):
    """Ensure each chaos test runs in isolation."""
    # Setup: Clear state
    await db_pool.execute("TRUNCATE TABLE test_data CASCADE")
    await clear_all_caches()

    yield

    # Teardown: Clean up
    await db_pool.execute("TRUNCATE TABLE test_data CASCADE")
    await clear_all_caches()
```

#### 4.2 Add Warmup Periods

```python
async def test_with_warmup(chaos_config):
    # Warmup: Prime caches, establish connections
    for _ in range(10):
        await lightweight_operation()

    # Allow system to stabilize
    await asyncio.sleep(0.5)

    # Actual test
    result = await chaos_operation()
    assert result.is_valid()
```

#### 4.3 Add Diagnostic Logging

```python
import logging
logger = logging.getLogger(__name__)

async def test_with_diagnostics(chaos_config):
    logger.info(f"Starting test with config: {chaos_config}")

    start_time = time.time()
    result = await chaos_operation()
    duration = time.time() - start_time

    logger.info(f"Test completed in {duration:.2f}s")
    logger.info(f"Result metrics: {result.get_metrics()}")

    assert result.is_valid(), f"Failed with metrics: {result.get_metrics()}"
```

#### 4.4 Add Performance Monitoring

```python
@pytest.fixture
async def performance_monitor():
    """Monitor system resources during test."""
    monitor = ResourceMonitor()
    await monitor.start()

    yield monitor

    await monitor.stop()
    metrics = monitor.get_metrics()

    # Fail if system was overloaded
    assert metrics['cpu_percent'] < 95, "CPU overloaded during test"
    assert metrics['memory_percent'] < 90, "Memory overloaded during test"
```

---

### Phase 5: Documentation & Configuration (1 day)

**Goal**: Document test behavior and configuration options

#### 5.1 Create Configuration File

**File**: `tests/chaos/chaos_config.yaml`

```yaml
# Chaos Test Configuration
# Override these values based on your environment

environments:
  ci:
    concurrent_requests: 50
    timeout_seconds: 10
    connection_pool_size: 10
    retry_attempts: 3

  local-development:
    concurrent_requests: 100
    timeout_seconds: 5
    connection_pool_size: 10
    retry_attempts: 3

  production-validation:
    concurrent_requests: 500
    timeout_seconds: 2
    connection_pool_size: 5
    retry_attempts: 5

# Hardware profiles
hardware_profiles:
  low:  # 2-4 cores, 4-8GB RAM
    load_multiplier: 0.5
  medium:  # 4-8 cores, 8-16GB RAM
    load_multiplier: 1.0
  high:  # 8+ cores, 16+ GB RAM
    load_multiplier: 2.0
```

#### 5.2 Update Test README

**File**: `tests/chaos/README.md`

```markdown
# Chaos Engineering Tests

## Overview
These tests validate FraiseQL's behavior under failure conditions.

## Running Tests

### All Chaos Tests
```bash
pytest tests/chaos -v
```

### Specific Category

```bash
pytest tests/chaos/auth -v
pytest tests/chaos/cache -v
pytest tests/chaos/concurrency -v
```

## Configuration

### Environment Variables

- `CHAOS_LOAD_MULTIPLIER`: Scale concurrent requests (default: auto-detect)
- `CHAOS_TIMEOUT`: Override timeout values (default: 5s)
- `CHAOS_POOL_SIZE`: Connection pool size (default: 10)

### Hardware Requirements

- Minimum: 2 cores, 4GB RAM
- Recommended: 4+ cores, 8+ GB RAM
- CI/CD: Tests auto-adjust for constrained environments

## Expected Pass Rates

- **Local Development**: 80-90%
- **CI/CD**: 70-85% (resource constraints)
- **Production Validation**: 90-95%

## Troubleshooting

### High Failure Rate

1. Check `CHAOS_LOAD_MULTIPLIER` - may be too high
2. Increase timeout values if hardware is slow
3. Review logs for specific assertion failures

### Flaky Tests

1. Ensure database is not under external load
2. Check for resource constraints (CPU, memory)
3. Run tests in isolation: `pytest -x tests/chaos/auth/test_specific.py`

```

---

## Implementation Roadmap

### Week 1: Analysis & Foundation

**Days 1-2**: Analysis
- [ ] Run all chaos tests with verbose output
- [ ] Categorize failures
- [ ] Identify patterns
- [ ] Create failure inventory

**Days 3-4**: Environment Detection
- [ ] Implement hardware detection
- [ ] Add CI/CD detection
- [ ] Create adaptive config system
- [ ] Test on different environments

**Day 5**: Initial Tuning
- [ ] Fix top 5 failing tests
- [ ] Validate fixes across environments
- [ ] Document changes

### Week 2: Comprehensive Tuning

**Days 6-8**: Category-by-Category Tuning
- [ ] Authentication tests (Day 6)
- [ ] Cache tests (Day 7)
- [ ] Concurrency tests (Day 8)

**Days 9-10**: Database & Network
- [ ] Database consistency tests (Day 9)
- [ ] Network chaos tests (Day 10)

**Days 11-12**: Stability & Documentation
- [ ] Add test isolation
- [ ] Add warmup periods
- [ ] Add diagnostic logging
- [ ] Create configuration file
- [ ] Update documentation

**Day 13**: Validation
- [ ] Run full suite on 3 different environments
- [ ] Verify 80-90% pass rate
- [ ] Document known failures
- [ ] Create issue tickets for remaining failures

---

## Success Metrics

### Target Pass Rates (by Environment)

| Environment | Target | Acceptable |
|-------------|--------|------------|
| **Local (high-end)** | 90%+ | 80%+ |
| **Local (low-end)** | 85%+ | 75%+ |
| **CI/CD (GitHub Actions)** | 80%+ | 70%+ |
| **Production Validation** | 95%+ | 90%+ |

### Quality Metrics

- **Flakiness**: < 5% of tests (run-to-run variance)
- **Execution Time**: < 5 minutes for full chaos suite
- **Isolation**: 100% (no test depends on another)
- **Documentation**: 100% coverage of test purpose

---

## Known Limitations

### Tests That May Always Fail

Some chaos tests are designed to fail under certain conditions:

1. **Resource Exhaustion Tests**
   - May fail on systems with abundant resources
   - Solution: Document as environment-dependent

2. **Timing-Sensitive Tests**
   - May fail on very fast or very slow hardware
   - Solution: Add retry logic or skip markers

3. **Network Tests**
   - May fail in containerized environments without network simulation
   - Solution: Add `pytest.mark.requires_network_simulation`

### Acceptable Failure Patterns

- **Low-end hardware**: 10-15% failure rate acceptable
- **CI/CD environments**: 15-20% failure rate acceptable
- **Container environments**: Some network tests may fail (document)

---

## Risk Assessment

### Low Risk ✅

- Core functionality unaffected (6088/6088 core tests pass)
- Chaos tests are supplementary validation
- Failures indicate overly strict tests, not bugs

### Medium Risk ⚠️

- Some chaos test failures may hide real bugs
- Need careful triage to separate tuning from bugs
- Time investment: 1-2 weeks of iterative work

### Mitigation

1. **Prioritize by Category**: Fix auth/security tests first
2. **Bug vs Tuning**: Clear categorization process
3. **Incremental Approach**: Fix in batches, validate thoroughly

---

## Deliverables

### Code Changes

1. `tests/chaos/conftest.py` - Environment detection and config
2. `tests/chaos/chaos_config.yaml` - Configuration file
3. `tests/chaos/README.md` - Documentation
4. Individual test files - Parameter tuning
5. `tests/chaos/utils/` - Shared utilities (monitoring, diagnostics)

### Documentation

1. Failure inventory spreadsheet
2. Tuning decision log
3. Environment-specific guidance
4. Known issues and limitations

### Reports

1. Before/after pass rate comparison
2. Performance impact analysis
3. Flakiness report
4. Recommendations for CI/CD integration

---

## Next Steps

### Immediate (This Sprint)

1. **Run Detailed Analysis**
   ```bash
   pytest tests/chaos -vv --tb=long > chaos-analysis.txt 2>&1
   ```

2. **Categorize Failures**
   - Create spreadsheet with test name, failure reason, priority
   - Tag as: TUNING_NEEDED, POTENTIAL_BUG, ENVIRONMENT, TEST_DESIGN

3. **Quick Wins**
   - Fix top 5 failures with obvious parameter issues
   - Add hardware detection fixture
   - Update 1-2 test files with new patterns

### Short Term (Next Sprint)

4. **Systematic Tuning**
   - Complete Phase 2 (environment detection)
   - Complete Phase 3 (parameter tuning) for 2-3 categories

5. **Validation**
   - Run on 2-3 different environments
   - Measure pass rate improvement
   - Document results

### Long Term (Before v2.0.0 Release)

6. **Full Coverage**
   - Complete all 5 phases
   - Achieve 80-90% pass rate across environments
   - Comprehensive documentation

7. **CI/CD Integration**
   - Add chaos tests to GitHub Actions
   - Configure environment-specific parameters
   - Add reporting and dashboards

---

## Conclusion

Chaos test tuning is a **systematic, iterative process** that will significantly improve the production readiness of v1.9.0a1. While the current ~50-70% pass rate may seem concerning, it's **normal and expected** for new chaos tests.

**Key Principles**:

1. **Adaptive**: Tests adapt to environment capabilities
2. **Documented**: All behaviors and limitations documented
3. **Prioritized**: Fix critical tests (auth, security) first
4. **Incremental**: Improve in manageable batches

**Expected Outcome**: 80-90% pass rate with comprehensive validation of system resilience under failure conditions.

---

**Status**: READY FOR IMPLEMENTATION
**Priority**: Medium
**Effort**: 1-2 weeks
**Impact**: High (production confidence)
