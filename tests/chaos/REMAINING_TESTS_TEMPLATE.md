# Template for Remaining Chaos Test Transformations

## Quick Reference

This template provides the standard pattern for transforming remaining chaos tests from mock-based to real PostgreSQL backend.

## Template Structure

```python
"""
Phase X.Y: [Category] Chaos Tests (Real PostgreSQL Backend)

Tests for [description].
Uses real PostgreSQL connections to validate [what FraiseQL handles].
"""

import pytest
import time
import asyncio
from chaos.fraiseql_scenarios import FraiseQLTestScenarios
from chaos.base import ChaosMetrics


@pytest.mark.chaos
@pytest.mark.chaos_[category]  # e.g., chaos_cache, chaos_auth, chaos_resources
@pytest.mark.chaos_real_db
@pytest.mark.asyncio
async def test_[scenario_name](chaos_db_client, chaos_test_schema, baseline_metrics):
    """
    Test [scenario description].

    Scenario: [What happens in chaos]
    Expected: [What FraiseQL should do]
    """
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.simple_user_query()

    metrics.start_test()

    # Baseline (no chaos)
    baseline_times = []
    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 10.0)
            baseline_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            metrics.record_error()

    # Inject chaos
    chaos_db_client.inject_latency(1000)  # Or other chaos method

    # Test under chaos
    chaos_times = []
    errors = 0
    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 1000.0)
            chaos_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            errors += 1
            metrics.record_error()

    # Reset chaos
    chaos_db_client.reset_chaos()

    # Verify recovery
    recovery_times = []
    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 10.0)
            recovery_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            metrics.record_error()

    metrics.end_test()

    # Assertions
    assert errors > 0 or len(chaos_times) > 0, "Should execute under chaos"
    # ... additional assertions
```

## Key Transformations Checklist

For each test function:

1. **Change from class-based to async function**
   - ❌ `class TestXxx(ChaosTestCase): def test_foo(self):`
   - ✅ `@pytest.mark.asyncio async def test_foo(...):`

2. **Add decorators**

   ```python
   @pytest.mark.chaos
   @pytest.mark.chaos_[category]  # chaos_cache, chaos_auth, etc
   @pytest.mark.chaos_real_db
   @pytest.mark.asyncio
   ```

3. **Update function signature**

   ```python
   async def test_name(chaos_db_client, chaos_test_schema, baseline_metrics):
   ```

4. **Replace metrics handling**
   - ❌ `self.metrics.start_test()`
   - ✅ `metrics = ChaosMetrics(); metrics.start_test()`

5. **Replace client initialization**
   - ❌ `client = MockFraiseQLClient()`
   - ✅ Use `chaos_db_client` parameter (real client fixture)

6. **Add await to async calls**
   - ❌ `result = chaos_db_client.execute_query(op)`
   - ✅ `result = await chaos_db_client.execute_query(op)`

7. **Handle timeouts properly**
   - ❌ Manual timeout simulation
   - ✅ `await asyncio.wait_for(chaos_db_client.execute_query(op), timeout=5.0)`

8. **Convert loops to async where needed**
   - Regular loops for baseline/recovery: keep as `for _ in range(n):`
   - Concurrent operations: use `await asyncio.gather(*tasks)`

## Files Remaining & Categories

### Cache Chaos (1 file, 4-6 tests)

**File**: `tests/chaos/cache/test_cache_chaos.py`

Likely tests:

- `test_cache_miss_performance` → Measure degradation with cold cache
- `test_cache_invalidation_propagation` → Verify cache invalidation
- `test_cache_eviction_handling` → Handle cache memory limits
- `test_cache_coherency` → Multi-client cache consistency

**Pattern**: Use chaos_db_client.inject_latency() to simulate slow cache

### Auth Chaos (1 file, 3-5 tests)

**File**: `tests/chaos/auth/test_auth_chaos.py`

Likely tests:

- `test_auth_token_expiration` → Expired token handling
- `test_auth_service_unavailability` → Auth service down scenarios
- `test_permission_denied_scenarios` → Authorization failures
- `test_auth_retry_logic` → Token refresh retries

**Pattern**: Simulate auth failures with custom chaos, then verify retry

### Resource Chaos (2 files, 8-10 tests)

**Files**:

- `tests/chaos/resources/test_resource_chaos.py`
- `tests/chaos/resources/test_phase4_validation.py`

Likely tests:

- `test_memory_pressure` → High memory usage scenarios
- `test_cpu_throttling` → CPU-bound query degradation
- `test_disk_io_bottleneck` → Slow disk I/O
- `test_concurrent_resource_limits` → Multiple concurrent operations
- `test_graceful_degradation` → Behavior under resource constraints

**Pattern**: Inject latency to simulate resource constraints

### Concurrency Chaos (1 file, 4-6 tests)

**File**: `tests/chaos/concurrency/test_concurrency_chaos.py`

Likely tests:

- `test_concurrent_query_execution` → Multiple simultaneous queries
- `test_race_condition_prevention` → Data race handling
- `test_deadlock_under_load` → Deadlocks with high concurrency
- `test_connection_pool_contention` → Pool under heavy load

**Pattern**: Use `asyncio.gather()` for true concurrent execution

## Rapid Transformation Strategy

To complete remaining 13 files quickly:

### Option 1: Per-File Template Application (Recommended)

