# Phase 16: Quick Reference Guide

**Implementation of Native Rust HTTP Server for FraiseQL**

---

## 🎯 One-Line Summary

Replace FastAPI/uvicorn with native Rust HTTP server → 1.5-3x faster response times, unchanged Python API.

---

## 📊 What Changes

### For Users
```python
# BEFORE: using FastAPI
from fraiseql import create_fraiseql_app
app = create_fraiseql_app(schema)
# Run: uvicorn app:app

# AFTER: using Rust HTTP (same API)
from fraiseql.http import create_rust_http_app
app = create_rust_http_app(schema)
# Run: python -c "asyncio.run(app.start())"

# ← Same behavior, faster performance
```

### For Developers
```
Before: Python HTTP (FastAPI) → Rust GraphQL Pipeline
After:  Rust HTTP → Rust GraphQL Pipeline (no Python in request path)
```

---

## 📈 Performance Impact

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Response Time | 12-22ms | 7-12ms | 1.5-3x faster |
| HTTP Overhead | 5-10ms | <1ms | 10x reduction |
| Memory Idle | 100-150MB | <50MB | 50% savings |
| Concurrency | 1,000 req/s | 5,000+ req/s | 5x better |
| Startup | 100-200ms | <50ms | 2-4x faster |

---

## 🏗️ File Structure (15 commits)

### Commit 1-3: HTTP Server Core
```
fraiseql_rs/src/http/
├── server.rs      # Tokio HTTP listener
├── request.rs     # Parse HTTP request
└── routing.rs     # Route requests to /graphql
```

### Commit 4-6: Response Handling
```
fraiseql_rs/src/http/
├── graphql_handler.rs   # Execute GraphQL
├── response.rs          # Serialize HTTP response
└── error_handler.rs     # Format errors
```

### Commit 7-9: WebSocket & Connections
```
fraiseql_rs/src/http/
├── websocket.rs      # WebSocket upgrade
├── connection.rs     # Connection limits
└── mod.rs            # Module exports
```

### Commit 10-13: Python Bridge
```
src/fraiseql/http/
├── __init__.py      # Module exports
├── config.py        # RustHttpConfig
├── server.py        # RustHttpServer wrapper
└── (py_bindings in Rust)
```

### Commit 14-15: Tests & Docs
```
tests/
├── unit/http/       # Rust unit tests
├── integration/http/ # Python integration tests
└── performance/     # Benchmarks

docs/
└── PHASE-16-HTTP-SERVER.md
```

---

## 🔧 Key Rust Components

### HttpServer
```rust
pub struct HttpServer {
    config: HttpServerConfig,
    listener: Option<TcpListener>,
}

impl HttpServer {
    pub async fn start(&mut self) -> Result<()>;
    pub async fn shutdown(&mut self);
}
```

### GraphQLRequest
```rust
pub struct GraphQLRequest {
    pub query: String,
    pub variables: Option<Value>,
    pub operation_name: Option<String>,
}
```

### HttpResponse
```rust
pub struct HttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn to_bytes(&self) -> Vec<u8>;
}
```

---

## 🐍 Key Python Components

### RustHttpConfig
```python
@dataclass
class RustHttpConfig:
    host: str = "0.0.0.0"
    port: int = 8000
    max_connections: int = 10000
    request_timeout_ms: int = 30000
    workers: Optional[int] = None
    enable_compression: bool = True
    enable_http2: bool = True
```

### RustHttpServer
```python
class RustHttpServer:
    async def start() -> None
    async def shutdown() -> None
    @property is_running -> bool
    @property active_connections -> int
```

### Factory Function
```python
def create_rust_http_app(
    schema: GraphQLSchema,
    config: Optional[RustHttpConfig] = None,
    auth_provider: Any = None,
    db_pool: Any = None,
) -> RustHttpServer
```

---

## 📋 Testing Checklist

### Unit Tests (Rust)
- [ ] Server starts without errors
- [ ] Request parsing works
- [ ] Routing works
- [ ] Response serialization works
- [ ] Error handling works
- [ ] Connection limits work
- [ ] WebSocket upgrade works

### Integration Tests (Python)
- [ ] Server starts via Python API
- [ ] GraphQL request returns correct response
- [ ] WebSocket subscriptions work
- [ ] Error responses match format
- [ ] Concurrent requests work
- [ ] Connection tracking works
- [ ] Graceful shutdown works

### Performance Tests
- [ ] Response time <5ms for cached queries
- [ ] Server startup <100ms
- [ ] Memory usage <50MB idle
- [ ] 10,000+ concurrent connections
- [ ] No memory leaks

### Comparison Tests
- [ ] Response identical to FastAPI
- [ ] Headers identical to FastAPI
- [ ] Error format identical to FastAPI

---

## 🎯 Success Metrics

### Code
```
✅ All Rust code compiles without warnings
✅ >95% test coverage
✅ Zero clippy warnings
✅ All 5991+ existing tests pass
```

### Performance
```
✅ Response time: 1.5-3x faster (12ms → 7ms)
✅ Startup time: <100ms
✅ Memory: <50MB idle
✅ Connections: 10,000+ concurrent
```

### Compatibility
```
✅ Python API unchanged
✅ No user code changes
✅ 100% backward compatible
✅ Easy rollback to FastAPI
```

