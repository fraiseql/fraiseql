# Federation Testing Guide

## Overview

This guide covers testing patterns for Federation-enabled GraphQL services, focusing on:
- DataLoader entity resolution
- Batch executor integration
- Error handling and edge cases
- Performance validation
- Migration testing strategies

---

## Test Infrastructure

### Available Test Files

**Core federation tests** (in `tests/federation/`):
- `test_dataloader.py` - Entity resolution patterns (200+ lines)
- `test_batch_executor.py` - Batch execution contexts (300+ lines)
- `test_entities.py` - Entity registry and decorators
- `test_apollo_router_integration.py` - Router integration tests

### Key Testing Utilities

**Mock Infrastructure:**
```python
# Mock database pool for testing
class MockAsyncPool:
    """Simulates async database connection pool."""
    def __init__(self, data: Dict[Tuple[str, str], Dict]):
        self.data = data  # {(typename, key): entity_dict}
        self.queries_executed = 0
```

**Mock Resolver:**
```python
class MockResolver:
    """Simulates EntitiesResolver for dependency injection."""
    def resolve(self, typename: str, key_values: List[str]) -> List[Dict]:
        pass
```

---

## Testing Patterns

### 1. DataLoader Unit Tests

**Purpose:** Verify entity resolution, batching, and caching behavior.

**Test Structure:**
```python
@pytest.fixture
def mock_pool():
    """Set up test data."""
    data = {
        ("User", "user-1"): {"name": "Alice", "email": "alice@example.com"},
        ("User", "user-2"): {"name": "Bob", "email": "bob@example.com"},
        ("Post", "post-1"): {"title": "Hello World"},
    }
    return MockAsyncPool(data)

@pytest.fixture
def loader(mock_pool):
    """Create DataLoader with mock pool."""
    return EntityDataLoader(mock_pool)

@pytest.mark.asyncio
async def test_load_single_entity(loader):
    """Test loading a single entity by key."""
    result = await loader.load("User", "user-1")

    assert result is not None
    assert result["name"] == "Alice"
    assert loader.stats.total_queries == 1
```

**Key Assertions:**
- ✅ Entity loaded correctly
- ✅ Query count (batching effectiveness)
- ✅ Cache hit/miss ratios
- ✅ Deduplication of duplicate keys

**Example Tests:**
- `test_load_single_entity` - Basic entity loading
- `test_load_multiple_entities` - Batching behavior
- `test_deduplication` - Duplicate key handling
- `test_cache_hits` - Cache effectiveness

---

### 2. Batch Executor Integration Tests

**Purpose:** Verify batch execution contexts and entity resolution flow.

**Test Structure:**
```python
@pytest.mark.asyncio
async def test_batch_execute_single_type(mock_pool):
    """Test batch execution for single entity type."""
    executor = BatchExecutor(mock_pool)

    requests = [
        EntityRequest(typename="User", key="user-1"),
        EntityRequest(typename="User", key="user-2"),
    ]

    results = await executor.execute(requests)

    assert len(results) == 2
    assert results[0]["name"] == "Alice"
    assert results[1]["name"] == "Bob"
```

**Context Manager Pattern:**
```python
@pytest.mark.asyncio
async def test_batch_context_manager(mock_pool):
    """Test batch execution using context manager."""
    async with BatchExecutor(mock_pool) as executor:
        user1 = await executor.load("User", "user-1")
        user2 = await executor.load("User", "user-2")

    # Batch is automatically flushed on context exit
    assert user1["name"] == "Alice"
    assert user2["name"] == "Bob"
```

**Key Test Cases:**
- ✅ Single type batch execution
- ✅ Multiple type batch execution (grouped)
- ✅ Context manager pattern
- ✅ Manual flush vs automatic
- ✅ Concurrent batch execution

---

### 3. Error Handling Tests

**Purpose:** Verify graceful handling of edge cases and errors.

**Missing Entity Handling:**
```python
@pytest.mark.asyncio
async def test_batch_with_missing_entities(mock_pool):
    """Test handling of missing entities."""
    executor = BatchExecutor(mock_pool)

    requests = [
        EntityRequest(typename="User", key="user-1"),  # exists
        EntityRequest(typename="User", key="user-999"),  # missing
    ]

    results = await executor.execute(requests)

    # Missing entities return None
    assert results[0] is not None
    assert results[1] is None  # or GraphQL error
```

**Empty Request Handling:**
```python
@pytest.mark.asyncio
async def test_empty_request_list(executor):
    """Test handling of empty request batch."""
    results = await executor.execute([])

    assert results == []
    assert executor.stats.total_queries == 0
```

**Error Propagation:**
```python
@pytest.mark.asyncio
async def test_concurrent_error_handling(mock_pool_with_failures):
    """Test error handling in concurrent batch execution."""
    executor = ConcurrentBatchExecutor(mock_pool_with_failures)

    # Partial failures should not crash entire batch
    with pytest.raises(PartialBatchError):
        await executor.execute(requests)
```

---

### 4. Performance Benchmarks

**Purpose:** Validate batching provides expected performance improvements.

**Benchmark Structure:**
```python
@pytest.mark.benchmark
@pytest.mark.asyncio
async def test_dataloader_performance_vs_naive(
    benchmark, mock_pool_large
):
    """Compare DataLoader performance vs naive sequential loading."""

    async def naive_loading():
        results = []
        for key in ["user-1", "user-2", ..., "user-1000"]:
            result = await mock_pool_large.fetch(
                "SELECT * FROM tv_user WHERE id = $1", key
            )
            results.append(result)
        return results

    async def batched_loading():
        loader = EntityDataLoader(mock_pool_large)
        keys = ["user-1", "user-2", ..., "user-1000"]
        return await asyncio.gather(*[
            loader.load("User", key) for key in keys
        ])

    # DataLoader should be ~7-10x faster
    naive_time = benchmark(asyncio.run, naive_loading())
    batch_time = benchmark(asyncio.run, batched_loading())

    assert naive_time > batch_time * 5
```

