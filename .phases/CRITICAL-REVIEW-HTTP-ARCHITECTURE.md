# Critical Review: Pluggable HTTP Servers Architecture

**Date**: January 5, 2026
**Document Reviewed**: `.phases/PLUGGABLE-HTTP-SERVERS.md`
**Reviewer**: Self (Critical Analysis)
**Purpose**: Identify weaknesses, risks, and unfounded assumptions before implementation

---

## Executive Summary

The architecture is **well-structured and directionally correct**, but has **critical gaps** that will cause problems if not addressed:

| Category | Rating | Status |
|----------|--------|--------|
| **Overall Vision** | ⭐⭐⭐⭐ | Strong, clear objective |
| **Architecture Design** | ⭐⭐⭐ | Good but oversimplified |
| **Phase Planning** | ⭐⭐⭐⭐ | Detailed and thorough |
| **Risk Assessment** | ⭐⭐ | **CRITICAL GAP** |
| **Technical Feasibility** | ⭐⭐⭐ | Achievable but with caveats |
| **Testing Strategy** | ⭐⭐⭐ | Good, but misses edge cases |

---

## 🚨 Critical Issues (Must Address Before Implementation)

### Issue 1: Protocol Boundary Complexity Not Addressed

**The Problem**:
The design shows a clean abstraction boundary between HTTP server and core framework:

```
HTTP Server Layer
    ↓
Abstraction Layer
    ↓
Core Framework
```

**Reality is much messier**:

```
HTTP Framework (Axum/Starlette/FastAPI)
    ↓
    ├─ Stream handling (body reading, backpressure)
    ├─ Middleware execution order/hooks
    ├─ Error handling (HTTPException vs custom errors)
    ├─ Type system (how to represent Optional fields)
    ├─ Request context (session, state, dependency injection)
    ├─ Response streaming (Server-Sent Events)
    ├─ WebSocket protocol (connection, ping/pong, close codes)
    ├─ Multipart file uploads
    ├─ HTTP headers (CORS, caching, security)
    └─ Backpressure/flow control
```

**Impact**: The abstraction layer in `HttpContext` and `HttpResponse` is **too simple** to capture these differences.

**Example - Middleware Order**:
```python
# Axum middleware order (explicit)
.layer(middleware1)  # outer
.layer(middleware2)  # inner
// Execution order: middleware2 → middleware1 (reverse)

# Starlette middleware order (order of addition)
.add_middleware(middleware1)  # outer
.add_middleware(middleware2)  # inner
// Execution order: middleware1 → middleware2 (same order)
```

**Question for plan**: How do we guarantee identical middleware execution order across all servers if the frameworks execute middleware differently?

**Recommendation**:
- Add explicit middleware interface with guaranteed execution order
- Document framework-specific quirks in detail
- Create middleware adapter layer (not just handler layer)

---

### Issue 2: Request Context Building is Oversimplified

**The Problem**:

Current `HttpContext` design:
```python
@dataclass
class HttpContext:
    request_body: dict[str, Any]
    headers: dict[str, str]
    user: Any | None
    variables: dict[str, Any] | None
    operation_name: str | None
```

**This misses critical concerns**:

1. **Streaming Request Bodies**
   - FastAPI: Can handle streaming bodies
   - Starlette: Same as FastAPI
   - Axum: Efficient streaming via extractors
   - **Problem**: All three have different streaming APIs

2. **Request Parsing Errors**
   - What if JSON is invalid?
   - What if multipart is malformed?
   - Timing: Parse before or after context building?

3. **Authentication Context**
   - FastAPI: Uses Depends() dependency injection
   - Starlette: Uses request.scope
   - Axum: Uses Request extractors
   - **Problem**: Can't abstract away these differences

4. **Request Scoping**
   - Transaction scope?
   - Database connection lifetime?
   - Cache invalidation timing?

5. **Async Context Variables**
   - Execution context propagation
   - Tracing context (OpenTelemetry)
   - Logging context

