# Option 2: Phase 0 + Phase 1 Only - Detailed Breakdown

**Decision**: Build Axum HTTP server only (no Starlette, no FastAPI compatibility)
**Effort**: 6-7 weeks total
**Effort Breakdown**: 10 days (Phase 0) + 25 days (Phase 1) = 35 days
**Risk Level**: Medium (Rust learning curve, but smaller scope than full initiative)

---

## ✅ Already Complete

These components are fully functional and production-ready:

### Phase 3: FraiseQL Axum Wrapper (Python Configuration)
- ✅ CORSConfig with 5 factory methods
- ✅ AxumMiddleware base class with 4 built-in implementations
- ✅ PlaygroundConfig for GraphQL Playground UI
- ✅ OpenAPIConfig for Swagger/ReDoc documentation
- ✅ AxumFraiseQLConfig with advanced configuration (Phase 3E)
- ✅ 217 tests passing, 64+ examples, zero regressions
- ✅ Production-ready

**These components are ready to integrate with the Rust HTTP server.**

---

## ⏳ What Remains: Phase 0 (Pre-Implementation Specification)

**Duration**: 2 weeks (10 days effort)
**Purpose**: Define specifications before building the Rust HTTP server
**Status**: NOT STARTED

### Phase 0.1: Axum Implementation Specification (3-4 days)

**Goal**: Define exact boundary between Python and Rust

**Deliverable**: `docs/architecture/AXUM-IMPLEMENTATION-SPEC.md`

**Tasks**:
1. **Define HTTP Layer Scope** (1 day)
   - What lives in Rust:
     - HTTP routing (GET /graphql, POST /graphql, WebSocket /graphql/ws)
     - Request parsing (JSON body, query parameters, headers)
     - Response building (JSON serialization, status codes, headers)
     - Middleware integration points
     - Error handling (convert Rust errors → GraphQL errors)
   - What stays in Python:
     - Business logic execution
     - GraphQL query processing
     - Configuration management
     - Logging and monitoring

2. **Define Communication Patterns** (1 day)
   - How Python calls Rust for HTTP handling
   - How Rust calls Python for business logic
   - PyO3 bindings design
   - Error propagation across language boundary
   - Performance implications

3. **Define Integration Points** (1-2 days)
   - Configuration synchronization
   - Authentication context passing
   - Request ID tracking
   - Logging coordination
   - Metrics collection
   - Graceful shutdown

**Acceptance Criteria**:
- [ ] Clear specification of what lives in Rust vs Python
- [ ] Communication patterns documented with examples
- [ ] PyO3 binding strategy defined
- [ ] Performance expectations outlined
- [ ] Agreed upon by team

---

### Phase 0.2: Database Connection Architecture (2-3 days)

**Goal**: Design connection pool ownership and lifecycle

**Deliverable**: `docs/architecture/DATABASE-CONNECTION-ARCH.md`

**Tasks**:
1. **Design Connection Pool Model** (1 day)
   - Option A: Python owns pool, Rust borrows connections
     - Simpler implementation
     - Potential bottleneck (Python holds pool)
     - Easier resource management
   - Option B: Rust owns pool, Python requests connections
     - Better performance
     - More complex error handling
     - Better isolation

   **Recommendation**: Option A (simpler, adequate for most use cases)

2. **Define Ownership Model** (1 day)
   - Lifetime management
   - How Rust requests connections from Python
   - How Python reclaims connections
   - Connection state tracking
   - Async handling across boundary

3. **Design Graceful Shutdown** (1 day)
   - How Rust signals shutdown to Python
   - How Python drains connection pool
   - Timeout handling
   - Resource cleanup

**Acceptance Criteria**:
- [ ] Clear ownership model chosen and documented
- [ ] Connection request/release protocol defined
- [ ] Error cases for exhausted pool handled
- [ ] Shutdown sequence specified
- [ ] Performance implications analyzed

---

### Phase 0.3: Abstraction Layer Design (2-3 days)

**Goal**: Design abstraction for multi-server support (even though Phase 2 not starting)