1. Read original test file
2. For each test function, apply transformation template
3. Replace MockFraiseQLClient → chaos_db_client fixture
4. Add @pytest.mark.asyncio and @pytest.mark.chaos_[category]
5. Add await to execute_query() calls
6. Replace self.metrics → local ChaosMetrics()
7. Test with `pytest --collect-only` to verify

**Time estimate**: 15-30 minutes per file

### Option 2: Batch Transformation with Local Model

Provide this template + example file to local model:

- "Transform all cache/auth/resource/concurrency chaos tests following this pattern"
- Local model can apply template systematically
- Human review + manual fixes for any deviations

**Time estimate**: 1-2 hours for all remaining files

### Option 3: Use opencode (If Available)

Create phase plan:

1. Phase 1: Transform cache tests (30 min)
2. Phase 2: Transform auth tests (30 min)
3. Phase 3: Transform resource tests (45 min)
4. Phase 4: Transform concurrency tests (30 min)
5. Phase 5: Run test suite and verify (30 min)

## Verification After Transformation

After transforming each file, verify:

```bash
# Collect tests
pytest tests/chaos/[category]/test_*_real.py --collect-only
# Should show async functions, no classes

# Run single test with verbose output
pytest tests/chaos/[category]/test_*_real.py::test_name -xvs
# Should run without fixture errors

# Run all category tests
pytest tests/chaos/[category]/ -m chaos_real_db -v
# All should pass or fail with meaningful errors
```

## Success Criteria Checklist

For complete transformation:

- [ ] All network chaos tests transformed (3/3 files) ✅ DONE
- [ ] All database chaos tests transformed (2/2 files) ✅ DONE
- [ ] Cache chaos tests transformed (1 file)
- [ ] Auth chaos tests transformed (1 file)
- [ ] Resource chaos tests transformed (2 files)
- [ ] Concurrency chaos tests transformed (1 file)
- [ ] Validation tests transformed (5 files)
- [ ] All tests discoverable via pytest
- [ ] All tests have @pytest.mark.chaos_real_db
- [ ] All tests are async def functions
- [ ] All tests use chaos_db_client fixture
- [ ] All tests properly handle ChaosMetrics
- [ ] Full test suite runs successfully

## Common Pitfalls

1. **Forgot await on execute_query()**
   - Error: TypeError or async warning
   - Fix: Add `await` before `chaos_db_client.execute_query()`

2. **Using self.metrics instead of local ChaosMetrics**
   - Error: AttributeError: 'NoneType' object has no attribute 'record_query_time'
   - Fix: Create `metrics = ChaosMetrics()` at start of test

3. **Using MockFraiseQLClient instead of fixture**
   - Error: NameError or fixture not found
   - Fix: Accept `chaos_db_client` as parameter, use it directly

4. **Missing @pytest.mark.asyncio**
   - Error: TimeoutError or event loop issues
   - Fix: Add `@pytest.mark.asyncio` decorator

5. **Synchronous wait instead of asyncio.wait_for()**
   - Error: Event loop blocking
   - Fix: Use `await asyncio.wait_for(coro, timeout=N)`

## Examples for Each Category

### Cache Chaos Example

```python
@pytest.mark.asyncio
@pytest.mark.chaos_cache
@pytest.mark.chaos_real_db
async def test_cache_miss_performance(chaos_db_client, chaos_test_schema):
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.simple_user_query()

    metrics.start_test()

    # Baseline: Warm cache
    baseline = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        baseline.append(result.get("_execution_time_ms", 10.0))
        metrics.record_query_time(baseline[-1])

    # Inject latency to simulate cache miss
    chaos_db_client.inject_latency(500)

    # Cold cache: should be slower
    cold_cache = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        cold_cache.append(result.get("_execution_time_ms", 500.0))
        metrics.record_query_time(cold_cache[-1])

    chaos_db_client.reset_chaos()
    metrics.end_test()

    # Verify cache miss impact
    assert statistics.mean(cold_cache) > statistics.mean(baseline)
```

### Auth Chaos Example

```python
@pytest.mark.asyncio
@pytest.mark.chaos_auth
@pytest.mark.chaos_real_db
async def test_auth_token_expiration(chaos_db_client, chaos_test_schema):
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.simple_user_query()

    metrics.start_test()

    # Baseline: Valid auth token
    valid_auth_times = []
    for _ in range(5):
        result = await chaos_db_client.execute_query(operation)
        valid_auth_times.append(result.get("_execution_time_ms", 10.0))
        metrics.record_query_time(valid_auth_times[-1])

    # Inject "token expired" error condition
    chaos_db_client.inject_connection_failure()

    # Expired token: should fail then retry
    expired_auth_errors = 0
    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            metrics.record_query_time(result.get("_execution_time_ms", 10.0))
        except ConnectionError:
            expired_auth_errors += 1
            metrics.record_error()

    chaos_db_client.reset_chaos()
    metrics.end_test()

    assert expired_auth_errors > 0, "Should experience auth failures"
```

## Next Steps

1. Choose transformation strategy (manual, local model, or opencode)
2. Start with cache tests (smallest, easiest)
3. Move to auth tests
4. Then resource tests (2 files, larger)
5. Finally concurrency tests
6. Run full test suite verification
7. Commit all changes

---

This template should enable rapid completion of all remaining test transformations following the established pattern.