**Example that breaks the abstraction**:
```python
# Axum - Request extracted as struct
#[derive(FromRequest)]
struct GraphQLRequest {
    body: Json<...>,
    headers: HeaderMap,
}

// This is type-safe, zero-copy

// Starlette - Request parsed manually
request = Request(scope, receive, send)
body = await request.json()
headers = dict(request.headers)

// This is dynamic, allocates

// FastAPI - Depends() with annotations
async def handler(request: Request, schema: GraphQLSchema = Depends(get_schema)):
    ...

// This requires runtime introspection
```

**Recommendation**:
- Make `HttpContext` extensible: `HttpContext.extra: dict[str, Any]`
- Store framework-specific request objects: `HttpContext.raw_request: Any`
- Document which context is passed to which handlers
- Consider abandoning full abstraction for request handling—let each server customize

---

### Issue 3: WebSocket/Subscriptions Cannot Be Fully Abstracted

**The Problem**:

Current plan shows:
```python
async def handle_subscriptions(self, context: HttpContext) -> AsyncIterator[HttpResponse]:
    """WebSocket subscriptions"""
```

**But WebSocket handling is fundamentally different**:

| Framework | WebSocket API | State Management | Error Handling |
|-----------|---------------|------------------|-----------------|
| **Axum** | `WebSocketUpgrade` extractor | Message buffering | `Error` type |
| **Starlette** | `WebSocket` with `accept/send/receive` | Automatic backpressure | Exceptions |
| **FastAPI** | `WebSocket` with `accept/send_json/receive_json` | Higher-level API | Exceptions |

**Real problems**:

1. **Connection Lifecycle**
   - When does subscription start? (on accept? on first message?)
   - When does it end? (client close? server error? timeout?)
   - Who manages the connection state?

2. **Message Format**
   - GraphQL-WS protocol (Apollo Subscriptions)
   - GraphQL-Transport-WS (newer standard)
   - Raw JSON (custom)
   - Which one is the "canonical" implementation?

3. **Backpressure**
   - If client can't keep up, buffer or disconnect?
   - Timeout on slow clients?
   - Backpressure propagation to database?

4. **Error Recovery**
   - Subscription fails mid-stream—what happens?
   - Client can't receive—reconnect or fail?
   - Server crashes—client knows?

**Current plan weakness**: Treats subscriptions as "just another handler" when they're fundamentally asynchronous streams with different semantics.

**Recommendation**:
- Implement subscriptions FIRST in one server (Axum), document fully
- Delay Starlette/FastAPI subscription support to separate phase
- Accept that subscription behavior may differ initially
- Plan for subscription-specific testing, not just parity tests

---

### Issue 4: Testing Strategy Assumes Identical Behavior Is Possible

**The Problem**:

Plan says:
```python
async def test_identical_graphql_results(self, http_server):
    """All servers produce identical GraphQL results"""
```

**But they WON'T be identical in all cases**:

1. **Timing/Concurrency**
   - Axum might handle 10,000 concurrent requests
   - Starlette might handle 1,000
   - Response times will differ
   - Test timeout assumptions are different

2. **Error Messages**
   - Axum: Rust error format
   - Starlette: Python exception format
   - FastAPI: FastAPI-specific validation errors
   - Can you guarantee identical error text?

3. **Headers**
   - CORS headers (Axum middleware vs Starlette middleware)
   - Cache headers (different computation?)
   - Custom headers (X-Custom-Middleware)

4. **Body Parsing Differences**
   - Invalid JSON handling
   - Large payload limits
   - Encoding issues (UTF-8 variants)
   - Null byte handling

5. **HTTP Semantics**
   - Status codes (400 vs 422 for validation?)
   - Content-Type handling
   - Compression
   - Keep-Alive behavior

**Example that will break**:
```python
# Test assumes identical error format
async def test_identical_error_messages(self, http_server):
    context = HttpContext(request_body={"query": "{ invalid }"}, ...)
    response = await http_server.handle_graphql(context)
    assert "errors" in response.body
    assert response.body["errors"][0]["message"] == "Field not found"
    # ❌ This will fail:
    # - Axum: "Field not found (at position 1)"
    # - Starlette: "Field 'invalid' not found"
    # - FastAPI: "GraphQL error: invalid field"
```