**Deliverable**: `docs/architecture/PROTOCOL-ABSTRACTION.md`

**Tasks**:
1. **Identify Protocol Abstraction** (1 day)
   - What both Axum and Starlette (future) need:
     - HTTP request handling
     - Response building
     - WebSocket support
     - Error formatting
     - Middleware integration
   - Server-specific vs server-agnostic code

2. **Design Trait System** (1 day)
   - `HttpServer` trait for basic operation
   - `RequestHandler` trait for request processing
   - `ResponseBuilder` trait for response creation
   - `WebSocketHandler` trait for subscriptions

3. **Module Structure** (1 day)
   - `fraiseql_rs/src/http/protocol.rs` - Shared protocol
   - `fraiseql_rs/src/http/axum/` - Axum-specific
   - Clear interface boundaries
   - Minimal coupling between modules

**Acceptance Criteria**:
- [ ] Protocol abstraction defined (even if only Axum uses it)
- [ ] Traits clearly specified
- [ ] Module boundaries clear
- [ ] Future Starlette implementation can use same abstractions
- [ ] No assumptions made about specific servers

---

### Phase 0.4: Realistic Timeline & Dependencies (1-2 days)

**Goal**: Create detailed implementation timeline

**Deliverable**: `docs/architecture/TIMELINE-DEPENDENCIES.md`

**Tasks**:
1. **Phase 1 Timeline Breakdown** (1 day)
   - Week 1: Basic server, request parsing, error handling (5 days)
   - Week 2: Continue foundation, health checks, validation (5 days)
   - Week 3: GraphQL execution, query/mutation (5 days)
   - Week 4: Continue execution, subscriptions, auth context (5 days)
   - Week 5: Polish, performance, testing, benchmarks (5 days)

2. **Resource Allocation** (0.5 days)
   - Minimum: 1 developer
   - Optimal: 1 developer (or 2 with reduced timeline)
   - Skill requirements
   - Learning curve accommodation

3. **Risk Mitigation** (0.5 days)
   - Async/await complexity → Break into smaller pieces
   - PyO3 complexity → Document patterns, write examples
   - WebSocket complexity → Start with basic, extend incrementally
   - Performance targets → Benchmark frequently

**Acceptance Criteria**:
- [ ] Week-by-week breakdown detailed
- [ ] Dependencies between tasks identified
- [ ] Buffer time allocated
- [ ] Risk mitigation strategies listed
- [ ] Resource requirements clear

---

## ⏳ What Remains: Phase 1 (Axum Server Implementation)

**Duration**: 4-5 weeks (25 days effort)
**Purpose**: Build fully functional Axum HTTP server with feature parity to FastAPI
**Status**: NOT STARTED

### Phase 1 - Week 1-2: Foundation & Request Handling (10 days)

**Goal**: Basic Axum server with request parsing and error handling

**Files to Create**:
- `fraiseql_rs/src/http/mod.rs` - Main server module
- `fraiseql_rs/src/http/request.rs` - Request parsing logic
- `fraiseql_rs/src/http/response.rs` - Response building
- `fraiseql_rs/src/http/error.rs` - Error handling
- `fraiseql_rs/src/http/middleware.rs` - Middleware integration
- `fraiseql_rs/src/http/health.rs` - Health check endpoint

**Detailed Tasks**:

#### Week 1, Day 1-2: Basic Server Setup (2 days)
1. Create Axum application with routing
   - `POST /graphql` for queries/mutations
   - `GET /graphql/playground` for UI
   - `GET /health` for health checks
   - `GET /metrics` for monitoring

2. Integrate with FraiseQL Rust pipeline
   - Link fraiseql_rs dependencies
   - Expose Python binding entry points
   - Set up logging

**Tests Required**:
- Server startup and shutdown
- Port binding verification
- Basic route access

#### Week 1, Day 3-5: Request Parsing (3 days)
1. Parse JSON request bodies
   ```rust
   pub struct GraphQLRequest {
       query: String,
       variables: Option<serde_json::Value>,
       operation_name: Option<String>,
   }
   ```

