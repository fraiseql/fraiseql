# Phase Plan: Fix pgvector Fixture Timing Issue

## Objective
Fix the pytest fixture dependency ordering issue causing pgvector tests to skip in full test suite runs while passing when run individually.

## Context

### Current Problem
- **Symptom**: 37 pgvector tests skip when running full suite (`pytest tests/`)
- **Reality**: Same tests PASS when run individually or in isolation
- **Root Cause**: Session-scoped `pgvector_available` fixture evaluates before postgres container is ready in full suite execution

### Evidence
```bash
# Individual file - works ✓
$ pytest tests/integration/test_langchain_vectorstore_integration.py
13 passed

# Full suite - skips ✗
$ pytest tests/
5318 passed, 44 skipped  # (37 pgvector + 7 other)
```

### Current Implementation
```python
# tests/fixtures/database/database_conftest.py:152
@pytest_asyncio.fixture(scope="session")
async def pgvector_available(postgres_url: str, postgres_container) -> bool:
    """Check if pgvector extension is available."""
    async with await psycopg.AsyncConnection.connect(postgres_url) as conn:
        try:
            await conn.execute("CREATE EXTENSION IF NOT EXISTS vector")
            await conn.commit()
            return True
        except Exception:
            return False
```

**The dependency on `postgres_container` exists but doesn't solve the timing issue in full suite runs.**

---

## Problem Analysis

### Why Does Adding `postgres_container` Dependency Not Work?

1. **Pytest Collection Phase**: When collecting 5000+ tests, pytest builds dependency graph
2. **Fixture Scope Conflict**: Session-scoped fixtures get initialized early in test session
3. **Container Initialization**: `postgres_container` is session-scoped, starts container
4. **Race Condition**: Connection attempts may occur during container startup
5. **Database Readiness**: Container may be "started" but database not yet accepting connections

### Test Execution Flow

```
Full Suite:
┌─────────────────────────────────────────────────────────┐
│ 1. Pytest collects all 5362 tests                      │
│ 2. Session fixtures initialize:                         │
│    a. postgres_container starts (takes 2-3 seconds)    │
│    b. pgvector_available runs immediately after        │
│    c. Database may not be ready → returns False        │
│ 3. All tests execute with pgvector_available=False     │
└─────────────────────────────────────────────────────────┘

Individual File:
┌─────────────────────────────────────────────────────────┐
│ 1. Pytest collects ~13 tests                           │
│ 2. postgres_container starts                           │
│ 3. More time passes before first test                  │
│ 4. pgvector_available runs → DB is ready → True        │
│ 5. Tests execute with pgvector_available=True          │
└─────────────────────────────────────────────────────────┘
```

---

## Solution Options

### Option 1: Add Explicit Wait/Retry Logic ⭐ RECOMMENDED
**Difficulty**: Easy
**Risk**: Low
**Effectiveness**: High

Add retry logic with timeout to `pgvector_available` fixture to handle database startup delay.

**Pros:**
- Simple, targeted fix
- Handles race conditions gracefully
- No changes to test structure
- Works for all container startup scenarios

**Cons:**
- Adds small delay (1-2 seconds) to test suite

**Implementation:**
```python
@pytest_asyncio.fixture(scope="session")
async def pgvector_available(postgres_url: str, postgres_container) -> bool:
    """Check if pgvector extension is available with retry logic."""
    import asyncio

    # Wait for database to be ready (max 10 seconds)
    for attempt in range(10):
        try:
            async with await psycopg.AsyncConnection.connect(
                postgres_url,
                connect_timeout=2
            ) as conn:
                await conn.execute("CREATE EXTENSION IF NOT EXISTS vector")
                await conn.commit()
                return True
        except (psycopg.OperationalError, OSError) as e:
            if attempt < 9:
                await asyncio.sleep(1)
                continue
            return False
        except Exception:
            return False
    return False
```

---

### Option 2: Use Connection Pool Instead of Direct Connection
**Difficulty**: Medium
**Risk**: Medium
**Effectiveness**: High

