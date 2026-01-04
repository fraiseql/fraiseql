# Commit 4.5 Plan: GraphQL Operation Monitoring

**Date**: January 4, 2026
**Phase**: Phase 19, Commit 4.5 of 9
**Status**: Planning

---

## 🎯 Objective

Extend FraiseQL's GraphQL execution layer with comprehensive operation-level monitoring, enabling detection and tracking of slow mutations, queries, and subscriptions. Integrates with W3C trace context from Commit 2 for distributed tracing.

---

## 📋 Scope

### What's Included
- ✅ Query execution time tracking
- ✅ Mutation execution time tracking (including slow detection)
- ✅ Subscription monitoring
- ✅ Resolver-level latency metrics
- ✅ Error tracking per operation
- ✅ Integration with trace context (Commit 2)
- ✅ Integration with Prometheus metrics

### What's NOT Included
- ❌ Database monitoring (Commit 4)
- ❌ Audit log storage (Commit 5)
- ❌ Health checks (Commit 6)
- ❌ Query complexity analysis (future)

---

## 🏗️ Architecture

### Layer Separation

```
HTTP Request (with trace context from Commit 2)
    ↓
FastAPI Route Handler
    ↓
GraphQL Execution Engine
    ├─ Query Resolver Layer (THIS COMMIT)
    │   ├─ Start timer
    │   ├─ Execute resolver
    │   ├─ Record latency
    │   └─ Track if slow
    ├─ Mutation Resolver Layer (THIS COMMIT) ← MUTATION SLOW DETECTION
    │   ├─ Start timer
    │   ├─ Execute mutation
    │   ├─ Record latency
    │   └─ Detect slow (config threshold)
    └─ Subscription Layer (THIS COMMIT)
        ├─ Track open subscriptions
        └─ Monitor active connections
    ↓
Response (with metrics recorded)
```

### Key Integration Points

1. **With Commit 1 (Config)**
   - Read `graphql_slow_operation_threshold_ms` (NEW config field)
   - Read `observability_enabled` and `metrics_enabled`

2. **With Commit 2 (Tracing)**
   - Correlate operations with `trace_id` and `span_id`
   - Include operation metrics in trace context
   - Log slow operations with full trace correlation

3. **With Existing Metrics (FraiseQLMetrics)**
   - `graphql_queries_total` (already exists)
   - `graphql_mutations_total` (already exists)
   - `graphql_query_duration_seconds` (already exists)
   - `graphql_mutation_duration_seconds` (already exists)
   - `graphql_slow_operations_total` (NEW counter)

---

## 📝 Implementation Plan

### Phase 1: Core Module (`src/fraiseql/monitoring/graphql_operations.py`)

```python
@dataclass
class OperationMetrics:
    """Metrics for a single GraphQL operation."""
    operation_name: str
    operation_type: str  # 'query', 'mutation', 'subscription'
    start_time_ms: float
    end_time_ms: float
    duration_ms: float
    trace_id: str | None
    span_id: str | None
    error: str | None = None
    is_slow: bool = False

class GraphQLOperationMonitor:
    """Monitor GraphQL query/mutation/subscription execution."""

    def start_operation(
        self,
        operation_name: str,
        operation_type: str,
        trace_id: str | None = None,
        span_id: str | None = None,
    ) -> str:
        """Start monitoring an operation, return operation_id."""

    def end_operation(
        self,
        operation_id: str,
        error: str | None = None,
    ) -> OperationMetrics:
        """End monitoring operation, return metrics."""

    def is_slow(self, duration_ms: float) -> bool:
        """Check if operation duration exceeds threshold."""

    def get_slow_operations(self) -> list[OperationMetrics]:
        """Get all slow operations recorded."""

    def record_metrics_to_prometheus(self, metrics: OperationMetrics) -> None:
        """Record operation metrics to Prometheus."""

class GraphQLOperationMiddleware:
    """GraphQL middleware to automatically track operations."""

    async def __call__(self, request, call_next):
        """Wrap GraphQL execution with operation monitoring."""
```

### Phase 2: Configuration Extension

**File**: `src/fraiseql/fastapi/config.py` (Commit 1 - extend)

Add new config field:
```python
graphql_slow_operation_threshold_ms: int = Field(
    default=1000,  # 1 second
    gt=0,
    description="Threshold in milliseconds for slow GraphQL operations"
)
```

### Phase 3: Middleware Integration

**File**: `src/fraiseql/fastapi/app_factory.py` (or appropriate app setup)

```python
def setup_graphql_operation_monitoring(
    app: FastAPI,
    config: FraiseQLConfig,
) -> None:
    """Setup GraphQL operation monitoring middleware."""
    if config.observability_enabled and config.metrics_enabled:
        app.add_middleware(GraphQLOperationMiddleware, config=config)
```

### Phase 4: Integration with Existing Systems

**Integration Points**:
1. Hook into GraphQL execution (before/after resolver execution)
2. Extract trace context from request state (Commit 2)
3. Record to FraiseQLMetrics (existing Prometheus system)
4. Correlate with query name and operation type

---

## 🧪 Test Strategy

**File**: `tests/unit/observability/test_graphql_operations.py`

### Test Classes

