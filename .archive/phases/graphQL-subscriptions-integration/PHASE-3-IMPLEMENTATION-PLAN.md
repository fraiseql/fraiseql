# Phase 3: Python Resolver Integration - Implementation Plan

**Phase**: 3 - Python Resolver Integration & Response Generation
**Status**: Planning
**Objective**: Integrate Python resolver functions with Rust dispatch engine, generate actual GraphQL responses from event data
**Estimated Time**: 2 weeks / 30 hours
**Files to Modify**: 3 Rust files, 1 Python module (~300 lines added)
**Success Criteria**:
- Python resolvers are called for each event delivery
- Resolver results are properly transformed to GraphQL responses
- GIL is properly managed across async Rust/Python boundary
- Response bytes are correctly formatted and queued
- All Phase 3 tests pass (35+ tests)

---

## Context

Phase 3 completes the event delivery pipeline by integrating user-defined Python resolver functions. In Phase 2, we had a placeholder that echoed event data. Now we'll actually invoke Python functions with proper error handling, GIL management, and result transformation.

**Key Design Decisions**:
- Store `Py<PyAny>` resolver functions in executor (already in place via resolvers map)
- Use `Python::with_gil()` to safely access Python objects
- Call resolver asynchronously, blocking on Python/GIL as needed
- Transform resolver results to GraphQL response format
- Proper error handling with detailed error messages

---

## Files to Create/Modify

### Modified Files
1. `fraiseql_rs/src/subscriptions/executor.rs` (~100 lines)
   - Implement real `invoke_python_resolver()`
   - Add response transformation logic
   - Add error handling for resolver failures

2. `fraiseql_rs/src/subscriptions/py_bindings.rs` (~80 lines)
   - Store resolver functions when subscriptions register
   - Expose method to register resolvers
   - Handle Python callback exceptions

3. `fraiseql_rs/src/subscriptions/error.rs` (~30 lines)
   - Add resolver-specific error types
   - Add response formatting error types

4. `src/fraiseql/subscriptions.py` (NEW - ~150 lines)
   - Python utilities for resolver registration
   - Response formatting helpers
   - Resolver type hints and documentation

### Test Files
- `tests/test_subscriptions_phase3.py` (NEW - ~600 lines)
  - Resolver registration tests
  - Basic resolver invocation tests
  - Error handling tests
  - Response formatting tests
  - End-to-end integration tests
  - Performance tests

---

## Detailed Implementation Tasks

### Task 3.1: Python Resolver Registration (4 hours)

**Objective**: Allow Python code to register resolver functions for subscriptions

**Status**: Depends on Phase 2 `resolvers: Arc<DashMap<String, Py<PyAny>>>`

**Steps**:

1. Add `register_resolver()` method to `PySubscriptionExecutor`
   - Takes subscription_id and Python callable
   - Stores in resolvers map using `Py::from()`
   - Validates callable is actually a function

2. Update `register_subscription()` to optionally take resolver function
   - If not provided, use default echo resolver

3. Add resolver validation
   - Check signature compatibility
   - Warn if resolver doesn't match expected parameters

**Code Example**:

```python
# Python usage
def my_order_updated_resolver(event_data: dict, subscription_vars: dict) -> dict:
    """
    Called when an order update event is received.

    Args:
        event_data: The raw event data from the database
        subscription_vars: Variables from the subscription query

    Returns:
        GraphQL response data matching the subscription selection set
    """
    order_id = event_data.get('order_id')
    return {
        'id': order_id,
        'status': event_data.get('status'),
        'updated_at': event_data.get('updated_at'),
        'items': event_data.get('items', [])
    }

executor = PySubscriptionExecutor()

# Register resolver for a subscription
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { orderUpdated { id status updatedAt items { id } } }",
    operation_name="OrderUpdated",
    variables={},
    user_id=1,
    tenant_id=1,
)

# Register the resolver function
executor.register_resolver("sub1", my_order_updated_resolver)
```

**Acceptance Criteria**:
- [ ] `register_resolver(sub_id, callable)` method works
- [ ] Resolvers stored in DashMap keyed by subscription_id
- [ ] Function validation prevents invalid resolvers
- [ ] Tests verify resolver registration succeeds

---

### Task 3.2: Python Resolver Invocation (6 hours)

**Objective**: Actually call Python resolver functions when events arrive

**Current State**: `invoke_python_resolver()` is a placeholder that echoes event data

**Steps**:

