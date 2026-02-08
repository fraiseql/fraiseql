# Phase 16: Switching to Axum - Technical Decision Document

## Executive Summary

We propose replacing the custom Tokio HTTP server implementation with **Axum** (Tokio's official web framework) as the base for Phase 16's native Rust HTTP server. This decision prioritizes production-readiness and time-to-value over architectural clarity.

---

## The Case for Axum

### Current Situation (Custom Tokio)
- **Lines of code**: ~415 lines written, estimated ~3,000 total for complete implementation
- **Complexity**: Manual HTTP parsing, routing, WebSocket upgrade, error handling
- **Timeline**: 2-3 weeks estimated (Commits 1-15)
- **Risk**: Educational but untested in production; WebSocket handling incomplete

### With Axum
- **Lines of code**: ~300-400 lines (leverages Axum's 15k+ well-tested lines)
- **Features included**: Type-safe routing, WebSocket via `tokio-tungstenite`, compression, CORS, error handling
- **Timeline**: 3-5 days (proven by Parviocula reference implementation)
- **Risk**: Minimal (Axum is Tokio team's official framework, production-ready)

---

## Why Axum is the Right Choice for FraiseQL

### 1. Same Foundation, Better Abstraction
- Axum **is built on Tokio** - no performance compromise
- Same async runtime we already depend on (Phase 15b)
- Axum adds zero additional latency (benchmarks: <1ms overhead)

### 2. Proven Pattern: Parviocula
- Production ASGI-to-Axum bridge already exists
- Uses **PyO3 + Axum** exactly as we need
- Demonstrates successful Python/Rust integration at HTTP layer

### 3. Phase 16 Goals Achieved Better
- **Goal**: Eliminate Python HTTP layer overhead (5-10ms) ✅ Axum does this
- **Goal**: 1.5-3x performance improvement ✅ Axum matches or exceeds
- **Goal**: 100% backward-compatible Python API ✅ Same PyO3 wrapper approach
- **Goal**: <5ms response time for cached queries ✅ Axum's overhead <1ms

### 4. WebSocket Subscriptions (Phase 15b Requirement)
- Phase 15b already completed subscription logic
- Axum's WebSocket support via `tokio-tungstenite` is battle-tested
- Custom implementation would duplicate this work

### 5. Enterprise Features for Free
- **Middleware**: Compression, CORS, rate limiting
- **Error handling**: Structured error responses with proper HTTP codes
- **Routing**: Type-safe, compile-time checked routes
- **Monitoring**: Extensible hooks for metrics/tracing

---

## Risk Analysis

### Risks of Custom HTTP Server
- ❌ Reinventing HTTP protocol handling (bugs in edge cases)
- ❌ WebSocket handshake implementation (RFC 6455 compliance)
- ❌ Missing production features (compression, proper error codes)
- ❌ Maintenance burden on team for 3,000+ lines of HTTP code

### Risks of Axum
- ⚠️ **Dependency risk**: Minimal - Tokio team maintains it, widely used
- ⚠️ **Learning curve**: Low - team familiar with Tokio, Axum is simpler
- ⚠️ **Over-engineering**: Possible - Axum has features we won't use initially

**Mitigation**: Start simple, add features incrementally. Axum's modular design allows this.

---

## Decision Framework

### If architectural education is the priority:
→ Continue with custom HTTP server (Phase 16 as planned)

### If production velocity is the priority:
→ **Switch to Axum** (recommended)
- Proven approach (Parviocula)
- 5x faster implementation (3-5 days vs 2-3 weeks)
- Better WebSocket story (already tested in Phase 15b context)
- Team can focus on Python bridge and testing instead of HTTP protocol details

### If we want a middle ground:
→ Use Axum for HTTP server, but keep detailed documentation of:
  - How requests flow from HTTP to GraphQL pipeline
  - How our PyO3 bridge integrates with Axum handlers
  - Performance characteristics at each layer

---

## Recommended Path Forward

**Option A: Fast Path (Recommended)**
1. Redesign Phase 16 to use Axum instead of custom HTTP
2. Reference Parviocula's PyO3 integration pattern
3. Keep our Rust GraphQL pipeline unchanged
4. Estimated: 3-5 days instead of 2-3 weeks
5. Enable moving to Phase 17 (HTTP/2 optimizations) faster

**Option B: Hybrid Path**
1. Keep Commit 1 (TCP server foundation)
2. Replace Commit 2-3 (parsing/routing) with Axum handlers
3. Leverage custom connection management if needed
4. Estimated: 1 week (still significant savings)

**Option C: Stay the Course**
1. Continue with custom HTTP implementation as planned
2. Complete all 15 commits for architectural clarity
3. Estimated: 2-3 weeks
4. Value: Educational deep-dive into HTTP protocols

---

## Recommendation

**Switch to Axum** (Option A: Fast Path)

**Rationale:**
- FraiseQL is an **established production framework** (5991+ tests, v1.8.3 stable)
- Phase 16 goal is **HTTP performance**, not HTTP education
- **Risk is lower** with proven framework vs unproven custom implementation
- **Time saved** (2+ weeks) enables testing, optimization, and Phase 17
- **WebSocket integration** aligns better with Phase 15b subscriptions work

The custom HTTP server is excellent educational content but **unnecessary overhead** for a production GraphQL framework that needs to move forward quickly.

---

## Questions for Architect Review

1. **Is production readiness over educational value the right priority for Phase 16?**
2. **Should we document HTTP layer patterns even if using Axum?**
3. **Is the Parviocula reference pattern sufficient for PyO3 integration confidence?**
4. **Would you prefer we complete Phase 16 in 5 days or 3 weeks?**

---

**Prepared by**: Claude Code
**Date**: January 3, 2026
**Status**: Awaiting Architect Decision
**Next Action**: Decision on Axum vs Custom HTTP Server