**Recommendation**:
- Define "parity" more carefully—identical results for VALID inputs, not errors
- Accept that error messages will differ
- Test valid query behavior (parity), not invalid behavior
- Test performance characteristics separately (not parity)
- Consider "behavioral compatibility" not "identical behavior"

---

### Issue 5: Axum Implementation Scope Undefined

**The Problem**:

Plan says Axum will have:
- Routing
- Middleware (APQ, auth, tracing)
- Response building
- WebSocket support
- "All existing FastAPI features"

**But never answers**:

1. **What happens to FastAPI's existing code?**
   - 64KB of routers.py
   - APQ metrics router
   - Dev auth
   - Turbo router (GraphQL batching?)
   - Subscription router
   - Which of these move to Axum?

2. **Who manages configuration?**
   - Python side: `FraiseQLConfig` in `src/fraiseql/fastapi/config.py`
   - Rust side: How is config passed to Axum?
   - PyO3 bindings: Are config changes instant or require restart?

3. **Who manages the Rust pipeline?**
   - Axum in Rust calls `fraiseql_rs` functions
   - Or does Axum call Python which calls Rust?
   - If Axum calls Rust directly, how does auth work?

4. **Database connection management**
   - Connection pooling in Python or Rust?
   - Who creates the pool?
   - Who owns connection lifecycle?

5. **Startup/Shutdown**
   - Database migrations on startup?
   - Schema validation?
   - Connection pool warmup?
   - All in Rust? Python? Shared?