1. Replace placeholder implementation with real invocation:
   ```rust
   async fn invoke_python_resolver(
       &self,
       subscription_id: &str,
       event_data: &Value,
   ) -> Result<Value, SubscriptionError> {
       // Get stored resolver function from resolvers map
       let resolver_func = self.resolvers
           .get(subscription_id)
           .ok_or(SubscriptionError::ResolverNotFound)?;

       // Clone the Py<PyAny> reference
       let resolver_py = resolver_func.value().clone();

       // Convert event_data to Python dict
       let event_dict = rust_value_to_python_dict(event_data)?;

       // Call resolver with GIL
       let result = pyo3::Python::with_gil(|py| {
           resolver_py.call1(py, (event_dict,))
               .map_err(|e| SubscriptionError::ResolverError(e.to_string()))
       })?;

       // Convert result back to JSON Value
       python_value_to_rust(result)
   }
   ```

2. Add proper error handling:
   - Resolver not found → error
   - Resolver raised exception → capture and report
   - Resolver returned invalid type → error with details
   - Resolver took too long → timeout

3. Add performance monitoring:
   - Track resolver call duration
   - Count resolver errors
   - Log slow resolvers

**Key Technical Details**:

- **GIL Management**: Use `Python::with_gil()` to safely access Python
- **Type Conversion**:
  - JSON Value → Python dict (using `value_to_python()`)
  - Python object → JSON Value (using `python_to_value()`)
- **Async/Sync Bridge**: Resolver is synchronous Python, call is async Rust
  - Use `tokio::task::spawn_blocking()` to avoid blocking async runtime
- **Error Propagation**: Convert Python exceptions to SubscriptionError

**Rust Code Structure**:

```rust
impl SubscriptionExecutor {
    async fn invoke_python_resolver(
        &self,
        subscription_id: &str,
        event_data: &Value,
    ) -> Result<Value, SubscriptionError> {
        // Get resolver from map
        let resolver_opt = self.resolvers.get(subscription_id);
        let resolver = match resolver_opt {
            Some(entry) => entry.value().clone(),
            None => {
                // Fall back to echo resolver if not registered
                return Ok(event_data.clone());
            }
        };

        // Prepare arguments for resolver
        let event_data_clone = event_data.clone();

        // Call resolver on blocking thread pool
        let result = tokio::task::spawn_blocking(move || {
            pyo3::Python::with_gil(|py| {
                // Convert JSON to Python dict
                let event_dict = serde_json::to_string(&event_data_clone)
                    .and_then(|s| Ok(py.eval(&s, None, None)?))?;

                // Call resolver
                let resolver_result = resolver.call1(py, (event_dict,))?;

                // Convert back to JSON
                let result_string = py
                    .import("json")?
                    .getattr("dumps")?
                    .call1((resolver_result,))?
                    .extract::<String>()?;

                serde_json::from_str(&result_string)
                    .map_err(|e| pyo3::PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        format!("Failed to parse resolver result: {}", e)
                    ))
            })
        })
        .await
        .map_err(|e| SubscriptionError::ResolverError(format!("Resolver task panicked: {}", e)))?
        .map_err(|e| SubscriptionError::ResolverError(e.to_string()))?;

        Ok(result)
    }
}
```

**Acceptance Criteria**:
- [ ] Resolver functions are actually called
- [ ] Resolver results are converted back to JSON
- [ ] GIL is properly managed (no deadlocks)
- [ ] Resolver exceptions are caught and reported
- [ ] Performance monitoring is in place
- [ ] Tests verify resolver invocation works

---

### Task 3.3: Response Transformation & Serialization (4 hours)

**Objective**: Transform resolver results into properly formatted GraphQL responses

**Current State**: `serialize_response()` exists but just returns event data as JSON

**Steps**:

1. Implement proper GraphQL response format:
   ```json
   {
     "type": "data",
     "id": "subscription-id",
     "payload": {
       "data": {
         "subscriptionField": {
           "field1": "value1",
           "field2": "value2"
         }
       }
     }
   }
   ```

2. Parse subscription query to understand structure
   - Extract field names and types
   - Match resolver result to expected shape
   - Handle nested fields and arrays

3. Validate resolver result against subscription schema
   - Check required fields are present
   - Ensure types match expectations
   - Handle missing optional fields

4. Serialize to GraphQL wire format
   - Use `serde_json` for JSON serialization
   - Format as UTF-8 bytes
   - Include proper message envelope

**Code Structure**:

```rust
impl SubscriptionExecutor {
    fn serialize_response(
        &self,
        subscription_id: &str,
        resolver_result: &Value,
    ) -> Result<Vec<u8>, SubscriptionError> {
        // Get subscription to find operation name
        let sub = self.subscriptions_secure
            .get(subscription_id)
            .ok_or(SubscriptionError::SubscriptionNotFound)?;

        let operation_name = &sub.subscription.operation_name;

        // Build GraphQL response message
        let response = json!({
            "type": "data",
            "id": subscription_id,
            "payload": {
                "data": {
                    operation_name.as_ref().unwrap_or(&"subscription".to_string()): resolver_result
                }
            }
        });

        // Serialize to JSON bytes
        serde_json::to_vec(&response)
            .map_err(|e| SubscriptionError::SerializationError(e.to_string()))
    }
}
```