**Key Metrics:**
- Query count (should be batched into ~10 queries)
- Execution time (should be < 100ms for 1000 entities)
- Memory usage (deduplication reduces memory)

---

## Testing Strategies by Scenario

### Migration Testing

**Testing that existing resolvers work with federation:**

```python
@pytest.mark.asyncio
async def test_resolver_migration_compatibility(schema, db_pool):
    """Test that legacy resolver works with federation context."""

    # Simulate legacy resolver
    @fraiseql.field
    async def author(self, info):
        # Old pattern: direct query
        user = await info.context["db"].fetch(
            "SELECT * FROM users WHERE id = $1",
            self.author_id
        )
        return user

    # With federation context, DataLoader should intercept
    executor = BatchExecutor(db_pool)
    async with executor:
        results = await execute_query(schema, query, executor=executor)

    # Verify batching happened
    assert executor.stats.total_queries < 5  # Batched
```

### Multi-Service Testing

**Testing federation across service boundaries:**

```python
@pytest.mark.asyncio
async def test_cross_service_entity_reference(
    user_service_schema, post_service_schema, router
):
    """Test that entities can reference across services."""

    query = """
    query {
        posts {
            id
            title
            author {  # Reference to User service
                id
                name
            }
        }
    }
    """

    result = await router.execute(query)

    # Router should resolve User entities using federation
    assert result.data["posts"][0]["author"]["name"] == "Alice"
```

### Canary Testing

**Testing gradual rollout of federation:**

```python
@pytest.mark.asyncio
async def test_federation_canary_routing(
    router_with_canary_config, user_service_pool
):
    """Test canary routing for federation rollout."""

    # 10% traffic to federation, 90% to legacy
    config = CanaryConfig(federation_percentage=10)

    results = []
    for i in range(100):
        result = await router_with_canary_config.execute(
            query, config=config
        )
        results.append(result)

    # ~10% should use federation path
    federation_count = sum(
        1 for r in results if r.federation_path
    )
    assert 5 < federation_count < 15  # 5-15 out of 100
```

---

## Test Organization

### Directory Structure

```
tests/federation/
├── conftest.py                    # Shared fixtures
├── test_dataloader.py             # DataLoader unit tests
├── test_batch_executor.py         # Batch executor tests
├── test_entities.py               # Entity registry tests
├── test_error_handling.py         # Error case tests
├── test_apollo_router_integration.py  # Router integration
├── test_dataloader_performance.py # Performance tests
└── fixtures/
    ├── mock_pools.py              # Reusable mock pools
    └── test_data.py               # Test data sets
```

### Running Tests

**All federation tests:**
```bash
pytest tests/federation/ -v
```

**Specific test file:**
```bash
pytest tests/federation/test_dataloader.py -v
```

**With coverage:**
```bash
pytest tests/federation/ --cov=src/fraiseql/federation
```

**Performance benchmarks:**
```bash
pytest tests/federation/test_dataloader_performance.py -v --benchmark-only
```

---

## Debugging Failed Tests

### Common Issues

**1. Mock pool data not matching query**
```python
# Problem: Query looks for 'tv_user' but pool uses 'User'
# Solution: Ensure mock query parsing matches actual query format

def fetch(self, sql, *params):
    if "tv_user" in sql:
        typename = "User"
```

**2. Async context not awaited**
```python
# Problem: Not using @pytest.mark.asyncio decorator
# Solution: Mark test as async
@pytest.mark.asyncio
async def test_something(loader):
    result = await loader.load("User", "user-1")
```

**3. Entity registry pollution between tests**
```python
# Problem: Entity definitions from previous test interfere
# Solution: Use clear_entities fixture
@pytest.fixture
def clear_entities():
    clear_entity_registry()
    yield
    clear_entity_registry()

def test_something(clear_entities):
    # Registry is clean
```

---

## Best Practices

1. **Isolate Test Data**
   - Use unique keys per test
   - Clear registry between tests
   - Avoid shared state

2. **Test Behavior, Not Implementation**
   - Assert results, not query count
   - Use performance benchmarks for speed checks
   - Focus on user-facing outcomes

3. **Use Fixtures for Reusability**
   - Common mock pools in conftest.py
   - Parameterized fixtures for variants
   - Gradual fixture composition

4. **Document Complex Tests**
   - Add docstrings explaining the test purpose
   - Comment on non-obvious assertions
   - Show expected vs actual patterns

5. **Performance Testing**
   - Establish baselines (7-10x improvement expected)
   - Test with realistic data volumes
   - Monitor for regressions

---

## Running Tests in CI/CD

### Pre-commit Validation

```bash
# Run before committing
make test-federation

# Or directly
pytest tests/federation/ -q
```

### Pull Request Validation

```yaml
# GitHub Actions example
- name: Run federation tests
  run: pytest tests/federation/ -v --cov

- name: Check coverage
  run: pytest tests/federation/ --cov --cov-fail-under=80
```

### Performance Gate

```bash
# Ensure no performance regressions
pytest tests/federation/ -v --benchmark-compare
```

---

## Summary

**Key Testing Patterns:**
- ✅ DataLoader unit tests validate batching/caching
- ✅ Batch executor tests verify integration
- ✅ Error handling tests ensure reliability
- ✅ Performance tests validate 7-10x improvement

**Next Steps:**
See [08-migration-guide.md](08-migration-guide.md) for patterns on migrating existing services to federation.
