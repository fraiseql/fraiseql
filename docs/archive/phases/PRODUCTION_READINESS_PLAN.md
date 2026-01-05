# FraiseQL Subscriptions - Production Readiness Plan
## NASA-Quality Standards

**Target**: Production-grade subscriptions module with comprehensive testing, security, and reliability.

**Current Grade**: B (75% production-ready)

**Estimated Timeline**: 2-3 weeks to full NASA-quality readiness

---

## CRITICAL ISSUES (Must Fix First)

### 1. TokenBucket Rate Limiting Bug ⚠️ CRITICAL
**File**: `fraiseql_rs/src/subscriptions/rate_limiter.rs` (lines 91-101)

**Problem**: TokenBucket is cloned on every check, defeating state maintenance
```rust
// Current broken code:
pub fn try_consume(&mut self, tokens: f64) -> bool {
    self.refill();
    // ...
}
```

**Impact**: Rate limiting doesn't actually limit - buckets reset on each check

**Fix Strategy**:
- [ ] Option A: Use interior mutability (Cell/RefCell)
- [ ] Option B: Refactor to use Arc<Mutex<TokenBucket>>
- [ ] Option C: Store tokens as atomic values
- **Recommendation**: Option B (most idiomatic for async context)

**Test Required**: Verify bucket state persists across 100+ rapid checks

---

### 2. No GraphQL Query Validation ⚠️ HIGH
**File**: `fraiseql_rs/src/subscriptions/executor.rs` (lines 194-202)

**Problem**: `validate_subscription()` is a no-op
```rust
fn validate_subscription(&self, _subscription: &ExecutedSubscription) -> Result<(), SubscriptionError> {
    // TODO: In full implementation, validate GraphQL syntax, operation name, etc.
    Ok(())  // Always succeeds!
}
```

**Impact**:
- Any query accepted (invalid, injection attempts, malicious)
- Security risk from unparsed queries
- No syntax error detection

**Fix Strategy**:
- [ ] Integrate graphql-parser crate
- [ ] Validate subscription query syntax
- [ ] Validate operation type is `subscription`
- [ ] Validate field existence against schema
- [ ] Add complexity scoring

**Test Required**:
- Invalid queries rejected
- Valid queries accepted
- Injection attempts blocked
- Complexity limits enforced

---

### 3. Event Cloning Performance Issue ⚠️ HIGH
**File**: `fraiseql_rs/src/subscriptions/event_bus/mod.rs` (line 205)

**Problem**: Events cloned for each subscriber
```rust
for sender in subs.iter() {
    if sender.send(event.clone()).is_ok() {  // Clone on every send!
        delivered += 1;
    }
}
```

**Impact**:
- Memory pressure with large payloads
- Degraded performance with many subscribers
- CPU overhead on cloning

**Fix Strategy**:
- [ ] Wrap Event in Arc<Event>
- [ ] Zero-copy distribution to subscribers
- [ ] Benchmark before/after

**Test Required**:
- Large event payload (1MB)
- 100+ concurrent subscribers
- Memory usage tracking

---

## PHASE 1: BUG FIXES & CORE STABILITY (Week 1)

### 1.1 Fix TokenBucket Rate Limiting
- [ ] Design interior mutability approach
- [ ] Implement new TokenBucket with Arc<Mutex<>>
- [ ] Update rate_limiter.rs
- [ ] Add unit tests: 100+ rapid requests still rate limited
- [ ] Benchmark: Verify minimal overhead

**Definition of Done**: All rate limit tests pass, metrics show correct limiting

---

### 1.2 Add GraphQL Query Validation
- [ ] Add graphql-parser to Cargo.toml
- [ ] Implement subscription query parser
- [ ] Add schema validation
- [ ] Add operation type checking
- [ ] Add complexity scoring integration
- [ ] Add unit tests: 20+ test cases

**Definition of Done**:
- Invalid queries rejected with clear errors
- Valid queries accepted
- Injection attempts blocked

---

### 1.3 Fix Event Cloning
- [ ] Refactor Event → Arc<Event>
- [ ] Update all event_bus implementations
- [ ] Benchmark memory usage
- [ ] Add stress test: 1000 subscribers, 10KB events

**Definition of Done**: Memory usage reduced by 80%+ with many subscribers

