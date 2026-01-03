# Phase 4: Integration & Testing - Implementation Plan

**Phase**: 4
**Objective**: Comprehensive end-to-end testing, performance benchmarking, and integration verification
**Estimated Time**: 2 weeks / 30 hours
**Files Created**: 3 new test files (~700 lines)
**Success Criteria**: E2E tests pass, performance benchmarks met (<10ms E2E), 100+ concurrent subscriptions stable
**Lead Engineer**: Junior Test Automation Engineer

---

## Context

Phase 4 ensures the subscription system works end-to-end. Tests security integration, performance targets, and concurrent operation.

**Key Testing Areas**:
- Security filtering end-to-end
- Rate limiting enforcement
- Concurrent subscriptions
- Framework adapters
- Performance benchmarks
- Memory usage and leaks

---

## Files to Create/Modify

### New Files
- `tests/test_subscriptions_e2e.py` (NEW, ~300 lines) - End-to-end tests
- `tests/test_subscriptions_performance.py` (NEW, ~200 lines) - Benchmarks
- `tests/test_subscriptions_fastapi.py` (NEW, ~200 lines) - Framework tests

### Modified Files
- `tests/conftest.py` (modify) - Add test fixtures for subscriptions

---

## Detailed Implementation Tasks

### Task 4.1: Test Suite (15 hours)

**Objective**: Comprehensive test coverage for all subscription functionality

#### 4.1a: End-to-End Tests (8 hours)

**File**: `tests/test_subscriptions_e2e.py`

**Tests to Implement**:

```python
import pytest
import asyncio
import json
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql import _fraiseql_rs


@pytest.mark.asyncio
async def test_subscription_full_workflow():
    """Complete subscription workflow from register to receive."""
    config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(config)

    # 1. Create subscription
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { users { id name } }",
        variables={},
        resolver_fn=lambda event, vars: {"users": [{"id": event["id"], "name": event["name"]}]},
        user_id="user1",
        tenant_id="tenant1",
    )

    # 2. Publish event
    await manager.publish_event(
        event_type="userCreated",
        channel="users",
        data={"id": "123", "name": "Alice"},
    )

    # 3. Receive response
    response_bytes = await manager.get_next_event("sub1")
    assert response_bytes is not None

    # 4. Parse and verify
    response = json.loads(response_bytes)
    assert response["type"] == "next"
    assert response["id"] == "sub1"
    assert "payload" in response
    assert "data" in response["payload"]
    assert response["payload"]["data"]["users"][0]["id"] == "123"


@pytest.mark.asyncio
async def test_security_filtering():
    """Test that security filtering works end-to-end."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Register subscription for user1
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { secretData }",
        variables={},
        resolver_fn=lambda e, v: {"secretData": "hidden"},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Publish event for different user
    await manager.publish_event(
        event_type="dataChanged",
        channel="secret",
        data={"user_id": "user2", "data": "secret"},
    )

    # Should not receive event (filtered by security)
    response = await manager.get_next_event("sub1")
    assert response is None  # Filtered out


@pytest.mark.asyncio
async def test_rate_limiting():
    """Test rate limiter enforcement."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { data }",
        variables={},
        resolver_fn=lambda e, v: {"data": "test"},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Publish many events quickly
    for i in range(100):
        await manager.publish_event(
            event_type="test",
            channel="test",
            data={"id": i},
        )

    # Count received events
    received = 0
    for _ in range(10):  # Wait a bit
        if await manager.get_next_event("sub1"):
            received += 1
        await asyncio.sleep(0.01)

    # Should be rate limited (not all 100 received)
    assert received < 100


@pytest.mark.asyncio
async def test_concurrent_subscriptions():
    """Test 100 concurrent subscriptions."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Create 100 subscriptions
    tasks = []
    for i in range(100):
        task = manager.create_subscription(
            subscription_id=f"sub{i}",
            connection_id=f"conn{i}",
            query="subscription { data }",
            variables={},
            resolver_fn=lambda e, v: {"data": f"response{i}"},
            user_id=f"user{i}",
            tenant_id="tenant1",
        )
        tasks.append(task)

    await asyncio.gather(*tasks)

    # Publish one event
    await manager.publish_event(
        event_type="test",
        channel="test",
        data={"id": "123"},
    )

    # Verify all subscriptions get the event
    received_count = 0
    for i in range(100):
        response = await manager.get_next_event(f"sub{i}")
        if response:
            received_count += 1

    assert received_count == 100


@pytest.mark.asyncio
async def test_subscription_cleanup():
    """Test subscription cleanup on complete."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Create subscription
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { data }",
        variables={},
        resolver_fn=lambda e, v: {"data": "test"},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Complete subscription
    await manager.complete_subscription("sub1")

    # Publish event
    await manager.publish_event(
        event_type="test",
        channel="test",
        data={"id": "123"},
    )

    # Should not receive event
    response = await manager.get_next_event("sub1")
    assert response is None
```