**Acceptance Criteria**:
- [ ] Responses follow GraphQL subscription message format
- [ ] All resolver results are properly serialized
- [ ] Types are validated against subscription expectations
- [ ] Error responses include meaningful messages
- [ ] Tests verify response format is correct

---

### Task 3.4: Error Handling & Recovery (3 hours)

**Objective**: Handle all failure modes gracefully

**Error Scenarios**:

1. **Resolver Not Found**
   - Subscribe without registering resolver
   - Use default echo resolver
   - Log warning to user

2. **Resolver Raised Exception**
   - Catch Python exception
   - Convert to GraphQL error format
   - Queue error response to client
   - Log exception for debugging

3. **Resolver Returned Wrong Type**
   - Validation failed
   - Return GraphQL error with details
   - Include what was expected vs what was received

4. **Resolver Timeout**
   - Set timeout on resolver execution
   - Cancel if takes > N seconds
   - Return timeout error to client

5. **GIL Deadlock**
   - Shouldn't happen with proper `with_gil()` usage
   - Add safeguards if needed
   - Implement GIL timeout

**Error Response Format**:

```json
{
  "type": "error",
  "id": "subscription-id",
  "payload": {
    "errors": [
      {
        "message": "Resolver error: NameError: undefined variable 'x'",
        "extensions": {
          "resolver_error": true,
          "exception_type": "NameError"
        }
      }
    ]
  }
}
```

**Code Structure**:

```rust
pub enum SubscriptionError {
    // ... existing variants ...

    // NEW: Resolver-specific errors
    ResolverNotFound,
    ResolverError(String),
    ResolverTimeout,
    ResolverTypeError(String),
    SerializationError(String),
}

impl SubscriptionExecutor {
    async fn invoke_python_resolver_with_timeout(
        &self,
        subscription_id: &str,
        event_data: &Value,
    ) -> Result<Value, SubscriptionError> {
        // Set timeout for resolver execution
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.invoke_python_resolver(subscription_id, event_data)
        ).await {
            Ok(result) => result,
            Err(_) => Err(SubscriptionError::ResolverTimeout),
        }
    }

    fn build_error_response(
        subscription_id: &str,
        error: &SubscriptionError,
    ) -> Result<Vec<u8>, SubscriptionError> {
        let error_msg = match error {
            SubscriptionError::ResolverNotFound => "Resolver not registered".to_string(),
            SubscriptionError::ResolverError(msg) => format!("Resolver error: {}", msg),
            SubscriptionError::ResolverTimeout => "Resolver execution timeout".to_string(),
            _ => "Unknown error".to_string(),
        };

        let response = json!({
            "type": "error",
            "id": subscription_id,
            "payload": {
                "errors": [{
                    "message": error_msg
                }]
            }
        });

        serde_json::to_vec(&response)
            .map_err(|e| SubscriptionError::SerializationError(e.to_string()))
    }
}
```

**Acceptance Criteria**:
- [ ] All resolver error cases are handled
- [ ] Errors are formatted as GraphQL errors
- [ ] Errors are queued to client
- [ ] Errors are logged for debugging
- [ ] Executor continues after errors (resilient)

---

### Task 3.5: Integration Testing & Validation (3 hours)

**Objective**: Comprehensive tests for Phase 3 functionality

**Test Categories**:

1. **Resolver Registration** (4 tests)
   - Register resolver for subscription
   - Replace resolver with new function
   - Resolver not found uses default
   - Invalid resolver rejected

2. **Resolver Invocation** (5 tests)
   - Resolver called with correct arguments
   - Resolver result used in response
   - Multiple resolvers called in parallel
   - Resolver handles complex data types
   - Resolver can access subscription variables

3. **Error Handling** (5 tests)
   - Resolver raises exception
   - Resolver returns wrong type
   - Resolver timeout is handled
   - Error response formatted correctly
   - Executor continues after error

4. **Response Formatting** (4 tests)
   - Response has correct structure
   - Operation name included
   - Subscription ID included
   - Nested fields serialized correctly

5. **End-to-End Workflows** (6 tests)
   - Full: register → resolver → response → queue
   - Multiple events trigger multiple resolver calls
   - Resolver with complex transformations
   - Error recovery and retry
   - Performance under load
   - Memory safety (no leaks)

6. **Performance Benchmarks** (2 tests)
   - Resolver invocation latency
   - Throughput: N resolvers per second