#### TestOperationMetrics (8 tests)
- ✅ Creation with all fields
- ✅ Duration calculation
- ✅ is_slow detection
- ✅ Serialization to dict

#### TestGraphQLOperationMonitor (15 tests)
- ✅ Starting/ending operations
- ✅ Latency tracking
- ✅ Slow operation detection
- ✅ Error recording
- ✅ Trace context correlation
- ✅ Getting slow operations
- ✅ Resetting metrics

#### TestGraphQLOperationIntegration (12 tests)
- ✅ Query operation tracking
- ✅ Mutation operation tracking (including slow detection)
- ✅ Subscription operation tracking
- ✅ Error handling
- ✅ Trace context propagation
- ✅ Prometheus metrics recording

#### TestIntegrationScenarios (5 tests)
- ✅ Typical slow mutation scenario
- ✅ Multiple operations with tracing
- ✅ Error in slow operation
- ✅ Configuration threshold changes
- ✅ Concurrent operation tracking

**Total Tests**: ~40 tests
**Expected Coverage**: 100%
**Execution Time**: ~0.1s

---

## 📊 Code Statistics

| Metric | Estimate |
|--------|----------|
| **Implementation LOC** | ~250 |
| **Test LOC** | ~600 |
| **Total LOC** | ~850 |
| **Test Cases** | 40 |
| **New Config Fields** | 1 |
| **New Classes** | 3 |
| **New Functions** | 8+ |

---

## 🔗 Dependencies

### Depends On
- ✅ Commit 1: FraiseQLConfig (config system)
- ✅ Commit 2: TraceContext (trace_id, span_id)
- ✅ Existing: FraiseQLMetrics (Prometheus integration)
- ✅ Existing: GraphQL execution system

### Used By
- ✅ Commit 5: Audit logs (slow mutations recorded)
- ✅ Commit 8: Integration tests (operation tracking verified)

---

## 💡 Key Design Decisions

### 1. **Separate from Database Monitoring (Commit 4)**
- Database monitoring tracks SQL queries
- GraphQL Operation monitoring tracks GraphQL operations (mutations, queries)
- Clear separation of concerns
- Each layer monitors its own operations

### 2. **Mutation Slow Detection in Operation Layer**
- Not in Commit 4 (database queries)
- At Commit 4.5 (GraphQL operations)
- Why: A slow mutation might be slow due to business logic, not just database
- Captures full operation latency, not just query time

### 3. **Integration with Trace Context**
- Every operation correlated with trace_id and span_id from Commit 2
- Enables distributed tracing of slow operations
- Allows correlation across service boundaries

### 4. **Config Threshold**
- `graphql_slow_operation_threshold_ms` configurable per environment
- Development: 500ms (catch slow operations early)
- Production: 2000ms (focus on real performance issues)

---

## 🎯 Success Criteria

- [ ] Core module created with OperationMetrics, GraphQLOperationMonitor
- [ ] Middleware for GraphQL execution created
- [ ] Config field added to FraiseQLConfig
- [ ] 40+ unit tests written and passing
- [ ] 100% code coverage on new code
- [ ] Slow mutation detection working
- [ ] Trace context correlation working
- [ ] Prometheus metrics integration working
- [ ] Zero overhead when disabled
- [ ] Documentation complete with examples

---

## ✅ Definition of Done

- [ ] All code committed
- [ ] All tests passing (40/40)
- [ ] Code review complete
- [ ] Documentation written
- [ ] Examples provided
- [ ] No breaking changes
- [ ] Backward compatible
- [ ] Performance verified (<0.5ms overhead per operation)

---

## 📅 Timeline

- **Estimated Implementation**: 1 day
- **Estimated Testing**: 0.5 day
- **Estimated Documentation**: 0.5 day
- **Total**: ~2 days (could be done in parallel with Commit 4)

---

## 🔄 Comparison: Before vs After

### Before (without Commit 4.5)
```
Slow DB query → Visible in Commit 4
Slow mutation → NOT visible (no GraphQL operation monitoring)
Slow resolver → NOT visible
```

### After (with Commit 4.5)
```
Slow DB query → Visible in Commit 4
Slow mutation → VISIBLE in Commit 4.5 (with trace correlation)
Slow resolver → VISIBLE in Commit 4.5
```

---

## 📝 Next Steps

1. **Finalize Architecture** - Confirm layer separation approach
2. **Create Module** - Implement core GraphQL operation monitoring
3. **Add Tests** - Write comprehensive test suite
4. **Integrate Config** - Add slow operation threshold to FraiseQLConfig
5. **Integrate Middleware** - Hook into GraphQL execution
6. **Documentation** - Write usage examples and architecture docs
7. **Code Review** - Submit for team review
8. **Merge** - Merge to dev branch

---

## 🎉 Why This Works Long-Term

✅ **Clean layer separation**: Database, Cache, Operations, Business each have own monitoring
✅ **Scalable**: Easy to add more operation-level metrics in future
✅ **Maintainable**: Clear ownership and responsibilities
✅ **Testable**: Each layer testable independently
✅ **Extensible**: Foundation for subscription monitoring, custom resolver metrics, etc.

---

*Plan Created: January 4, 2026*
*Status: Ready for Implementation*
*Next: Implement after Commit 4*
