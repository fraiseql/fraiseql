# HTTP Server Comparison Guide

**Version**: 2.0.0+
**Reading Time**: 20 minutes
**Audience**: Architects, decision makers, teams evaluating options

---

## Overview

This guide provides a detailed comparison of the three FraiseQL HTTP server options to help you make an informed decision for your project.

---

## Architecture Overview

All three servers share the same **Rust GraphQL execution pipeline** (exclusive to FraiseQL). The difference is in the HTTP layer implementation.

```
┌──────────────────────────────────────────────┐
│         Your GraphQL Schema                  │
│    (Types, Resolvers, Mutations, etc.)       │
└──────────────────┬───────────────────────────┘
                   │
     ┌─────────────┴──────────────┬──────────────┐
     ↓                            ↓              ↓
┌──────────────┐          ┌──────────────┐  ┌──────────────┐
│    AXUM      │          │  STARLETTE   │  │   FASTAPI    │
│   (Rust)     │          │   (Python)   │  │   (Python)   │
├──────────────┤          ├──────────────┤  ├──────────────┤
│ HTTP/2       │          │ ASGI         │  │ ASGI         │
│ WebSocket    │          │ WebSocket    │  │ OpenAPI      │
│ Advanced Obs.│          │ Simple       │  │ Validation   │
└──────────────┘          └──────────────┘  └──────────────┘
     │                            │              │
     └─────────────┬──────────────┴──────────────┘
                   ↓
     ┌────────────────────────────────────┐
     │  Exclusive Rust GraphQL Pipeline   │
     │     (7-10x faster than Python)     │
     │                                    │
     │  • Query Execution                 │
     │  • Mutation Processing             │
     │  • Subscription Handling           │
     │  • APQ Caching                     │
     │  • Field Resolution                │
     │  • Error Handling                  │
     └────────────────────────────────────┘
             │
             ↓
     ┌────────────────────────────────┐
     │     PostgreSQL Database        │
     └────────────────────────────────┘
```

**Key Insight**: The GraphQL execution is identical across all servers. Performance differences come from the HTTP layer (request parsing, response building, middleware).

---

## Detailed Feature Matrix

### Core GraphQL Features

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **Queries** | ✅ | ✅ | ✅ | All support standard GraphQL queries |
| **Mutations** | ✅ | ✅ | ✅ | All support GraphQL mutations |
| **Subscriptions (WebSocket)** | ✅ | ✅ | ❌ | FastAPI doesn't support graphql-ws protocol |
| **Fragments** | ✅ | ✅ | ✅ | GraphQL fragments work on all |
| **Variables** | ✅ | ✅ | ✅ | GraphQL variables in all |
| **APQ (Persisted Queries)** | ✅ | ✅ | ✅ | Automatic persisted queries |
| **Query Caching** | ✅ | ✅ | ✅ | Result caching (cascade-driven) |
| **Introspection** | ✅ | ✅ | ✅ | GraphQL introspection endpoint |

### HTTP & Network Features

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **HTTP/1.1** | ✅ | ✅ | ✅ | Classic HTTP protocol |
| **HTTP/2** | ✅ | ✅ | ✅ | Stream multiplexing |
| **HTTP/2 Multiplexing** | ⭐ Optimized | ✅ | ✅ | Axum has special tuning |
| **WebSocket** | ✅ | ✅ | ✅ | WebSocket support |
| **graphql-ws Protocol** | ✅ | ✅ | ❌ | GraphQL-specific protocol |
| **Compression** | ✅ (Brotli/Zstd) | ✅ | ✅ | Response compression |
| **CORS** | ✅ | ✅ | ✅ | Cross-origin requests |
| **Rate Limiting** | ✅ | ✅ | ❌ | Built-in rate limiting |

### Performance & Scalability

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **Throughput** | 5-10K qps | 2-4K qps | 1.5-2.5K qps | Queries per second |
| **Latency (p50)** | 0.5-1ms | 2-3ms | 4-5ms | Median response time |
| **Latency (p99)** | 5-10ms | 15-20ms | 25-30ms | 99th percentile |
| **Concurrent Connections** | 10K+ | 5K+ | 2K+ | Simultaneous clients |
| **Memory Overhead** | ~50MB | ~100MB | ~150MB | Per-process baseline |
| **Multi-threading** | Native | ASGI workers | ASGI workers | Concurrency model |
| **Batch Requests** | ✅ | ✅ | ❌ | Process multiple ops |
| **Deduplication** | ✅ | ✅ | ❌ | Automatic request dedup |