Change `pgvector_available` to use the `class_db_pool` which has built-in retry logic.

**Pros:**
- Reuses existing infrastructure
- Pool has retry/timeout logic built-in
- More consistent with other fixtures

**Cons:**
- Scope mismatch (session vs class)
- Would need to change fixture scope
- Affects test isolation

**Implementation:**
```python
@pytest_asyncio.fixture(scope="class")  # Changed from session
async def pgvector_available(class_db_pool) -> bool:
    """Check if pgvector extension is available."""
    async with class_db_pool.connection() as conn:
        try:
            await conn.execute("CREATE EXTENSION IF NOT EXISTS vector")
            await conn.commit()
            return True
        except Exception:
            return False
```

**Issue**: Changing scope from `session` to `class` would make the check run multiple times, increasing test execution time.

---

### Option 3: Add Health Check to Container Fixture
**Difficulty**: Medium
**Risk**: Low
**Effectiveness**: High

Modify `postgres_container` fixture to wait until database is accepting connections before yielding.

**Pros:**
- Fixes root cause at source
- Benefits all tests, not just pgvector
- Standard pattern for container fixtures

**Cons:**
- Changes core fixture behavior
- Adds delay to ALL test runs
- May affect other tests expecting immediate container

**Implementation:**
```python
@pytest.fixture(scope="session")
def postgres_container():
    """Start postgres container and wait until ready."""
    container = PostgresContainer(
        image="pgvector/pgvector:pg16",
        username="fraiseql",
        password="fraiseql",
        dbname="fraiseql_test",
    )
    container.start()

    # Wait for database to accept connections
    import psycopg
    import time
    max_attempts = 30
    for attempt in range(max_attempts):
        try:
            conn = psycopg.connect(container.get_connection_url())
            conn.close()
            break  # Success!
        except psycopg.OperationalError:
            if attempt < max_attempts - 1:
                time.sleep(0.5)
            else:
                raise RuntimeError("PostgreSQL container did not become ready")

    yield container
    container.stop()
```

---

### Option 4: Make pgvector_available Non-Caching
**Difficulty**: Easy
**Risk**: Medium
**Effectiveness**: Low

Remove `scope="session"` so fixture re-evaluates for each test.

**Pros:**
- Guarantees fresh check
- Simple change

**Cons:**
- Adds overhead (37 tests × connection time)
- Slower test execution
- Doesn't fix root cause
- Wasteful

**Not recommended.**

---

## Recommended Approach: Hybrid Solution

Combine **Option 1** (retry logic) + **Option 3** (health check) for maximum reliability.

### Phase 1: Add Health Check to Container Fixture (30 min)
This benefits ALL tests and is the "right" way to handle containers.

```python
# tests/fixtures/database/database_conftest.py

@pytest.fixture(scope="session")
def postgres_container():
    """Start postgres container and wait until ready."""
    if not HAS_DOCKER:
        pytest.skip("Docker not available")

    # Reuse existing container if available
    if "postgres" in _container_cache:
        yield _container_cache["postgres"]
        return

    container = PostgresContainer(
        image="pgvector/pgvector:pg16",
        username="fraiseql",
        password="fraiseql",
        dbname="fraiseql_test",
        driver="psycopg",
    )
    container.start()

    # ADDED: Wait for database to be ready
    _wait_for_database_ready(container.get_connection_url())

    _container_cache["postgres"] = container
    yield container

    container.stop()
    _container_cache.pop("postgres", None)


def _wait_for_database_ready(url: str, max_attempts: int = 30) -> None:
    """Wait for PostgreSQL to accept connections.

    Args:
        url: PostgreSQL connection URL
        max_attempts: Maximum retry attempts (default: 30 = 15 seconds)

    Raises:
        RuntimeError: If database doesn't become ready in time
    """
    import psycopg
    import time

    for attempt in range(max_attempts):
        try:
            # Try synchronous connection (simpler for startup check)
            with psycopg.connect(url, connect_timeout=2) as conn:
                # Verify database is actually ready
                with conn.cursor() as cur:
                    cur.execute("SELECT 1")
                return  # Success!
        except (psycopg.OperationalError, OSError):
            if attempt < max_attempts - 1:
                time.sleep(0.5)
            else:
                raise RuntimeError(
                    f"PostgreSQL container did not become ready after "
                    f"{max_attempts * 0.5:.1f} seconds"
                )
```