2. Parse query parameters
   - Support query in URL query string
   - Support variables as JSON

3. Parse headers
   - Content-Type validation
   - User authentication headers
   - Request ID headers

4. Validate request structure
   - Query must be non-empty string
   - Variables must be valid JSON object
   - Operation name must be valid identifier

**Tests Required**:
- Valid query request parsing
- Valid mutation request parsing
- Query string parameters
- Invalid JSON rejection
- Missing fields handling

#### Week 2, Day 1-3: Error Handling (3 days)
1. Convert Rust errors to GraphQL errors
   ```rust
   pub struct GraphQLError {
       message: String,
       extensions: Option<ErrorExtensions>,
   }
   ```

2. Define error categories
   - Request parsing errors → 400 Bad Request
   - GraphQL errors → 200 OK with errors array
   - Server errors → 500 Internal Server Error
   - Timeout errors → 504 Gateway Timeout

3. Implement error response formatting
   - Consistent error structure
   - Detailed messages in development
   - Generic messages in production

**Tests Required**:
- Parse error handling
- Validation error formatting
- Internal error masking
- Proper HTTP status codes

#### Week 2, Day 4-5: Health & Validation (2 days)
1. Health check endpoint
   - Database connectivity check
   - Simple "alive" response
   - Structured health output

2. Request validation
   - Maximum query depth (default: 10)
   - Maximum query complexity
   - Rate limiting preparation (not implementation yet)

**Tests Required**:
- Health check responds correctly
- Valid queries pass validation
- Deep queries rejected
- Large queries rejected

**Week 1-2 Acceptance Criteria**:
- [ ] Axum server starts and stops gracefully
- [ ] Request parsing handles valid GraphQL queries
- [ ] Request parsing handles valid GraphQL mutations
- [ ] Query parameters supported
- [ ] Headers parsed correctly
- [ ] Invalid requests rejected with proper errors
- [ ] Health check endpoint works
- [ ] Basic validation in place
- [ ] Server integrates with FraiseQL pipeline
- [ ] All tests passing for request handling

---

### Phase 1 - Week 3-4: GraphQL Execution (10 days)

**Goal**: Execute GraphQL queries and mutations through Rust

**Files to Create/Modify**:
- `fraiseql_rs/src/http/handler.rs` - GraphQL handler
- `fraiseql_rs/src/http/context.rs` - Request context
- `fraiseql_rs/src/http/subscription.rs` - WebSocket subscriptions
- Integration with `fraiseql_rs/src/pipeline/` modules

**Detailed Tasks**:

#### Week 3, Day 1-3: Query Execution (3 days)
1. Build GraphQL execution context
   ```rust
   pub struct GraphQLContext {
       request_id: String,
       user_id: Option<String>,
       headers: HashMap<String, String>,
   }
   ```

2. Call Rust pipeline with request
   ```rust
   let result = fraiseql_rs::execute_query(
       &pool,
       &context,
       &request.query,
       request.variables,
   ).await;
   ```

3. Format query results
   - Convert Rust response to JSON
   - Include errors if present
   - Include extensions (timing, etc.)

4. Logging
   - Log query with request ID
   - Log execution time
   - Log errors encountered

**Tests Required**:
- Simple query execution returns results
- Query with variables works
- Query with authentication context
- Query error handling
- Execution time logging

#### Week 3, Day 4-5 & Week 4, Day 1-2: Mutation Handling (3 days)
1. Distinguish mutations from queries
   - Parse operation type
   - Route to mutation handler
   - Enforce single root mutation

2. Handle mutation side effects
   - Track mutations for logging
   - Maintain transaction semantics
   - Return mutation result

3. Error handling for mutations
   - Database constraint violations
   - Authorization failures
   - Data validation errors

**Tests Required**:
- Simple mutation execution
- Mutation with side effects
- Mutation error handling
- Mutation with variables
- Result data returned correctly