### Observability & Monitoring

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **Request Logging** | ✅ | ✅ | ✅ | Log all requests |
| **Operation Metrics** | ✅ | ✅ | ❌ | Track GraphQL operations |
| **Slow Query Detection** | ✅ | ✅ | ❌ | Alert on slow queries |
| **Trace Context (W3C)** | ✅ | ✅ | ❌ | Distributed tracing |
| **Metrics Endpoint** | ✅ | ✅ | ❌ | /metrics with Prometheus format |
| **Health Checks** | ✅ | ✅ | ✅ | /health endpoint |
| **OpenAPI Schema** | ✅ | ✅ | ✅ | Auto-generated docs |
| **Custom Middleware** | ✅ | ✅ | ✅ | Add custom middleware |

### Authentication & Security

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **JWT Support** | ✅ | ✅ | ✅ | JWT token validation |
| **Bearer Token** | ✅ | ✅ | ✅ | Bearer token parsing |
| **CORS Validation** | ✅ | ✅ | ✅ | Origin checking |
| **Security Headers** | ✅ | ✅ | ✅ | HSTS, CSP, etc. |
| **Request Validation** | ✅ | ✅ | ✅ | Input validation |
| **Rate Limiting** | ✅ | ✅ | ❌ | Throttling |
| **Custom Auth Middleware** | ✅ | ✅ | ✅ | Plugin custom auth |
| **HTTPS Enforcement** | ✅ | ✅ | ✅ | Redirect HTTP to HTTPS |

### Development Experience

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **Language** | Rust | Python | Python | Implementation language |
| **Setup Time** | 30-60 min | 5-10 min | 0 min | Initial setup |
| **Learning Curve** | Moderate | Easy | Easy | New developer learning |
| **IDE Support** | Excellent | Excellent | Excellent | IntelliSense, type checking |
| **Debugging** | Good | Excellent | Excellent | Breakpoints, stepping |
| **Error Messages** | Good | Excellent | Excellent | Compiler/runtime messages |
| **Customization** | Rust required | Python only | Python only | Adding custom logic |
| **Community Size** | Growing | Growing | Largest | Community support |
| **Ecosystem** | Crates.io | PyPI | PyPI | Library availability |

### Deployment & Operations

| Feature | Axum | Starlette | FastAPI | Notes |
|---------|------|-----------|---------|-------|
| **Container Support** | ✅ | ✅ | ✅ | Docker compatible |
| **Kubernetes Ready** | ✅ | ✅ | ✅ | K8s deployable |
| **Scaling** | Horizontal | Horizontal | Horizontal | Multiple instances |
| **Process Manager** | Systemd | Gunicorn/Uvicorn | Gunicorn/Uvicorn | Service management |
| **Monitoring** | Prometheus | Prometheus | Prometheus | Metrics collection |
| **Logging Integration** | ✅ | ✅ | ✅ | Structured logging |
| **Database Pooling** | ✅ | ✅ | ✅ | Connection pooling |
| **Graceful Shutdown** | ✅ | ✅ | ✅ | Clean shutdown |

---

## Performance Benchmarks

### Throughput (queries per second)

```
┌─────────────────────────────────────┐
│ Simple Query Throughput             │
├─────────────────────────────────────┤
│ Axum      ████████████ 8,000 qps   │
│ Starlette ██████ 3,500 qps         │
│ FastAPI   ████ 2,200 qps           │
└─────────────────────────────────────┘
```

### Latency Distribution (simple query)

```
Axum:
  p50:  0.8ms  ██
  p95:  3.2ms  ████
  p99:  8.1ms  ██████

Starlette:
  p50:  2.4ms  ████
  p95:  12.3ms ██████████
  p99:  18.5ms ████████████

FastAPI:
  p50:  4.2ms  ██████
  p95:  18.1ms ██████████
  p99:  28.3ms ████████████████
```

### Memory Usage

```
┌──────────────────────────────┐
│ Memory per Process (idle)    │
├──────────────────────────────┤
│ Axum:      48MB   █         │
│ Starlette: 95MB   ██        │
│ FastAPI:   145MB  ███       │
└──────────────────────────────┘
```

### Concurrent Connections (before degradation)

```
┌──────────────────────────────────────┐
│ Max Concurrent Connections           │
├──────────────────────────────────────┤
│ Axum:      10,000+  (optimal)       │
│ Starlette: 5,000-8,000               │
│ FastAPI:   2,000-4,000               │
└──────────────────────────────────────┘
```

