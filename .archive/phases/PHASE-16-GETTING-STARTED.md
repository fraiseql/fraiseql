# Phase 16: Getting Started Checklist

**Status**: Ready to begin implementation
**Duration**: 3-5 days (8 commits)
**Framework**: Axum + Tokio + PyO3

---

## 📖 Pre-Implementation Reading (30 minutes)

- [ ] Read: `.phases/PHASE-16-README.md` - Overview of all documents
- [ ] Read: `.phases/PHASE-16-AXUM-DECISION.md` - Why we chose Axum
- [ ] Skim: `.phases/phase-16-axum-http-server.md` - Main implementation plan
- [ ] Bookmark: `.phases/phase-16-axum-quick-start.md` - You'll use this constantly

---

## 🚀 Before You Start Coding

### Setup & Knowledge
- [ ] Create feature branch: `git checkout -b feature/phase-16-axum-http-server`
- [ ] Review Axum documentation: https://docs.rs/axum/latest/axum/
- [ ] Review Parviocula pattern: https://github.com/tristan/parviocula
- [ ] Understand PyO3 async patterns from Phase 15b code
- [ ] Review current Cargo.toml to understand dependency structure

### Environment Check
- [ ] Rust toolchain is latest (`rustup update`)
- [ ] Cargo works: `cargo --version`
- [ ] Git is on feature branch: `git branch`
- [ ] Code editor/IDE is ready

### Knowledge Prerequisites
- [ ] Understand basic Axum concepts (routing, handlers, extractors)
- [ ] Familiar with Tokio async/await
- [ ] Know how PyO3 FFI works (from Phase 15b)
- [ ] Understand our GraphQL pipeline from Phases 1-15

---

## 📋 Implementation Checklist

### Commit 1: Cargo.toml & Module Setup (1 hour)

Dependencies to add:
- [ ] `axum = "0.7"`
- [ ] `tower = "0.4"`
- [ ] `tower-http` with features: `cors`, `compression`, `trace`
- [ ] `hyper = "1.1"`
- [ ] `futures = "0.3"`

Module structure:
- [ ] Create `fraiseql_rs/src/http/mod.rs`
- [ ] Create `fraiseql_rs/src/http/axum_server.rs`
- [ ] Add `pub mod http;` to `fraiseql_rs/src/lib.rs`
- [ ] Verify: `cargo check --lib` passes

Tests:
- [ ] Module exports work
- [ ] Basic compilation works
- [ ] No clippy warnings

**Git**: `git add fraiseql_rs/Cargo.toml fraiseql_rs/src/lib.rs fraiseql_rs/src/http/`

---

### Commit 2: Axum Server & GraphQL Handler (1-2 hours)

Core implementation:
- [ ] Create Axum Router with type-safe routes
- [ ] Implement POST `/graphql` handler
- [ ] Implement JSON extraction (auto via serde)
- [ ] Integrate with GraphQL pipeline
- [ ] Return JSON response
- [ ] Handle extraction errors properly

Key code patterns:
- [ ] `Router::new().route("/graphql", post(handler))`
- [ ] `async fn handler(Json(req): Json<GraphQLRequest>)`
- [ ] `State<Arc<GraphQLPipeline>>`

Tests:
- [ ] Server creation works
- [ ] GraphQL query returns response
- [ ] Response format is correct
- [ ] Error handling works

**Git**: `git add fraiseql_rs/src/http/axum_server.rs`

---

### Commit 3: WebSocket & Subscriptions (1-2 hours)

WebSocket handler:
- [ ] Add GET `/graphql/subscriptions` route
- [ ] Implement WebSocket upgrade handler
- [ ] Reuse Phase 15b subscription logic
- [ ] Handle WebSocket frames
- [ ] Send subscription updates

Integration:
- [ ] Verify Phase 15b subscription code exists
- [ ] Adapt it to WebSocket frames
- [ ] Message serialization/deserialization
- [ ] Connection cleanup on disconnect