### Phase 2: Add Retry Logic to pgvector_available (15 min)
Belt-and-suspenders approach for maximum reliability.

```python
@pytest_asyncio.fixture(scope="session")
async def pgvector_available(postgres_url: str, postgres_container) -> bool:
    """Check if pgvector extension is available.

    Returns True if pgvector extension can be used, False otherwise.
    This allows tests to skip gracefully when pgvector is not available.

    Note: Depends on postgres_container to ensure container is ready before checking.
    Includes retry logic to handle any remaining timing issues.
    """
    import asyncio

    # Try up to 5 times with 1-second delays
    # (Should succeed immediately if Phase 1 health check works)
    for attempt in range(5):
        try:
            async with await psycopg.AsyncConnection.connect(
                postgres_url,
                connect_timeout=3
            ) as conn:
                await conn.execute("CREATE EXTENSION IF NOT EXISTS vector")
                await conn.commit()
                return True
        except (psycopg.OperationalError, OSError):
            if attempt < 4:
                await asyncio.sleep(1)
                continue
            # Database is up but doesn't have pgvector - skip tests
            return False
        except psycopg.errors.InsufficientPrivilege:
            # Check if already installed
            try:
                async with await psycopg.AsyncConnection.connect(postgres_url) as check_conn:
                    result = await check_conn.execute("""
                        SELECT EXISTS(
                            SELECT 1 FROM pg_extension WHERE extname = 'vector'
                        )
                    """)
                    row = await result.fetchone()
                    return row[0] if row else False
            except Exception:
                return False
        except Exception:
            # Other errors (permissions, etc) - skip tests
            return False

    return False
```

---

## Implementation Steps

### Step 1: Add Helper Function (5 min)
Add `_wait_for_database_ready()` helper function at bottom of `database_conftest.py`.

**File**: `tests/fixtures/database/database_conftest.py`

**Location**: After all fixtures, before closing comment

**Code**: See Phase 1 above

---

### Step 2: Update postgres_container Fixture (10 min)
Add health check call to `postgres_container` fixture.

**File**: `tests/fixtures/database/database_conftest.py:66`

**Changes**:
```python
container.start()

# ADD THIS LINE:
_wait_for_database_ready(container.get_connection_url())

_container_cache["postgres"] = container
```

---

### Step 3: Update pgvector_available Fixture (15 min)
Add retry logic to handle edge cases.

**File**: `tests/fixtures/database/database_conftest.py:152`

**Changes**: Replace entire function with Phase 2 implementation above

---

### Step 4: Test Individual Execution (5 min)
Verify pgvector tests still pass individually.

```bash
pytest tests/integration/test_langchain_vectorstore_integration.py -v
# Expected: 13 passed
```

---

### Step 5: Test Full Suite Execution (2 min + 37 sec runtime)
Verify pgvector tests now run in full suite.

```bash
pytest tests/ -v 2>&1 | grep -E "test_langchain.*PASSED|test_llamaindex.*PASSED|test_vector_e2e.*PASSED" | wc -l
# Expected: ~28 passed (some will fail due to incomplete implementation)
```

---

### Step 6: Verify No Regressions (2 min + 37 sec runtime)
Run full suite and check overall stats.

```bash
pytest tests/ --tb=no -q
# Expected: 5318 passed, 7 skipped (down from 44)
```

---

### Step 7: Clean Up Debug Output (2 min)
Remove any temporary print statements added during debugging.

---

## Verification Commands

### Before Fix
```bash
# Run full suite - currently shows 44 skipped
pytest tests/ --tb=no -q 2>&1 | tail -3

# Run pgvector tests directly - currently pass
pytest tests/integration/test_langchain_vectorstore_integration.py --tb=no -q
```