**Current answer in plan**: "See Rust implementation details" (doesn't exist)

**Recommendation**:
- Clarify Axum's exact scope BEFORE implementation
- Define clear boundary: What stays in Python, what moves to Rust?
- Create Rust/Python boundary diagram
- Document startup sequence in detail
- Document shutdown gracefully in detail

---

### Issue 6: Performance Claims Are Unvalidated

**The Problem**:

Plan claims:
> "Axum achieves 7-10x speedup over Python servers"

**But this is misleading**:

1. **What are we measuring?**
   - HTTP parsing? (Axum faster)
   - JSON transformation? (Rust pipeline faster)
   - Database query? (Same speed regardless of HTTP server)
   - Full query execution? (Depends on work distribution)

2. **Unfair comparison**
   ```
   Axum (Rust):
   - HTTP parsing (Rust fast)
   - JSON building (Rust fast)
   - Database call (PostgreSQL)
   - Response (Rust fast)

   Starlette (Python):
   - HTTP parsing (Python, calls C)
   - JSON building (calls Rust pipeline via PyO3)
   - Database call (Same, psycopg3)
   - Response (Python)

   // The 7-10x claim assumes all time in JSON transformation
   // But if database is 90% of time, Axum looks only 10% faster
   ```

3. **Real-world queries spend time where?**
   - Parsing GraphQL query: ~1ms (Rust does this already)
   - Planning execution: ~2ms (Rust pipeline or Python?)
   - Running SQL: ~100ms (PostgreSQL, same for both)
   - Serializing response: ~5ms (Axum fast, Starlette slow)

   **Result**: Total difference is 5ms not 105ms. That's 1.05x, not 7-10x.

4. **The benchmark will be misleading**
   ```python
   # Simple query benchmark
   { __typename }

   // This is 90% serialization, 10% database
   // Axum wins here: 5ms vs 50ms = 10x

   // But real query:
   { user { id name email posts { id title } comments { id text } } }

   // This is 90% database, 10% serialization
   // Axum wins here: 105ms vs 110ms = 1.05x
   ```

**Recommendation**:
- Benchmark realistic queries, not synthetic ones
- Measure with actual database (not in-memory)
- Include P95, P99 latencies, not just averages
- Document what portion of time is in each layer
- Set realistic performance targets (2-3x not 7-10x)
- Plan separate: "HTTP layer optimization" vs "full query optimization"

---

### Issue 7: FastAPI "Deprecation" Plan is Incomplete

**The Problem**:

Plan says FastAPI will:
- Be marked deprecated in v2.0
- Get removed in v3.0
- Have "clear migration path"

**But ignores**:

1. **Existing users**
   - How many FastAPI users exist?
   - How much effort to migrate?
   - What if they can't migrate? (Legacy code, business constraints)

2. **Breaking changes**
   - v2.0 removes features? Or just marks as deprecated?
   - v2.0 still fully functional?
   - When is actual removal? 6 months? 1 year? 2 years?

3. **Migration difficulty**
   - If Axum is the only Rust server, Starlette is the only Python option
   - Is Starlette a drop-in replacement for FastAPI?
   - What API surface needs to change?

4. **Support burden**
   - v1.9: Support FastAPI fully
   - v2.0: Support FastAPI + new servers
   - v2.1-2.9: Support deprecated FastAPI
   - v3.0: Break existing users
   - When can you actually stop supporting?

**Example risk**:
```python
# User has FastAPI code deployed
app = create_fastapi_app(config)

# v1.9: Works
# v2.0: "deprecated, migrate to Axum"
# v2.5: Code still works, but no new features
# v3.0: "This is removed. Here's migration guide."

// User hasn't migrated because:
// - Code is in "legacy maintenance" mode
// - Team has bandwidth only for critical bugs
// - Migration risk is perceived as high
// - FastAPI works fine

// Now forced to migrate or stay on v2.9
```

**Recommendation**:
- Clarify support timeline upfront (v1.9, v2.0, v2.5, v3.0, v4.0)
- Document actual removal date (not vague "v3.0")
- Create detailed migration guide with examples
- Consider keeping minimal FastAPI support longer (v4.0 instead of v3.0)
- Plan backwards-compatibility shim if possible

---

## ⚠️ High-Risk Design Decisions

### Decision 1: Abstraction-First Approach

**What the plan does**:
1. Design abstract interface
2. Extract business logic
3. Implement servers

**Why this is risky**:
- You don't know what abstraction is needed until you've built at least one server
- The abstraction may be "wrong" once you hit real implementation constraints
- Early abstraction often creates more problems than it solves

**Better approach** (Intel):
1. Build Axum server FIRST (complete, no abstraction)
2. Once Axum works, identify what's framework-specific
3. Extract shared code
4. Build abstraction from actual code (not theoretical)
5. Then implement Starlette

**Risk**: Spending weeks on perfect abstraction, then discovering it doesn't work

---

### Decision 2: Parallel Server Implementation

**What the plan does**:
- Week 4-5: Axum
- Week 6: Starlette (in parallel or sequential?)
- Week 7: FastAPI

**Why this is risky**:
- Parity tests won't pass until BOTH servers are complete
- Can't validate abstraction until you've built two servers
- If Axum implementation finds issues, you redo Starlette

**Better approach**:
- Phase 1: Axum fully complete and tested
- Phase 2: Validate Starlette against Axum
- Phase 3: Refactor both based on learnings

**Risk**: Discovering mid-way that abstraction doesn't work, having to rework Starlette

---

### Decision 3: Single Abstraction for All Concerns

**What the plan does**:
- One `HttpServer` protocol covers routing, middleware, context building, responses, subscriptions

**Why this is risky**:
- These are fundamentally different concerns
- Routing abstraction ≠ Middleware abstraction ≠ Context abstraction
- Bundling them means if one breaks, all are affected

**Better approach**:
- Separate abstractions for each concern
- Route handler abstraction
- Middleware abstraction (separate from handler)
- Context building (separate from execution)
- Response formatting (separate)

**Risk**: Finding out halfway through that you need different abstractions, forcing refactor

---

## 🔴 Missing Pieces

### Missing 1: Error Handling Strategy

**Not addressed**:
- How do HTTP 4xx/5xx errors become GraphQL errors?
- How do GraphQL errors become HTTP responses?
- Are all GraphQL errors 200 OK?
- Are validation errors 400 Bad Request?
- Are authentication errors 401 Unauthorized or 200 with error?

**Impact**: Each server might implement differently, breaking parity

**Needs**: Explicit error mapping specification

---

### Missing 2: Configuration Management

**Not addressed**:
- How is config passed from Python to Rust?
- Can config be changed at runtime?
- Are config changes applied to both servers?
- What if Python config changes but Rust cached the old value?

**Impact**: Bugs where Python and Rust have different config

**Needs**: Configuration synchronization protocol

---

### Missing 3: Database Connection Ownership

**Not addressed**:
- Who creates the connection pool?
- Is it in Python or Rust?
- Who manages connection lifecycle?
- Who handles stale connections?

**Impact**: Connection pooling bugs, connection leaks

**Needs**: Connection management architecture

---

### Missing 4: Logging & Observability

**Not addressed**:
- How are logs aggregated from Rust and Python?
- Are log levels consistent?
- How are traces propagated?
- Are error rates calculated the same way?

**Impact**: Hard to debug cross-language issues

**Needs**: Observability architecture

---

### Missing 5: Graceful Shutdown

**Not addressed**:
- How do you shut down Axum server gracefully?
- How does Rust notify Python layer of shutdown?
- How do in-flight requests complete?
- How do subscriptions close?

**Impact**: Data loss, incomplete requests on shutdown

**Needs**: Shutdown coordination protocol

---

## 🟡 Questionable Assumptions

### Assumption 1: "Identical Behavior" is Achievable

**Plan assumes**: All servers will behave identically

**Reality**: Some differences are fundamental:
- Error messages (framework-specific)
- Response headers (framework-specific)
- Timing/concurrency (framework-specific)
- WebSocket protocol details (framework-specific)

**Fix**: Define "sufficient parity" (95% of cases identical, edge cases allowed to differ)

---

### Assumption 2: Testing Strategy is Sufficient

**Plan shows**: Parametrized tests across all servers

**Problem**:
- Tests only cover what you explicitly test
- Race conditions may only appear on one server
- Large payloads may only fail on one server
- Concurrency bugs may only appear on one server

**Fix**: Add property-based testing, chaos engineering, load testing per-server

---

### Assumption 3: 8-Week Timeline is Realistic

**Plan shows**: Phase 0-5 in 8 weeks (1 week per phase)

**Reality**:
- Phase 1 (abstraction): 1-2 weeks reasonable
- Phase 2 (Axum): 2-3 weeks if you hit issues
- Phase 3 (Starlette): 2-3 weeks minimum
- Phase 4 (FastAPI): 1 week (thin wrapper)
- Phase 5 (testing/docs): 2-3 weeks

**Realistic timeline**: 10-14 weeks, not 8

**Buffer needed**: 40-50% extra time for unforeseen issues

---

### Assumption 4: Rust Pipeline is Performance Bottleneck

**Plan justifies Axum with**: "7-10x faster due to Rust pipeline"

**But**:
- Rust pipeline (JSON transformation) is ALREADY being used in FastAPI
- Axum doesn't change pipeline speed
- Axum might be faster at HTTP parsing/serialization, but that's not 7-10x
- Most time is spent in database queries, not JSON

**Reality**: Axum might be 20-30% faster for full queries, not 7-10x

---

## ✅ Strengths of the Plan

Despite criticisms, the plan has genuine strengths:

### Strength 1: Clear Phase Breakdown

The 5-phase structure is well-organized and logical:
- Phase 0: Design
- Phase 1: Abstraction
- Phase 2: Axum
- Phase 3: Starlette
- Phase 4: FastAPI
- Phase 5: Testing

Each phase has clear deliverables and success criteria.

---

### Strength 2: Detailed Test Coverage

The plan includes specific test cases for:
- GraphQL query execution
- APQ caching
- Error formatting
- Middleware execution
- Context building
- WebSocket subscriptions

This is far better than "we'll test it later."

---

### Strength 3: Migration Path for FastAPI

The plan acknowledges FastAPI needs to be deprecated and provides:
- Clear deprecation timeline
- Migration guides
- Thin wrapper approach (not rewrite)

This is responsible deprecation planning.

---

### Strength 4: Developer Workflow Examples

The plan shows:
- How to add new features (implement once in abstraction)
- How to add new servers (implement protocol, inherit features)
- Clear boundaries between framework-specific and shared code

This makes future maintenance easier.

---

### Strength 5: Comprehensive Documentation

The plan includes documentation for:
- Architecture overview
- Server selection guide
- Server-specific setup
- Migration guides
- Performance comparisons

This is important for user adoption.

---

## 🎯 Key Recommendations

### Recommendation 1: Invert the Approach

**Instead of**: Abstract first, then implement servers

**Do**:
1. Build Axum server completely (no abstraction)
2. Make it production-ready
3. Deploy it to real users
4. THEN extract abstraction based on actual learnings
5. Then add Starlette

**Benefit**: Abstraction will be driven by real constraints, not theory

**Timeline impact**: +2-3 weeks (but better result)

---

### Recommendation 2: Separate Concerns

**Instead of**: One `HttpServer` protocol for everything

**Do**:
- `RequestParser` protocol
- `Middleware` protocol
- `ResponseFormatter` protocol
- `SubscriptionHandler` protocol
- One per concern, loose coupling

**Benefit**: Easier to swap parts, easier to test individually

---

### Recommendation 3: Define Parity Carefully

**Instead of**: "All servers produce identical results"

**Do**: Define clear parity criteria:
- ✅ Valid queries: Identical results
- ✅ Auth/permission: Identical behavior
- ✅ APQ caching: Identical responses
- ❌ Error messages: Framework may differ
- ❌ HTTP headers: Framework may differ
- ❌ Performance: Framework may differ

**Benefit**: Tests won't fail on things you don't control

---

### Recommendation 4: Phase Axum → Starlette Sequentially

**Instead of**: Week 4-5 Axum, Week 6 Starlette (parallel implied)

**Do**:
- Weeks 4-5: Axum complete
- Week 6: Axum production-ready, documented
- Week 7-8: Starlette
- Week 9-10: Parity testing

**Benefit**: Validate one before building the next

---

### Recommendation 5: Realistic Performance Claims

**Instead of**: "7-10x faster due to Rust"

**Do**:
- Benchmark actual workloads
- Document where time is spent
- Show realistic speedups (2-3x for HTTP layer)
- Acknowledge that database is 90% of time
- Position Axum as "future-proof" not "faster"

**Benefit**: Users have correct expectations

---

### Recommendation 6: Plan for Maintenance Mode

**Instead of**: "FastAPI removed in v3.0"

**Do**:
- v2.0: Recommend Axum/Starlette
- v2.1-v3.9: Maintenance mode (bug fixes only, no features)
- v4.0: Removed

**Benefit**: Gives users 2+ years to migrate, reduces support burden

---

### Recommendation 7: Add "Real-World Validation" Phase

**Add to plan**: Phase 6 (Week 11-12)

```
Phase 6: Real-World Testing
- Run against actual customer workloads
- Test with multi-tenant databases
- Test with subscriptions at scale
- Load test each server
- Validate parity with real data
- Document compatibility matrix
```

**Benefit**: Find real issues before v2.0 release

---

## 🏁 Conclusion

**Overall Assessment**: 85/100

**Verdict**: The architecture is **good strategy but needs refinement before implementation**

**Key Issues to Fix**:
1. ⚠️ CRITICAL: Define abstraction boundaries better
2. ⚠️ CRITICAL: Address protocol differences explicitly
3. ⚠️ HIGH: Invert implementation order (build Axum first)
4. ⚠️ HIGH: Separate concerns (don't bundle all in one protocol)
5. ⚠️ MEDIUM: Realistic timeline and performance claims
6. ⚠️ MEDIUM: Missing pieces (error handling, config, logging)

**Recommendation**:

Do NOT start implementation immediately. Instead:

1. **Week 1**: Create detailed "Axum Implementation Spec"
   - What exactly is Axum's scope?
   - How does it interact with Python?
   - Database connection management?
   - Configuration synchronization?

2. **Week 2**: Build Axum server (focused, no abstraction)
   - Single server implementation
   - Full test coverage
   - Production-ready

3. **Week 3**: Evaluate what worked, what didn't
   - What was hard to abstract?
   - What differences emerged?
   - How should Starlette differ?

4. **Then**: Extract abstraction based on learnings

This approach reduces risk of building the wrong abstraction.

---

**Document Created**: January 5, 2026
**Critical Issues Found**: 7
**High-Risk Decisions**: 3
**Missing Pieces**: 5
**Recommendations**: 7
**Strengths Identified**: 5

**Next Step**: Address critical issues and create Axum Implementation Spec before proceeding.