---

## 🚀 Rollout Plan

### Week 1: Implementation
```
Mon-Tue: HTTP server core (3 commits)
Wed:     Response handling (3 commits)
Thu:     WebSocket & connections (3 commits)
Fri:     Testing & docs (6 commits)
```

### Week 2: Testing & Staging
```
Mon-Tue: Full test suite
Wed:     Performance benchmarking
Thu:     Staging deployment
Fri:     Load testing
```

### Week 3: Production
```
Mon-Tue: Feature flag setup
Wed:     Canary rollout (1%)
Thu-Fri: Monitor & scale (10% → 50% → 100%)
```

---

## 🔄 Fallback Strategy

If Rust HTTP server has issues:

```python
# Option 1: Feature flag
FRAISEQL_HTTP_SERVER = "fastapi"  # Revert to FastAPI

# Option 2: Code change
# from fraiseql import create_fraiseql_app  # Revert to FastAPI

# No database migration, no schema changes
# Users don't notice the switch
```

---

## 📊 Comparison Matrix

| Feature | FastAPI | Rust HTTP | Winner |
|---------|---------|-----------|--------|
| Speed | 12-22ms | 7-12ms | Rust |
| Setup | Easy | Easy | Tie |
| Python API | Yes | Yes | Tie |
| Memory | 100-150MB | <50MB | Rust |
| Connections | 1,000/s | 5,000/s | Rust |
| WebSocket | Yes | Yes | Tie |
| Maintenance | Moderate | Low | Rust |
| Debugging | Easy | Medium | FastAPI |
| Production Ready | Yes | Yes (after Phase 16) | Tie |

---

## 🎓 Implementation Tips

### 1. Start Simple
- Get basic server working first
- Add features incrementally
- Test at each step

### 2. Reuse Existing Code
- Use existing Rust GraphQL pipeline (Phase 9)
- Reuse subscription logic (Phase 15b)
- Reuse auth/RBAC (Phases 10-11)

### 3. Test Continuously
```bash
# After each commit
cargo test --lib
pytest tests/ -v

# Performance check
pytest tests/performance/ -v
```

### 4. Document as You Go
- Code comments explain algorithm
- Docstrings for public API
- Update Phase 16 docs

---

## 📞 Common Issues & Solutions

### Issue: "Address already in use"
```python
config = RustHttpConfig(port=8001)  # Use different port
```

### Issue: "Too many open files"
```python
config = RustHttpConfig(max_connections=5000)  # Reduce limit
```

### Issue: "Request timeout"
```python
config = RustHttpConfig(request_timeout_ms=60000)  # Increase to 60s
```

### Issue: "High memory usage"
```python
# Reduce max concurrent connections
config = RustHttpConfig(max_connections=1000)
```

---

## 🔗 Cross-References

**Related Phases**:
- Phase 15b: Tokio driver & subscriptions (prerequisite)
- Phase 17: HTTP/2 & optimization (next)
- Phase 18: Load balancing (after)

**Key Files**:
- Implementation: `.phases/phase-16-rust-http-server.md` (main plan)
- Config: `src/fraiseql/http/config.py`
- Server: `src/fraiseql/http/server.py`
- Rust: `fraiseql_rs/src/http/mod.rs`

---

## 📅 Timeline Estimate

| Task | Estimate | Status |
|------|----------|--------|
| HTTP Core | 2-3 days | Todo |
| Response Handling | 1-2 days | Todo |
| WebSocket | 2-3 days | Todo |
| Python Bridge | 1-2 days | Todo |
| Testing | 3-4 days | Todo |
| Documentation | 1-2 days | Todo |
| **Total** | **2-3 weeks** | **Planning** |

---

## ✅ Pre-Implementation Checklist

- [ ] Read full Phase 16 implementation plan
- [ ] Understand current HTTP handling (FastAPI/uvicorn)
- [ ] Review Tokio async patterns
- [ ] Set up feature branch
- [ ] Review existing Rust code patterns
- [ ] Understand Python-Rust FFI approach
- [ ] Plan test strategy

---

## 🎬 Getting Started

### 1. Create Feature Branch
```bash
git checkout -b feature/phase-16-rust-http-server
```

### 2. Create HTTP Module Structure
```bash
mkdir fraiseql_rs/src/http
touch fraiseql_rs/src/http/{mod.rs,server.rs,request.rs,routing.rs}
```

### 3. Update Cargo.toml
```toml
[dependencies]
http = "1.1"
# ... others as needed
```

### 4. Start with Commit 1
- Implement `HttpServer` struct
- Implement `HttpServerConfig`
- Get basic TCP listener working
- Write unit tests

### 5. Iterate Through Commits
- Each commit is independent
- Test after each commit
- Document as you go

---

## 🎯 Phase 16 Goals

- ✅ Eliminate Python HTTP overhead
- ✅ Maintain 100% backward compatibility
- ✅ Achieve 1.5-3x performance improvement
- ✅ Keep Python API unchanged
- ✅ Enable easier production deployment
- ✅ Set foundation for Phase 17+ optimizations

---

**Version**: 1.0
**Date**: January 3, 2026
**Status**: Ready for Implementation
**Effort**: 2-3 weeks
**Next Action**: Create feature branch and begin implementation