---

### 1.4 Add Subscription Lifetime Limits
- [ ] Add max_subscription_duration to config
- [ ] Implement subscription timeout enforcement
- [ ] Add cleanup task for expired subscriptions
- [ ] Add unit tests: subscription expires after N seconds

**Definition of Done**: Subscriptions automatically cleanup after timeout

---

## PHASE 2: COMPREHENSIVE TESTING (Week 1-2)

### 2.1 Unit Test Coverage
- [ ] Add missing tests for Redis event bus (15+ tests)
- [ ] Add missing tests for PostgreSQL event bus (15+ tests)
- [ ] Add consumer group tests (10+ tests)
- [ ] Add connection pool stress tests (8+ tests)
- [ ] Add all negative test cases (error paths)

**Target**: 85%+ code coverage

---

### 2.2 Integration Testing
- [ ] Redis integration: publish/subscribe end-to-end
- [ ] PostgreSQL integration: LISTEN/NOTIFY end-to-end
- [ ] Multi-subscriber scenarios (10, 100, 1000 subscribers)
- [ ] Event filtering edge cases
- [ ] Error recovery scenarios

**Target**: All happy paths + error paths tested

---

### 2.3 Load Testing
- [ ] 1,000 concurrent connections
- [ ] 10,000 subscriptions
- [ ] 100 events/second throughput
- [ ] Memory stability over 1 hour
- [ ] No connection leaks

**Target**: Identify performance bottlenecks

---

### 2.4 Stress Testing
- [ ] 10,000 concurrent connections
- [ ] 100,000 subscriptions
- [ ] Connection drop/reconnect scenarios
- [ ] Network latency injection (50ms, 500ms)
- [ ] Memory pressure (garbage collection patterns)

**Target**: System degrades gracefully, no crashes

---

### 2.5 Chaos Engineering
- [ ] Redis unavailability (fallback to PostgreSQL)
- [ ] PostgreSQL unavailability (fallback to in-memory)
- [ ] Random connection drops (10% failure rate)
- [ ] Event bus lag (100ms→1000ms delays)
- [ ] Rate limiter false positives

**Target**: System recovers within configured timeouts

---

## PHASE 3: SECURITY AUDIT (Week 2)

### 3.1 Authentication & Authorization
- [ ] Verify connection authentication enforcement
- [ ] Test per-user subscription isolation
- [ ] Verify tenant_id segregation
- [ ] Test authorization header handling

**Target**: No cross-user data leaks

---

### 3.2 Rate Limiting Verification
- [ ] Test per-user limits work correctly (50/min)
- [ ] Test per-subscription limits (100/sec)
- [ ] Test per-connection limits (5 concurrent)
- [ ] Test bypass prevention (burst attacks, token stuffing)

**Target**: All limits enforced, no bypasses

---

### 3.3 Input Validation
- [ ] Query size limit: 64KB boundary testing
- [ ] Payload size limit: 1MB boundary testing
- [ ] Message size limit: 256KB boundary testing
- [ ] Filter complexity limit: 50 level testing
- [ ] Invalid query detection

**Target**: All inputs properly validated

---

### 3.4 Injection Attack Prevention
- [ ] GraphQL injection attempts blocked
- [ ] JSON injection in payloads handled
- [ ] SQL injection (if database queries added)
- [ ] XSS prevention in event data

**Target**: OWASP Top 10 coverage

---

### 3.5 DoS Attack Prevention
- [ ] Large query attack (complexity bomb)
- [ ] Connection exhaustion (10,000+ connections)
- [ ] Event spam (millions of events/sec)
- [ ] Memory exhaustion (huge payloads)

**Target**: System remains responsive under attack

---

## PHASE 4: PERFORMANCE OPTIMIZATION (Week 2)

### 4.1 Benchmarking
- [ ] Publish latency (p50, p95, p99)
- [ ] Subscribe latency
- [ ] Event delivery latency
- [ ] Memory footprint per connection/subscription
- [ ] CPU utilization patterns

**Target**: Baseline metrics for all operations

---

### 4.2 Optimization Opportunities
- [ ] Index resource lookups (O(n) → O(1))
- [ ] Batch event delivery where possible
- [ ] Memory pool for event allocations
- [ ] Connection buffer tuning
- [ ] Lock contention analysis