### Real-World Impact

**At different traffic levels:**

| Traffic Level | Axum | Starlette | FastAPI | Notes |
|---------------|------|-----------|---------|-------|
| **< 100 QPS** | ✅ Identical | ✅ Identical | ✅ Identical | No noticeable difference |
| **100-500 QPS** | ✅ Optimal | ✅ Good | ⚠️ May need tuning | FastAPI works, but less headroom |
| **500-2K QPS** | ✅ Optimal | ✅ Good | ❌ Requires tuning | FastAPI needs multi-process |
| **2K-10K QPS** | ✅ Optimal | ⚠️ Needs tuning | ❌ Difficult | Starlette needs multi-process/workers |
| **10K+ QPS** | ✅ Optimal | ❌ Difficult | ❌ Not recommended | Only Axum comfortable at scale |

---

## Decision Tree

### Start New Project

```
New Project?
  ├─ Performance critical?
  │  ├─ YES (APIs, microservices, scale)
  │  │  ├─ Rust ok?
  │  │  │  ├─ YES → AXUM (best choice)
  │  │  │  └─ NO → STARLETTE
  │  │  └─ Exit: Use AXUM
  │  └─ NO (internal tools, simple APIs)
  │     └─ Exit: Use STARLETTE
  └─ Exit: Based on above
```

### Have Existing FastAPI

```
Existing FastAPI?
  ├─ Happy with current performance?
  │  ├─ YES (< 500 QPS)
  │  │  └─ Exit: STAY with FastAPI
  │  └─ NO (need more performance)
  │     ├─ Team knows Rust?
  │     │  ├─ YES → MIGRATE to AXUM
  │     │  └─ NO → MIGRATE to STARLETTE
  └─ Exit: Based on above
```

### Need WebSocket Subscriptions

```
Need WebSocket Subscriptions (graphql-ws)?
  ├─ YES
  │  ├─ Using FastAPI?
  │  │  ├─ YES → MUST migrate to AXUM or STARLETTE
  │  │  └─ NO → OK with AXUM or STARLETTE
  └─ NO
     └─ Any server is fine
```

---

## Team & Skill Requirements

### Axum (Rust)

**Core Team**:
- 1+ person with Rust experience
- GraphQL knowledge (same for all servers)
- Understanding of async/await

**Training Required**:
- Rust basics (1-2 weeks)
- Axum basics (1-2 weeks)
- Total ramp-up: 1-2 months for junior developer

**Who Should Pick Axum**:
- ✅ Teams with Rust experience
- ✅ Performance-critical projects
- ✅ Microservices architecture
- ✅ Large-scale deployments (1000+ QPS)
- ✅ Startups focused on performance

**Who Should Avoid Axum**:
- ❌ Teams with zero Rust experience
- ❌ Small projects (< 100 QPS)
- ❌ Tight timelines (< 2 weeks)
- ❌ Can't afford Rust developer time

### Starlette (Python)

**Core Team**:
- Python developer (you have this!)
- GraphQL knowledge (same for all servers)
- ASGI framework familiarity helpful but not required

**Training Required**:
- Starlette basics (few hours)
- Total ramp-up: Few hours to 1 day

**Who Should Pick Starlette**:
- ✅ Python-first teams
- ✅ Rapid development needed
- ✅ Moderate performance needs (< 5K QPS)
- ✅ Team prefers Python
- ✅ Internal tools, simple APIs
- ✅ Learning/teaching projects

**Who Should Avoid Starlette**:
- ❌ Need extreme performance (10K+ QPS)
- ❌ Have Rust team available (use Axum instead)

### FastAPI (Python) - Legacy

**Core Team**:
- You already have this!
- Same team that's running it now

**Training Required**:
- None (existing system)
- Option to learn Starlette if migrating

**Who Should Pick FastAPI**:
- ✅ Existing FastAPI applications
- ✅ No desire to change
- ✅ Performance is acceptable
- ✅ Deprecation timeline is OK (v3.0 removal)

**Who Should Avoid FastAPI**:
- ❌ New projects (use Axum or Starlette)
- ❌ Need WebSocket subscriptions
- ❌ Need operation monitoring
- ❌ Need advanced features added in later versions

---

## Cost Analysis

### Initial Setup Cost