#### Week 4, Day 3-4: Subscription Protocol (2 days)
1. WebSocket setup
   - Upgrade HTTP connection to WebSocket
   - Maintain WebSocket connection state
   - Handle client messages

2. GraphQL-transport-ws protocol
   - Subscribe message handling
   - Data message sending
   - Complete message
   - Error message protocol

3. Subscription lifecycle
   - Connection initialization
   - Subscription setup
   - Data streaming
   - Connection close

**Tests Required**:
- WebSocket connection upgrade
- Subscribe message handling
- Data streaming
- Connection close handling
- Error during streaming

#### Week 4, Day 5: Authentication Context (1 day)
1. Extract auth from headers
   - Bearer token parsing
   - JWT token validation (if enabled)
   - User ID extraction

2. Build authenticated context
   - Attach user to GraphQL context
   - Make available to resolvers
   - Log authenticated user

**Tests Required**:
- Bearer token extraction
- Token validation
- Unauthenticated requests
- Invalid tokens rejected

**Week 3-4 Acceptance Criteria**:
- [ ] Queries execute through Rust pipeline
- [ ] Query results returned in GraphQL format
- [ ] Mutations execute successfully
- [ ] Mutations return results
- [ ] Errors formatted as GraphQL errors
- [ ] WebSocket subscriptions work (basic)
- [ ] Authentication context built
- [ ] Authentication context passed to resolvers
- [ ] Request logging with IDs
- [ ] All integration tests passing

---

### Phase 1 - Week 5: Polish & Testing (5 days)

**Goal**: Optimize, test comprehensively, benchmark

**Detailed Tasks**:

#### Day 1: Performance Optimization (1 day)
1. Profile request handling
   - Identify bottlenecks
   - Optimize hot paths
   - Reduce allocations

2. Connection pooling efficiency
   - Verify pool is being used
   - Monitor pool utilization
   - Check for connection leaks

3. Response building efficiency
   - JSON serialization performance
   - String allocation minimization

**Tests Required**:
- Profiling shows expected hot paths
- No memory leaks
- Connection pool properly utilized

#### Day 2: Memory Profiling (1 day)
1. Memory usage analysis
   - Measure per-request memory
   - Check for leaks
   - Verify cleanup on connection close

2. Stress test memory
   - Run many concurrent requests
   - Monitor memory growth
   - Verify cleanup

**Tests Required**:
- Memory usage stable over time
- No leaks under stress
- Acceptable per-request overhead

#### Day 3: Full Test Coverage (1 day)
1. Unit test suite
   - All error paths covered
   - All request types covered
   - Edge cases included

2. Integration tests
   - Full request → response cycle
   - Multiple concurrent requests
   - Error conditions

**Tests Required**:
- Coverage > 90% of code
- All error paths tested
- All features exercised

#### Day 4-5: Benchmarks & Documentation (2 days)
1. Benchmark against FastAPI
   - Request throughput
   - Query latency (p50, p95, p99)
   - Concurrent connection handling
   - Memory usage comparison

2. Document Rust module
   - Architecture overview
   - Module responsibilities
   - API documentation
   - Performance notes

**Tests Required**:
- Benchmarks complete
- Axum matches or exceeds FastAPI performance
- Documentation comprehensive

**Week 5 Acceptance Criteria**:
- [ ] Performance profiling complete
- [ ] No memory leaks detected
- [ ] Test coverage > 90%
- [ ] Benchmark results documented
- [ ] Axum performance verified
- [ ] Full documentation written
- [ ] Ready for production deployment

---

## Phase 1 Summary

**Total Effort**: 25 days (5 weeks)

### Deliverables:
- ✅ Fully functional Axum HTTP server
- ✅ Request parsing and validation
- ✅ GraphQL query/mutation execution
- ✅ WebSocket subscription support
- ✅ Error handling and formatting
- ✅ Authentication context
- ✅ Request logging
- ✅ Health check endpoint
- ✅ Comprehensive test suite (100+ tests)
- ✅ Performance benchmarks
- ✅ Complete documentation