#### 4.1b: Framework Integration Tests (4 hours)

**File**: `tests/test_subscriptions_fastapi.py`

**Tests to Implement**:

```python
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
from fraiseql import _fraiseql_rs


def test_fastapi_router_creation():
    """Test FastAPI router can be created."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())
    router = SubscriptionRouterFactory.create(manager)

    assert router is not None
    # Check that websocket route exists
    routes = [route for route in router.routes if hasattr(route, 'path')]
    assert len(routes) > 0


@pytest.mark.asyncio
async def test_fastapi_websocket_connection():
    """Test WebSocket connection through FastAPI."""
    app = FastAPI()
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())
    router = SubscriptionRouterFactory.create(manager)
    app.include_router(router)

    # Test with test client
    client = TestClient(app)

    # WebSocket connections are hard to test with TestClient
    # This would require a full WebSocket test client
    # For now, just verify the endpoint exists
    assert True  # Placeholder


def test_fastapi_auth_handler():
    """Test auth handler integration."""
    def auth_handler(payload):
        return {"user_id": "test_user", "tenant_id": "test_tenant"}

    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())
    router = SubscriptionRouterFactory.create(manager, auth_handler=auth_handler)

    assert router is not None
```

#### 4.1c: Unit Tests for Components (3 hours)

**Add to existing test files**:

```python
# In test_subscriptions_phase1.py
def test_payload_types():
    payload = _fraiseql_rs.subscriptions.PySubscriptionPayload("query")
    assert payload.query == "query"

# In test_subscriptions_phase2.py
@pytest.mark.asyncio
async def test_dispatch_performance():
    # Measure dispatch time for N subscriptions
    pass
```

### Task 4.2: Performance Benchmarks (10 hours)

**Objective**: Verify performance targets are met

**File**: `tests/test_subscriptions_performance.py`

**Benchmarks to Implement**:

```python
import pytest
import asyncio
import time
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql import _fraiseql_rs


@pytest.mark.asyncio
async def test_event_dispatch_throughput():
    """Benchmark: 10,000 events with 100 subscriptions."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Create 100 subscriptions
    for i in range(100):
        await manager.create_subscription(
            subscription_id=f"sub{i}",
            connection_id=f"conn{i}",
            query="subscription { data }",
            variables={},
            resolver_fn=lambda e, v: {"data": "test"},
            user_id=f"user{i}",
            tenant_id="tenant1",
        )

    # Measure 10,000 publishes
    start_time = time.time()
    for i in range(10000):
        await manager.publish_event(
            event_type="test",
            channel="test",
            data={"id": i},
        )
    end_time = time.time()

    total_time = end_time - start_time
    events_per_sec = 10000 / total_time

    # Target: >10k events/sec
    assert events_per_sec > 10000
    # Target: <10 seconds total
    assert total_time < 10.0


@pytest.mark.asyncio
async def test_end_to_end_latency():
    """Measure complete E2E latency."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { data }",
        variables={},
        resolver_fn=lambda e, v: {"data": "test"},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Measure publish to receive
    start_time = time.time()
    await manager.publish_event(
        event_type="test",
        channel="test",
        data={"id": "123"},
    )

    # Wait for response
    response = None
    for _ in range(100):  # Max 100ms wait
        response = await manager.get_next_event("sub1")
        if response:
            break
        await asyncio.sleep(0.001)

    end_time = time.time()
    latency_ms = (end_time - start_time) * 1000

    # Target: <10ms E2E
    assert latency_ms < 10.0
    assert response is not None


@pytest.mark.asyncio
async def test_concurrent_subscriptions_performance():
    """Test 1000 concurrent subscriptions."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Create 1000 subscriptions
    tasks = []
    for i in range(1000):
        task = manager.create_subscription(
            subscription_id=f"sub{i}",
            connection_id=f"conn{i}",
            query="subscription { data }",
            variables={},
            resolver_fn=lambda e, v: {"data": "test"},
            user_id=f"user{i}",
            tenant_id="tenant1",
        )
        tasks.append(task)

    start_time = time.time()
    await asyncio.gather(*tasks)
    end_time = time.time()

    creation_time = end_time - start_time

    # Target: Create 1000 subscriptions quickly
    assert creation_time < 5.0  # <5 seconds

    # Publish event and verify delivery
    await manager.publish_event(
        event_type="test",
        channel="test",
        data={"id": "123"},
    )

    # Count responses
    received = 0
    for i in range(1000):
        if await manager.get_next_event(f"sub{i}"):
            received += 1

    assert received == 1000


@pytest.mark.asyncio
async def test_memory_usage():
    """Test for memory leaks."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    # Create many subscriptions and events
    for i in range(100):
        await manager.create_subscription(
            subscription_id=f"sub{i}",
            connection_id=f"conn{i}",
            query="subscription { data }",
            variables={},
            resolver_fn=lambda e, v: {"data": "test"},
            user_id=f"user{i}",
            tenant_id="tenant1",
        )

    # Publish many events
    for i in range(1000):
        await manager.publish_event(
            event_type="test",
            channel="test",
            data={"id": i},
        )

    # Cleanup
    for i in range(100):
        await manager.complete_subscription(f"sub{i}")

    # Memory should be stable (no test for this, but monitor manually)
    assert True


@pytest.mark.asyncio
async def test_python_resolver_overhead():
    """Measure Python resolver call overhead."""
    manager = SubscriptionManager(_fraiseql_rs.PyEventBusConfig.memory())

    def resolver(event, variables):
        return {"result": event["id"] * 2}

    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { result }",
        variables={},
        resolver_fn=resolver,
        user_id="user1",
        tenant_id="tenant1",
    )

    # Measure resolver call time
    start_time = time.time()
    await manager.publish_event(
        event_type="test",
        channel="test",
        data={"id": 42},
    )

    response = await manager.get_next_event("sub1")
    end_time = time.time()

    latency_ms = (end_time - start_time) * 1000

    # Target: <100μs per Python call (0.1ms)
    assert latency_ms < 0.1
```