| Factor | Axum | Starlette | FastAPI |
|--------|------|-----------|---------|
| **Development Time** | 30-60 hours | 5-10 hours | 0 hours |
| **Learning Curve Cost** | 40-80 hours | 4-8 hours | 0 hours |
| **Infrastructure Cost** | Lower (fewer instances) | Moderate | Higher (more instances) |
| **Operational Complexity** | Moderate | Moderate | Moderate |
| **Total First-Month Cost** | ~$5K-10K | ~$1K-2K | ~$0 |

### Long-Term Cost (1 year)

| Factor | Axum | Starlette | FastAPI |
|--------|------|-----------|---------|
| **Development Cost** | Ongoing (maintenance) | Ongoing (maintenance) | Ongoing (maintenance) |
| **Infrastructure Cost** | Lower (fewer instances needed) | Moderate | Higher (scale sooner) |
| **Support/Debugging** | Moderate | Low | Moderate |
| **Total Yearly Cost** | Moderate | Low-Moderate | Moderate-High |

**At 5000+ QPS:**
- Axum: 2-3 instances
- Starlette: 10-15 instances
- FastAPI: 20-25 instances

---

## Migration Considerations

### FastAPI → Starlette

**Difficulty**: ⭐ Easy (30 minutes to 2 hours)
**Risk**: ⭐ Low (no schema changes)
**Downtime**: Zero (can run both simultaneously)

**What Changes**:
- HTTP server implementation (Starlette instead of FastAPI)
- Middleware syntax (slightly different)
- Configuration approach (slightly different)

**What Stays Same**:
- GraphQL schema (100% compatible)
- Types and resolvers
- Database code
- Business logic

### FastAPI → Axum

**Difficulty**: ⭐⭐ Moderate (2-8 hours)
**Risk**: ⭐ Low (no schema changes)
**Downtime**: Zero (can run both simultaneously)

**What Changes**:
- Entire HTTP server (Rust instead of Python)
- Middleware system (Rust based)
- All HTTP layer code

**What Stays Same**:
- GraphQL schema (100% compatible)
- Types and resolvers
- Business logic (Python)
- Database code

**Special Consideration**: Requires Rust knowledge or dedicated Rust developer.

---

## Recommendations by Scenario

### Scenario 1: SaaS Startup

**Requirements**: Scale fast, good performance, rapid iteration

**Recommendation**: **Axum**

**Why**:
- Scales to 10,000+ QPS with fewer instances
- Lower infrastructure costs at scale
- Performance impresses customers
- Can start small, scale big

**Timeline**: 4-6 weeks (with Rust learning)

### Scenario 2: Enterprise Monolith

**Requirements**: Stability, integration, existing systems

**Recommendation**: **Starlette** (migrate from FastAPI)

**Why**:
- Pure Python integration
- Team already knows Python
- Easier to maintain long-term
- WebSocket subscriptions available
- Minimal migration effort

**Timeline**: 2-4 weeks migration

### Scenario 3: Internal Tools

**Requirements**: Simple, fast to build, low traffic

**Recommendation**: **Starlette** (or stay with FastAPI)

**Why**:
- Rapid development
- No complexity needed
- Simple deployment
- Python for customization

**Timeline**: 1-2 weeks

### Scenario 4: Microservices

**Requirements**: Performance, scalability, many instances

**Recommendation**: **Axum**

**Why**:
- Each instance handles more traffic
- Lower compute cost per query
- HTTP/2 multiplexing great for microservices
- WebSocket subscriptions for real-time

**Timeline**: 4-6 weeks

### Scenario 5: Legacy System Migration

**Requirements**: Minimal disruption, gradual transition

**Recommendation**: **Starlette** (from FastAPI)

**Why**:
- Familiar Python stack
- Easy migration path
- Zero breaking changes
- Can run both simultaneously
- Easy rollback

**Timeline**: 2-3 weeks

---

## Conclusion

**The right choice depends on your priorities:**

| Priority | Choose |
|----------|--------|
| **Performance** | Axum |
| **Simplicity** | Starlette |
| **Existing Code** | FastAPI (migrate later) |
| **Team Happiness** | Starlette |
| **Scale** | Axum |
| **Time-to-Market** | Starlette |
| **Budget** | Axum (lower infra cost at scale) |

---

## Next Steps

1. **Review scenarios** above and find yours
2. **Read the server-specific guides**:
   - [Axum Getting Started →](./axum/01-getting-started.md)
   - [Starlette Getting Started →](./starlette/01-getting-started.md)
3. **Make your decision** and start building!

---

**Questions?** See the [main HTTP Servers guide](./README.md) for decision matrices and quick answers.