Tests:
- [ ] WebSocket upgrade works
- [ ] Subscription messages flow correctly
- [ ] Connections close cleanly

**Git**: `git add fraiseql_rs/src/http/websocket.rs`

---

### Commit 4: Middleware & Error Handling (1-2 hours)

Middleware:
- [ ] Add `CompressionLayer` (gzip)
- [ ] Add `CorsLayer` (permissive for now)
- [ ] Add custom error handler
- [ ] Implement request logging

Error handling:
- [ ] Create error types
- [ ] Implement `IntoResponse` for errors
- [ ] Format GraphQL errors correctly
- [ ] Return proper HTTP status codes

Tests:
- [ ] Compression works
- [ ] CORS headers present
- [ ] Errors formatted correctly
- [ ] Middleware applied in right order

**Git**: `git add fraiseql_rs/src/http/middleware.rs fraiseql_rs/src/http/errors.rs`

---

### Commit 5: Validation & Rate Limiting (1 hour)

Validation:
- [ ] Validate GraphQL request structure
- [ ] Check query is not empty
- [ ] Validate variable types
- [ ] Check operation name if present

Rate limiting:
- [ ] Add governor crate (already in Cargo.toml)
- [ ] Create rate limiter in app state
- [ ] Extract client IP from request
- [ ] Check rate limit before executing

Tests:
- [ ] Invalid requests rejected
- [ ] Rate limit enforced
- [ ] Proper error messages

**Git**: `git add fraiseql_rs/src/http/validation.rs fraiseql_rs/src/http/rate_limit.rs`

---

### Commit 6: Monitoring & Metrics (1-2 hours)

Metrics collection:
- [ ] Track active connections (Arc<AtomicUsize>)
- [ ] Count total requests
- [ ] Count errors
- [ ] Measure latency (histogram)
- [ ] Track cache hits

Expose metrics:
- [ ] Add `/metrics` endpoint (optional)
- [ ] Return Prometheus-compatible format
- [ ] Or just track internally for monitoring

Tests:
- [ ] Metrics recorded correctly
- [ ] Connection count accurate
- [ ] Latency histogram works

**Git**: `git add fraiseql_rs/src/http/metrics.rs fraiseql_rs/src/http/connection.rs`

---

### Commit 7: Python Bridge & PyO3 (2-3 hours)

Python module structure:
- [ ] Create `src/fraiseql/http/` directory
- [ ] Create `src/fraiseql/http/__init__.py`
- [ ] Create `src/fraiseql/http/config.py`
- [ ] Create `src/fraiseql/http/server.py`

PyO3 bindings:
- [ ] Create `fraiseql_rs/src/http/py_bindings.rs`
- [ ] Implement `PyAxumServer` class
- [ ] Implement `new()` method (create server)
- [ ] Implement `start()` method (async wrapper)
- [ ] Implement `shutdown()` method
- [ ] Implement `active_connections()` method
- [ ] Add to module exports

Python API:
- [ ] `create_rust_http_app()` factory function
- [ ] `RustHttpConfig` class
- [ ] `RustHttpServer` wrapper
- [ ] 100% compatible with original API

Tests:
- [ ] Python module imports
- [ ] Server creates successfully
- [ ] Configuration applies correctly
- [ ] Async start/shutdown works

**Git**: `git add src/fraiseql/http/ fraiseql_rs/src/http/py_bindings.rs`

---

### Commit 8: Tests & Documentation (2-3 hours)

Unit Tests (Rust):
- [ ] `tests/unit/http/test_server.rs`
- [ ] Server initialization
- [ ] Route handling
- [ ] WebSocket upgrade
- [ ] Error responses
- [ ] Middleware application
- [ ] Metrics collection

Integration Tests (Python):
- [ ] `tests/integration/http/test_server.py`
- [ ] Server starts from Python
- [ ] GraphQL query works
- [ ] WebSocket subscriptions work
- [ ] Error responses format
- [ ] Configuration works