**Total Phase 3 Tests**: 35+ tests

**Example Test**:

```python
def test_resolver_invocation():
    """Test that Python resolver is actually called"""
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

    # Track resolver calls
    call_count = 0
    def test_resolver(event_data):
        nonlocal call_count
        call_count += 1
        return {
            "id": event_data.get("id"),
            "transformed": True,
            "call_count": call_count
        }

    # Register subscription
    sub_id = executor.register_subscription(
        connection_id="conn1",
        subscription_id="sub1",
        query="subscription { test { id transformed } }",
        operation_name=None,
        variables={},
        user_id=1,
        tenant_id=1,
    )

    # Register resolver
    executor.register_resolver(sub_id, test_resolver)

    # Publish event
    executor.publish_event(
        event_type="test",
        channel="test",
        data={"user_id": 1, "tenant_id": 1, "id": "123"}
    )

    # Verify resolver was called
    assert call_count == 1

    # Publish another event
    executor.publish_event(
        event_type="test",
        channel="test",
        data={"user_id": 1, "tenant_id": 1, "id": "456"}
    )

    # Verify resolver called again
    assert call_count == 2
```

**Acceptance Criteria**:
- [ ] All 35+ tests pass
- [ ] Tests cover all error scenarios
- [ ] Performance meets targets
- [ ] No memory leaks detected
- [ ] Tests run in < 1 second

---

## Implementation Sequence

1. **Task 3.1** (4h): Python resolver registration
   - Add `register_resolver()` method
   - Add validation
   - Tests for registration

2. **Task 3.2** (6h): Python resolver invocation
   - Implement real `invoke_python_resolver()`
   - GIL management
   - Tests for invocation

3. **Task 3.3** (4h): Response transformation
   - Proper response formatting
   - Validation
   - Tests for responses

4. **Task 3.4** (3h): Error handling
   - All error scenarios covered
   - Error responses formatted
   - Tests for errors

5. **Task 3.5** (3h): Integration testing
   - Comprehensive test suite
   - Performance validation
   - All tests passing

**Total**: 20 hours (estimated 2-3 days of focused development)

---

## Success Criteria

### Functional
- [ ] Python resolvers are called for each subscription event
- [ ] Resolver results are transformed to GraphQL responses
- [ ] All resolver error cases handled gracefully
- [ ] Responses properly formatted and queued
- [ ] GIL properly managed (no crashes/deadlocks)

### Performance
- [ ] Single resolver invocation: < 10ms
- [ ] 100 parallel resolvers: < 50ms
- [ ] Throughput: 1000 events/sec
- [ ] No memory leaks

### Quality
- [ ] 35+ tests with 100% pass rate
- [ ] All error scenarios tested
- [ ] Code is well-documented
- [ ] Clean compilation with no warnings

### Integration
- [ ] Works with Phase 2 dispatch engine
- [ ] Compatible with all security filters
- [ ] Response queuing works correctly
- [ ] Next Phase (4) can start cleanly

---

## Risk Mitigation

### Risk 1: GIL Deadlock
**Mitigation**:
- Use `Python::with_gil()` exclusively
- Never hold GIL across async boundaries
- Test with stress testing

### Risk 2: Type Mismatch
**Mitigation**:
- Validate resolver returns dict-like object
- Schema validation before queueing
- Detailed error messages

### Risk 3: Resolver Exceptions
**Mitigation**:
- Catch all Python exceptions
- Convert to GraphQL errors
- Log for debugging

### Risk 4: Performance Regression
**Mitigation**:
- Benchmark before/after
- Profile resolver calls
- Optimize hot paths

---

## Deliverables

1. **Updated Rust Files**
   - `fraiseql_rs/src/subscriptions/executor.rs` - Real resolver invocation
   - `fraiseql_rs/src/subscriptions/py_bindings.rs` - Resolver registration API
   - `fraiseql_rs/src/subscriptions/error.rs` - New error types

2. **New Python Module**
   - `src/fraiseql/subscriptions.py` - Helper utilities

3. **Test Suite**
   - `tests/test_subscriptions_phase3.py` - 35+ comprehensive tests

4. **Documentation**
   - Resolver implementation guide
   - Error handling guide
   - API documentation
   - Example resolvers

---

## Next Phase (Phase 4)

Phase 4 will focus on:
- Advanced resolver patterns (caching, batching)
- Rate limiting per resolver
- Metrics and monitoring
- WebSocket frame handling
- Backpressure and flow control

---

## Timeline

- **Start**: Immediately after Phase 2
- **Duration**: 2-3 days of focused development
- **End**: Phase 3 complete and tested
- **Next**: Begin Phase 4