**Target**: 50%+ performance improvement in key metrics

---

### 4.3 Monitoring & Metrics
- [ ] Add structured logging (tracing crate)
- [ ] Implement request tracing (correlation IDs)
- [ ] Add slow subscription detection
- [ ] Add memory pressure alerts
- [ ] Build Grafana dashboards (3+)

**Target**: Observable system with actionable metrics

---

## PHASE 5: PYTHON INTEGRATION (Week 2-3)

### 5.1 PyO3 Bindings
- [ ] Export SubscriptionManager from Rust
- [ ] Create Python wrapper module
- [ ] Python type hints and docstrings
- [ ] Error translation to Python exceptions

**Target**: Clean Python API

---

### 5.2 GraphQL Integration
- [ ] Add subscription root to schema
- [ ] Implement subscription resolvers
- [ ] Add websocket protocol handler
- [ ] Client connection lifecycle

**Target**: End-to-end subscription flow

---

### 5.3 Python Tests
- [ ] 20+ integration tests
- [ ] Real GraphQL schema testing
- [ ] End-to-end workflows
- [ ] Error handling paths

**Target**: Production-ready Python API

---

## PHASE 6: DOCUMENTATION & HARDENING (Week 3)

### 6.1 Documentation
- [ ] Architecture guide (with diagrams)
- [ ] Configuration guide
- [ ] API documentation (Rust + Python)
- [ ] Troubleshooting guide
- [ ] Production deployment guide
- [ ] Monitoring guide (Grafana)

**Target**: New developer can understand system in 1 hour

---

### 6.2 Example Applications
- [ ] Chat application example (publish/subscribe)
- [ ] Real-time dashboard example
- [ ] Event streaming example
- [ ] Error recovery example

**Target**: Developers can copy-paste working code

---

### 6.3 Production Hardening
- [ ] Add graceful shutdown (SIGTERM handling)
- [ ] Add health check endpoints
- [ ] Add readiness probes
- [ ] Add configuration validation
- [ ] Add startup sanity checks

**Target**: Safe production deployment

---

### 6.4 Release Preparation
- [ ] Update CHANGELOG.md
- [ ] Update version numbers (prep for v1.10.0)
- [ ] Create release notes
- [ ] Migration guide (if needed)

**Target**: Ready for release

---

## SUCCESS CRITERIA

### Pre-Launch Checklist
- [ ] All critical issues fixed (3/3)
- [ ] Unit test coverage ≥ 85%
- [ ] Integration tests covering all paths
- [ ] Load test: 10,000 concurrent connections stable
- [ ] Stress test: System recovers from failures
- [ ] Chaos test: All failure scenarios handled
- [ ] Security audit: No vulnerabilities found
- [ ] Performance baseline established
- [ ] Python bindings complete and tested
- [ ] Documentation complete
- [ ] Example applications working
- [ ] Production hardening complete
- [ ] Team sign-off on quality

### NASA-Quality Standards Met
- ✅ Comprehensive test coverage (85%+)
- ✅ Security audit completed
- ✅ Performance characterized
- ✅ Error handling comprehensive
- ✅ Observable system (logging, metrics, tracing)
- ✅ Well-documented
- ✅ Production-ready deployment
- ✅ Graceful failure modes

---

## EFFORT ESTIMATION

| Phase | Effort | Timeline |
|-------|--------|----------|
| Phase 1: Bug Fixes | 20 hrs | Week 1 |
| Phase 2: Testing | 40 hrs | Week 1-2 |
| Phase 3: Security | 20 hrs | Week 2 |
| Phase 4: Performance | 20 hrs | Week 2 |
| Phase 5: Python Integration | 20 hrs | Week 2-3 |
| Phase 6: Documentation | 15 hrs | Week 3 |
| **Total** | **135 hrs** | **3 weeks** |

---

## STARTING POINT

**Next action**: Start Phase 1 - Fix the 3 critical issues

**Recommended order**:
1. TokenBucket rate limiting (blocks release)
2. GraphQL query validation (security critical)
3. Event cloning fix (performance critical)

Then proceed to Phase 2 (testing) with fixes in place.

---

**Quality Target**: Grade A (95%+) - NASA-quality production system

**Target Ship Date**: After completing all 6 phases + sign-off