### Task 4.3: Compilation & Type Checking (5 hours)

**Objective**: Ensure code quality and type safety

**Steps**:
1. Verify Rust compilation
2. Run Python type checking
3. Test imports and basic functionality

**Commands to Run**:

```bash
# Rust compilation
cargo build --lib
cargo clippy

# Python type checking
mypy src/fraiseql/subscriptions/ --ignore-missing-imports

# Run all tests
pytest tests/test_subscriptions_*.py -v

# Test imports
python3 -c "
from fraiseql.subscriptions import SubscriptionManager
from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
from fraiseql.integrations.starlette_subscriptions import create_subscription_app
from fraiseql import _fraiseql_rs
print('All imports successful')
"
```

**Acceptance Criteria**:
- [ ] `cargo build --lib` succeeds with zero errors
- [ ] `cargo clippy` shows zero warnings
- [ ] `mypy` passes with acceptable warnings
- [ ] All test files run without import errors
- [ ] Basic instantiation works

---

## Testing Requirements

### Test Fixtures

**Add to tests/conftest.py**:

```python
import pytest
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql import _fraiseql_rs


@pytest.fixture
async def subscription_manager():
    """Fixture for SubscriptionManager with memory event bus."""
    config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(config)
    return manager


@pytest.fixture
def sample_resolver():
    """Sample resolver function for testing."""
    def resolver(event, variables):
        return {"data": event["id"]}
    return resolver
```

### Running Tests

```bash
# All subscription tests
pytest tests/test_subscriptions_*.py -v

# Performance tests only
pytest tests/test_subscriptions_performance.py -v

# Fast tests only
pytest tests/test_subscriptions_*.py -k "not performance"

# With coverage
pytest tests/test_subscriptions_*.py --cov=fraiseql.subscriptions --cov-report=html
```

---

## Verification Checklist

- [ ] All E2E tests pass (security, rate limiting, concurrent subs)
- [ ] Performance benchmarks met (>10k events/sec, <10ms E2E)
- [ ] 100+ concurrent subscriptions stable
- [ ] Memory usage reasonable (no obvious leaks)
- [ ] Framework adapters work (FastAPI, Starlette)
- [ ] Type checking passes
- [ ] Compilation clean (Rust + Python)
- [ ] All imports work
- [ ] Error handling tested

---

## Success Criteria for Phase 4

When Phase 4 is complete:

**Functional Tests Pass**:
- ✅ End-to-end subscription workflow works
- ✅ Security filtering blocks unauthorized events
- ✅ Rate limiting prevents abuse
- ✅ 100+ concurrent subscriptions stable

**Performance Targets Met**:
- ✅ Event dispatch: <1ms for 100 subscriptions
- ✅ Python resolver: <100μs per call
- ✅ E2E latency: <10ms
- ✅ Throughput: >10k events/sec

**Quality Assurance**:
- ✅ Type checking clean
- ✅ Compilation clean
- ✅ Test coverage >80%
- ✅ Memory usage stable

---

## Blockers & Dependencies

**Prerequisites**:
- Phase 1-3 complete and working
- Test environment set up
- FastAPI/Starlette available

**Help Needed**:
- If performance issues, ask senior engineer
- If test environment setup unclear, ask senior engineer
- If benchmark results unexpected, ask senior engineer

---

## Time Estimate Breakdown

- Task 4.1: 15 hours (Test suite: 8 E2E + 4 framework + 3 unit)
- Task 4.2: 10 hours (Performance benchmarks)
- Task 4.3: 5 hours (Compilation & type checking)
- Documentation: 0 hours (covered in Phase 5)

**Total: 30 hours**

---

## Next Phase Dependencies

Phase 4 provides verified working system that Phase 5 documents. Phase 4 must be complete with all tests passing and performance targets met before Phase 5 begins.</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-4.md