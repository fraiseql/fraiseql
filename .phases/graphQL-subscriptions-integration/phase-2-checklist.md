# Phase 1 Implementation Checklist

**Phase**: 1 - PyO3 Core Bindings
**Engineer**: Junior Rust/Python FFI Developer
**Timeline**: 2 weeks / 30 hours

---

## Pre-Implementation Checklist

- [ ] Read `phase-1.md` implementation plan
- [ ] Review existing PyO3 patterns in `auth/py_bindings.rs` and `apq/py_bindings.rs`
- [ ] Understand global tokio runtime pattern from existing code
- [ ] Check existing SubscriptionExecutor structure
- [ ] Verify development environment (cargo, python, pyo3)

---

## Task 1.1: Subscription Payload Types

### Requirements
- [ ] Create `fraiseql_rs/src/subscriptions/py_bindings.rs`
- [ ] Implement `PySubscriptionPayload` class
- [ ] Implement `PyGraphQLMessage` class
- [ ] Add proper PyO3 decorators and methods

### Code Checklist
- [ ] `PySubscriptionPayload` has all required fields (query, operation_name, variables, extensions)
- [ ] `PySubscriptionPayload::new()` constructor implemented
- [ ] `PyGraphQLMessage` has type_, id, payload fields
- [ ] `PyGraphQLMessage::from_dict()` parses dict correctly
- [ ] `PyGraphQLMessage::to_dict()` converts back to dict
- [ ] All fields properly exposed with `#[pyo3(get, set)]` or `#[pyo3(get)]`

### Testing Checklist
- [ ] Can instantiate `PySubscriptionPayload("query { test }")`
- [ ] Can instantiate `PyGraphQLMessage()` and set fields
- [ ] `from_dict()` works with valid GraphQL message format
- [ ] `to_dict()` returns correct Python dict
- [ ] All field access works (get/set)

### Compilation Checklist
- [ ] `cargo build --lib` succeeds
- [ ] No clippy warnings
- [ ] Python import works: `from fraiseql import _fraiseql_rs`

---

## Task 1.2: Core Subscription Executor

### Requirements
- [ ] Implement `PySubscriptionExecutor` class
- [ ] Add all required methods (register, publish, next_event, complete, metrics)
- [ ] Use global tokio runtime correctly
- [ ] Handle PyDict ↔ Rust conversions

### Code Checklist
- [ ] `PySubscriptionExecutor` stores `Arc<SubscriptionExecutor>` and runtime
- [ ] `new()` gets global runtime with existing pattern
- [ ] `register_subscription()` converts PyDict variables to Rust types
- [ ] `publish_event()` uses `runtime.block_on()` for async work
- [ ] `next_event()` returns `Option<Vec<u8>>` (pre-serialized bytes)
- [ ] `complete_subscription()` calls cleanup
- [ ] `get_metrics()` converts metrics to PyDict

### Helper Functions Checklist
- [ ] `python_dict_to_json_map()` converts PyDict to HashMap<String, Value>
- [ ] `python_dict_to_event()` creates Arc<Event>
- [ ] `json_to_python_dict()` converts back to PyDict
- [ ] `python_metrics_dict()` converts SecurityMetrics

### Testing Checklist
- [ ] Can instantiate `PySubscriptionExecutor()`
- [ ] `register_subscription()` accepts all parameters
- [ ] `publish_event()` doesn't block Python GIL
- [ ] `next_event()` returns bytes or None
- [ ] `get_metrics()` returns dict with expected structure
- [ ] All methods callable from Python without errors

### Performance Checklist
- [ ] Methods respond quickly (<1ms for sync operations)
- [ ] No blocking calls outside runtime
- [ ] Memory usage reasonable

---

## Task 1.3: Event Bus Bridge

### Requirements
- [ ] Implement `PyEventBusConfig` class
- [ ] Add static methods for memory, redis, postgresql
- [ ] Include validation for URLs and connection strings

### Code Checklist
- [ ] `PyEventBusConfig` stores `EventBusConfig` enum
- [ ] `memory()` creates InMemory config
- [ ] `redis()` validates URL format and creates Redis config
- [ ] `postgresql()` validates connection string and creates PostgreSQL config
- [ ] Error handling for invalid inputs

### Testing Checklist
- [ ] `PyEventBusConfig.memory()` works
- [ ] `PyEventBusConfig.redis()` validates URLs
- [ ] `PyEventBusConfig.postgresql()` validates connection strings
- [ ] Invalid inputs raise appropriate PyErr

---

## Task 1.4: Module Registration

### Requirements
- [ ] Update `fraiseql_rs/src/lib.rs`
- [ ] Create `init_subscriptions()` function
- [ ] Register all classes with Python module

### Code Checklist
- [ ] Added subscriptions module declaration in `lib.rs`
- [ ] `init_subscriptions()` function implemented in `py_bindings.rs`
- [ ] All 4 classes registered: PySubscriptionPayload, PyGraphQLMessage, PySubscriptionExecutor, PyEventBusConfig
- [ ] Module registration in main `fraizeql_rs()` function

### Testing Checklist
- [ ] `cargo build --lib` succeeds with module changes
- [ ] Can import all classes from Python
- [ ] All classes accessible: `_fraiseql_rs.subscriptions.PySubscriptionExecutor`
- [ ] Can instantiate all classes without errors

---

## Overall Phase 1 Verification

### Compilation & Import
- [ ] `cargo build --lib` succeeds with zero errors
- [ ] `cargo clippy` shows zero warnings
- [ ] Python imports work: `from fraiseql import _fraiseql_rs`
- [ ] All subscription classes accessible

### Unit Tests
- [ ] All individual method tests pass
- [ ] Integration test (register → publish → get_event) works
- [ ] Error handling tested
- [ ] Edge cases covered

### End-to-End Test
Run this Python code successfully:

```python
from fraiseql import _fraiseql_rs

# Create executor
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

# Register subscription
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { users { id } }",
    variables={},
    user_id="user1",
    tenant_id="tenant1",
)

# Publish event
executor.publish_event(
    event_type="userCreated",
    channel="users",
    data={"id": "123", "name": "Alice"},
)

# Get response
response_bytes = executor.next_event("sub1")
if response_bytes:
    import json
    response = json.loads(response_bytes)
    assert response["type"] == "next"
    print("✅ Phase 1 complete!")
```

### Performance Baseline
- [ ] Methods respond in <1ms
- [ ] Memory usage stable
- [ ] No obvious performance issues

---

## Phase 1 Success Criteria Met

- [ ] ✅ PySubscriptionExecutor callable from Python
- [ ] ✅ Can register subscriptions
- [ ] ✅ Can publish events
- [ ] ✅ Can retrieve pre-serialized responses
- [ ] ✅ Unit tests pass
- [ ] ✅ Compilation clean
- [ ] ✅ No blocking issues

---

## Next Steps

Once Phase 1 is complete:
1. **Commit changes** with message: `feat: Phase 1 - PyO3 core bindings for GraphQL subscriptions`
2. **Update project status** to Phase 1 ✅ Complete
3. **Start Phase 2** - Event distribution engine
4. **Notify team** that Phase 1 is ready for review

---

## Help Resources

- **Reference Code**: `auth/py_bindings.rs`, `apq/py_bindings.rs`
- **Existing Patterns**: Global runtime access, PyO3 conversions
- **Planning Docs**: `IMPLEMENTATION_QUICK_START.md` has code examples
- **Senior Help**: For complex FFI issues or unclear patterns

---

**Phase 1 Checklist Complete**: Ready for implementation</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-1-checklist.md