### After Fix
```bash
# Full suite - should show 7 skipped (not 44)
pytest tests/ --tb=no -q 2>&1 | tail -3

# Verify pgvector tests run (not skip)
pytest tests/ -v 2>&1 | grep "test_langchain" | grep -c SKIPPED
# Expected: 0 (was 13)

# Check test count
pytest tests/ -v 2>&1 | grep "test_langchain" | grep -E "PASSED|FAILED" | wc -l
# Expected: 13 (running, not skipped)
```

---

## Acceptance Criteria

✅ **Primary Goal**: pgvector tests run in full test suite (not skipped)
✅ **Test Count**: Skip count drops from 44 to 7 (removes 37 pgvector skips)
✅ **Reliability**: Container health check ensures database is always ready
✅ **Performance**: Minimal overhead (<2 seconds added to suite startup)
✅ **No Regressions**: All previously passing tests still pass
✅ **Documentation**: Code comments explain the health check logic

---

## Expected Outcomes

### Test Results After Fix

| Category | Before | After | Notes |
|----------|--------|-------|-------|
| **Total Tests** | 5,362 | 5,362 | No change |
| **Passing** | 5,318 | 5,346 | +28 (pgvector tests now run) |
| **Skipped** | 44 | 7 | -37 (pgvector unskipped) |
| **Failing** | 0 | 9 | Expected (incomplete vector feature) |

### Remaining Skips (7 total)
1. Vault KMS integration (3) - requires manual env setup ✓
2. AWS KMS integration (3) - requires AWS credentials ✓
3. Error log partitioning (1) - known database bug ✓

**All 7 remaining skips are intentional and correct.**

---

## Risks & Mitigation

### Risk 1: Health Check Timeout Too Short
**Mitigation**: Use 15-second timeout (30 × 0.5s), which is generous for container startup

### Risk 2: Health Check Adds Too Much Delay
**Mitigation**: Health check only runs once per session, amortized across all tests

### Risk 3: Async/Sync Mixing Issues
**Mitigation**: Use synchronous psycopg connection for health check (simpler, more reliable)

### Risk 4: Container Already Running
**Mitigation**: Check container cache first, skip health check if reusing

---

## Rollback Plan

If issues arise:

```bash
# Revert the commits
git revert HEAD~2..HEAD

# Or restore specific file
git checkout HEAD~2 -- tests/fixtures/database/database_conftest.py

# Run tests to verify
pytest tests/ --tb=no -q
```

---

## DO NOT

❌ **Don't** change test execution order with pytest marks
❌ **Don't** remove the `postgres_container` dependency from `pgvector_available`
❌ **Don't** change fixture scopes without considering impact
❌ **Don't** add print statements to production code (debug only)
❌ **Don't** skip implementing the health check (it benefits all tests)
❌ **Don't** assume container "started" = "ready to accept connections"

---

## Success Metrics

1. ✅ pgvector tests execute in full suite (not skipped)
2. ✅ All 28 passing pgvector tests continue to pass
3. ✅ 9 expected failures in incomplete vector features
4. ✅ No new test failures introduced
5. ✅ Test suite runtime increase < 2 seconds
6. ✅ Health check logs show database ready confirmation

---

## Time Estimate

- **Total Time**: 71 minutes
  - Planning/Analysis: 0 min (already done)
  - Implementation: 40 min
  - Testing: 20 min
  - Documentation: 5 min
  - Commit/Review: 6 min

---

## Implementation Order

1. ✅ Write this plan
2. ⏳ Implement `_wait_for_database_ready()` helper
3. ⏳ Update `postgres_container` fixture
4. ⏳ Update `pgvector_available` fixture
5. ⏳ Test individual file execution
6. ⏳ Test full suite execution
7. ⏳ Verify and commit changes

---

## Notes

- The container uses `pgvector/pgvector:pg16` image which includes pgvector
- Some vector tests will still FAIL after this fix (expected - incomplete feature)
- The fix makes tests RUN (not skip), but doesn't implement the vector feature
- Focus is on infrastructure reliability, not feature completeness