Performance Tests:
- [ ] Latency benchmarks
- [ ] Startup time
- [ ] Memory usage
- [ ] Concurrent connections

Documentation:
- [ ] `docs/PHASE-16-AXUM.md` - Architecture
- [ ] Migration guide from FastAPI
- [ ] Configuration options
- [ ] Performance comparison
- [ ] Troubleshooting guide

**Git**: `git add tests/ docs/`

---

## ✅ Final Checklist

Before calling Phase 16 complete:

### Code Quality
- [ ] `cargo check --lib` - No errors
- [ ] `cargo clippy` - Zero warnings
- [ ] `cargo fmt` - Properly formatted
- [ ] `cargo test` - All tests pass

### Tests
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Performance tests meet targets
- [ ] No regressions in existing tests

### Documentation
- [ ] Code comments on complex logic
- [ ] Docstrings on public APIs
- [ ] README for HTTP module
- [ ] Examples in docstrings

### Performance
- [ ] Response time <5ms (cached)
- [ ] Startup time <100ms
- [ ] Memory usage <50MB
- [ ] Handle 10,000+ connections

### Compatibility
- [ ] Python API unchanged
- [ ] No user code changes needed
- [ ] Existing tests still pass
- [ ] Can switch back to FastAPI

---

## 🎯 Daily Progress Tracking

### Day 1
- [ ] Commit 1: Cargo.toml & setup (1 hour)
- [ ] Commit 2: Axum server & handler (1-2 hours)
- [ ] Commit 3: WebSocket & subscriptions (1-2 hours)
- [ ] Commit 4: Middleware & error handling (1-2 hours)
- **Expected**: Basic server working with GraphQL queries

### Day 2
- [ ] Commit 5: Validation & rate limiting (1 hour)
- [ ] Commit 6: Monitoring & metrics (1-2 hours)
- [ ] Start: Commit 7 Python bridge (1-2 hours)
- **Expected**: Full request handling with features

### Day 3
- [ ] Finish: Commit 7 Python bridge (1-2 hours)
- [ ] Commit 8: Tests & documentation (2-3 hours)
- **Expected**: Full test suite passing, production ready

---

## 📚 Reference During Implementation

Keep these handy:
- `.phases/phase-16-axum-quick-start.md` - Code patterns
- `https://docs.rs/axum/latest/axum/` - Axum docs
- `https://github.com/tokio-rs/axum/tree/main/examples` - Axum examples
- `.phases/phase-16-axum-http-server.md` - Detailed plan

---

## 🆘 When You Get Stuck

Common issues and solutions:

**"Can't import axum"**
- Run: `cargo fetch`
- Verify Cargo.toml syntax
- Run: `cargo check --lib`

**"Type mismatch in handler"**
- Review Axum handler signature examples in quick-start
- Ensure extractors are in right order
- Check Json<T> vs State<T> usage

**"PyO3 compilation errors"**
- Check pyo3-asyncio is configured correctly
- Verify pyo3 version matches lib.rs imports
- Review Phase 15b code for patterns

**"WebSocket not upgrading"**
- Verify route is GET not POST
- Check websocket crate features enabled
- Test with simple echo handler first

---

## ✨ Success Definition

Phase 16 is complete when:

1. ✅ All 8 commits implemented
2. ✅ All tests passing
3. ✅ No clippy warnings
4. ✅ Performance targets met
5. ✅ Python API unchanged
6. ✅ Documentation complete
7. ✅ Code reviewed and approved

---

## 🚀 Next Steps After Phase 16

Once complete:
1. Merge to `dev` branch
2. Version bump to v2.0.0
3. Tag release
4. Plan Phase 17 (HTTP/2 optimizations)
5. Plan Phase 18+ (advanced features)

---

**Version**: 1.0
**Created**: January 3, 2026
**Status**: Ready to implement
**Estimated Duration**: 3-5 days
**Next Action**: Start Commit 1!