### Quality Metrics:
- Test coverage > 90%
- All 100+ tests passing
- Zero regressions with Python wrapper
- Performance: >= FastAPI
- Production-ready code

### Git Artifacts:
- Well-organized commits (1 per day ideally)
- Clear commit messages
- All changes documented
- Ready for release

---

## Timeline Summary

| Phase | Duration | Effort | Status | Complete By |
|-------|----------|--------|--------|-------------|
| Phase 0 | 2 weeks | 10 days | NOT STARTED | Week 2 |
| Phase 1 | 5 weeks | 25 days | NOT STARTED | Week 7 |
| **TOTAL** | **6-7 weeks** | **35 days** | **PENDING** | **End of Week 7** |

---

## What You Get After Option 2

### Immediately Usable
- ✅ Phase 3 Python wrapper (already complete)
- ✅ Full GraphQL configuration system
- ✅ CORS, Middleware, Playground, OpenAPI, Advanced Config
- ✅ Production-ready Python HTTP wrapper

### After Phase 0 (Week 2)
- ✅ Clear specifications for Rust implementation
- ✅ Database architecture designed
- ✅ Abstract protocol defined (can support Starlette later if needed)
- ✅ Detailed timeline and risk analysis

### After Phase 1 (Week 7)
- ✅ Fully functional Axum HTTP server
- ✅ Performance improvements over FastAPI (7-10x faster Rust pipeline)
- ✅ Production-ready system
- ✅ Can evaluate and decide on Starlette/FastAPI layers later
- ✅ 100+ tests passing
- ✅ Complete documentation

---

## Risk Mitigation for Option 2

### High Risks
| Risk | Mitigation |
|------|-----------|
| Rust async/await complexity | Break into small pieces, write examples, do code reviews |
| PyO3 integration issues | Document patterns early, test integration frequently |
| WebSocket protocol complexity | Start basic, extend incrementally, reference reference implementation |
| Performance targets | Benchmark early and often, profile continuously |

### Medium Risks
| Risk | Mitigation |
|------|-----------|
| Rust learning curve | Allocate time, pair programming, reference existing code |
| Team unfamiliarity | Document decisions, maintain architecture doc |
| Integration testing | Write integration tests early, automate testing |

### Low Risks
| Risk | Mitigation |
|------|-----------|
| Basic HTTP handling | Proven Axum patterns exist, use examples |
| Configuration parsing | Already done in Phase 3, reuse patterns |
| Documentation | Clear requirements, many Axum examples |

---

## When to Choose Option 2

**Choose Option 2 if:**
- ✅ You need Axum HTTP server working quickly
- ✅ You want to evaluate before committing to full initiative
- ✅ You have limited Rust expertise (smaller scope to learn)
- ✅ You need production deployment in 6-7 weeks
- ✅ You're uncertain about Starlette requirements

**Do NOT choose Option 2 if:**
- ❌ You need Starlette alternative immediately
- ❌ You need FastAPI compatibility layer
- ❌ You need full pluggable architecture
- ❌ You can't commit developers for 6-7 weeks

---

## Next Steps After Decision

If you choose Option 2:

1. **Immediate (Today)**
   - [ ] Approve specifications approach
   - [ ] Assign developer(s) to Phase 0
   - [ ] Review Phase 0 deliverables (spec docs)

2. **After Phase 0 (Week 2-3)**
   - [ ] Review specifications with team
   - [ ] Approve Rust implementation approach
   - [ ] Assign developer(s) to Phase 1
   - [ ] Set up Rust development environment

3. **Weekly During Phase 1**
   - [ ] Verify test coverage
   - [ ] Review benchmarks
   - [ ] Check for blockers
   - [ ] Adjust timeline if needed

4. **After Phase 1 (Week 7)**
   - [ ] Integration testing complete
   - [ ] Performance verified
   - [ ] Documentation complete
   - [ ] Ready for production release

---

**Document Version**: 1.0
**Last Updated**: 2026-01-05
**Status**: Ready for Option 2 Implementation
