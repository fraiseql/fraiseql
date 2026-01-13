# fraiseql-wire Roadmap: MVP to Production

This document outlines the path from the current MVP (v0.1.0) to a production-ready release.

## Current Status: MVP Complete ✅

**Version**: 0.1.0
**Status**: Feature-complete MVP with comprehensive documentation
**Tests**: 34 unit tests passing, 13 integration tests ready
**Quality**: Zero clippy warnings, full documentation coverage

### What Works Now

- ✅ Async JSON streaming from Postgres 17
- ✅ TCP and Unix socket connections
- ✅ Connection string parsing
- ✅ SQL predicate pushdown
- ✅ Rust-side predicate filtering
- ✅ Server-side ORDER BY
- ✅ Configurable chunk size
- ✅ Query cancellation on drop
- ✅ Bounded memory usage
- ✅ Comprehensive error handling
- ✅ Tracing/observability
- ✅ Full test coverage and documentation

---

## Phase 7: Stabilization (v0.1.x)

### Goal
Harden the MVP for real-world use without adding new features.

### Tasks

#### 7.1 Performance Profiling & Optimization

##### 7.1.1 Micro-benchmarks (Core Operations) ✅
- [x] Set up Criterion benchmarking framework
- [x] Protocol encoding/decoding benchmarks
- [x] JSON parsing benchmarks (small, large, deeply nested)
- [x] Connection string parsing benchmarks
- [x] Chunking strategy overhead measurements
- [x] Error handling overhead benchmarks
- [x] String matching and HashMap lookup benchmarks
- [x] Baseline establishment for regression detection
- [x] CI integration ready (always-run, ~30 seconds)

**Status**: Complete - 6 benchmark groups with detailed statistical analysis

##### 7.1.2 Integration Benchmarks (With Postgres) ✅
- [x] Throughput benchmarks (rows/sec) with 1K, 100K, 1M row sets
- [x] Memory usage under load with different chunk sizes
- [x] Time-to-first-row latency measurements
- [x] Connection setup time benchmarks
- [x] Large result set streaming (memory stability)
- [x] CI integration (nightly, requires Postgres service)
- [x] Predicate effectiveness benchmarks
- [x] Chunking strategy impact measurements
- [x] JSON parsing load benchmarks
- [x] Test database setup with v_test_* views
- [x] GitHub Actions workflow for nightly execution

**Status**: Complete - 8 benchmark groups with Postgres, GitHub Actions integration, test database schema

##### 7.1.3 Comparison Benchmarks (vs tokio-postgres) - Pending
- [ ] Set up tokio-postgres comparison suite
- [ ] Memory usage comparison
- [ ] Throughput (rows/sec) comparison
- [ ] Time-to-first-row comparison
- [ ] CPU usage patterns comparison
- [ ] Manual/pre-release execution only (not in CI)

##### 7.1.4 Documentation & Optimization
- [ ] Profile hot paths with flamegraph
- [ ] Optimize identified bottlenecks
- [ ] Update README with benchmark results
- [ ] Create performance tuning guide
- [ ] Publish baseline results in CHANGELOG

#### 7.2 Security Audit
- [ ] Review all unsafe code (if any)
  - Current codebase appears to have none, verify
  - Document safety guarantees

- [ ] Authentication review
  - Cleartext password handling
  - Connection string parsing (no logging credentials)
  - Error messages (don't leak sensitive info)

- [ ] Connection validation
  - Verify SSL can be added safely
  - Check for connection hijacking issues
  - Review cancellation mechanism safety

- [ ] Dependencies audit
  - Run `cargo audit` regularly
  - Pin critical dependency versions
  - Review major dependency updates

#### 7.3 Real-World Testing
- [ ] Set up staging database for testing
  - Use realistic data volumes
  - Test with various JSON shapes
  - Test edge cases (very large JSON, deeply nested, etc.)

- [ ] Load testing
  - Sustained connections (100+)
  - High throughput (1M+ rows)
  - Memory stability over time

- [ ] Stress testing
  - Connection drops
  - Network delays
  - Database unavailability
  - Query timeouts

#### 7.4 Error Message Refinement
- [ ] Review all error messages
  - Are they actionable?
  - Do they help debug issues?
  - Are they user-friendly?

- [ ] Add common error scenarios
  - Connection refused
  - Authentication failed
  - Schema mismatch
  - JSON decode errors

- [ ] Documentation
  - Troubleshooting guide
  - Common errors and solutions
  - Performance tuning tips

#### 7.5 CI/CD Improvement
- [ ] GitHub Actions enhancements
  - Run integration tests against real Postgres
  - Add performance benchmarks to CI
  - Track benchmark history
  - Add coverage reporting

- [ ] Docker improvements
  - Multi-platform builds (Linux/ARM)
  - Optimized build cache

- [ ] Release automation
  - Automated crates.io publishing
  - GitHub release creation
  - Changelog automation

#### 7.6 Documentation Polish
- [ ] API documentation review
  - Ensure all public items documented
  - Add more examples
  - Review for clarity

- [ ] Create troubleshooting guide
- [ ] Create performance tuning guide
- [ ] Create migration guide from tokio-postgres (if applicable)

---

## Phase 8: Feature Expansion (v0.2.0)

### Goal
Add requested features based on real-world usage feedback.

### Optional Features (Select Based on Feedback)

#### 8.1 Typed Streaming
```rust
// Instead of: Stream<Item = Result<serde_json::Value>>
// Support: Stream<Item = Result<T: DeserializeOwned>>

let stream = client
    .query::<User>("user")
    .execute()
    .await?;
```

**Why**: Better type safety, less runtime JSON manipulation
**Effort**: Medium (requires generic query builder)
**Trade-offs**: Adds serde dependency to main API

#### 8.2 Connection Pooling
Create separate `fraiseql-pool` crate:
```rust
let pool = PoolConfig::new("postgres://localhost/db")
    .max_size(10)
    .build()
    .await?;

let client = pool.get().await?;
```

**Why**: Applications need connection reuse
**Effort**: High (significant complexity)
**Trade-offs**: Separate crate, additional maintenance

#### 8.3 TLS Support
```rust
let client = FraiseClient::connect_tls(
    "postgres://localhost/db",
    TlsConfig::builder()
        .ca_cert(...)
        .build()?
)
.await?;
```

**Why**: Required for cloud/remote Postgres
**Effort**: Medium (integrate native-tls or rustls)
**Trade-offs**: Additional dependency, platform-specific issues

#### 8.4 SCRAM Authentication
- Current: Cleartext password only
- Add: SCRAM-SHA-256 (Postgres 10+)

**Why**: Better security than cleartext
**Effort**: Medium (complex auth protocol)
**Trade-offs**: More dependencies, more testing needed

#### 8.5 Query Metrics/Tracing
```rust
// Built-in metrics
client.metrics()
  .query_count
  .row_count
  .bytes_received
  .elapsed
```

**Why**: Observability in production
**Effort**: Low-Medium (add metrics collection)
**Trade-offs**: Slight performance overhead

#### 8.6 Connection Configuration
More connection options:
```rust
ConnectionConfig::builder()
    .statement_timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(10))
    .keepalive_idle(Duration::from_secs(5))
    .build()?
```

**Why**: Better control over timeouts/behavior
**Effort**: Low-Medium
**Trade-offs**: API surface grows slightly

---

## Phase 9: Production Readiness (v1.0.0)

### Goal
Achieve stable, production-ready release.

### Requirements for v1.0.0

#### 9.1 API Stability
- [ ] API audit and stabilization
  - Review all public APIs
  - Finalize error types
  - Lock trait definitions
  - Document stability guarantees

- [ ] Backward compatibility policy
  - Semantic versioning strictly enforced
  - Breaking changes only in major versions
  - Deprecation warnings before removal

#### 9.2 Performance SLAs
Define and meet these targets:
- [ ] Time-to-first-row: < 5ms over local network
- [ ] Throughput: > 100k rows/sec
- [ ] Memory: O(chunk_size) with < 1MB overhead
- [ ] Connection startup: < 100ms
- [ ] CPU efficiency: < 1% baseline idle

#### 9.3 Production Testing
- [ ] Real-world production trial
  - Deploy to actual FraiseQL application
  - Gather metrics and feedback
  - Fix any issues discovered

- [ ] Stress/chaos testing
  - Simulate network failures
  - Test under peak load
  - Verify recovery behavior

#### 9.4 Security Certification
- [ ] Third-party security audit (optional)
- [ ] Vulnerability disclosure policy
- [ ] Security update process

#### 9.5 Compliance
- [ ] License verification
  - All dependencies compatible with MIT OR Apache-2.0
  - REUSE compliance
  - License file updates

- [ ] Legal review
  - Terms of service
  - Privacy considerations
  - Data handling

#### 9.6 Release Preparation
- [ ] Final documentation review
- [ ] Create release notes
- [ ] Tag release in git
- [ ] Publish to crates.io
- [ ] Announce on Rust forums

---

## Success Metrics

### MVP (Current)
- ✅ 6 phases completed
- ✅ 34 unit tests passing
- ✅ Documentation complete
- ✅ Examples working

### Stabilization (Phase 7)
- Performance benchmarks established
- Real-world testing completed
- Zero critical issues
- Security audit passed

### v1.0.0 (Phase 9)
- API stable (no breaking changes in 6+ months)
- 1000+ downloads on crates.io
- Integrated into FraiseQL production
- Community contributions accepted

---

## Decision Framework

### When to Add Features

**YES** if:
- Multiple users request it
- It aligns with "JSON streaming from Postgres" scope
- It doesn't violate hard invariants
- It can be implemented without major refactoring

**NO** if:
- It's solving a different problem
- It requires buffering full result sets
- It breaks the "one query per connection" model
- It requires arbitrary SQL support

### When to Defer Features

Most features should defer to Phase 8 or later unless:
- They're critical for v0.1.0 stability
- They're blocking real-world adoption
- They're trivial to implement

---

## Communication Plan

### Sharing Results
1. **GitHub Releases**: Publish v0.1.0 with full release notes
2. **Crates.io**: Publish v0.1.0 (when ready)
3. **Blog/Announcement**: Share architecture and design
4. **Community**: Share in Rust forums, Reddit, etc.

### Gathering Feedback
1. **GitHub Issues**: Feature requests and bug reports
2. **GitHub Discussions**: Questions and discussions
3. **User Surveys**: Gather requirements for Phase 8
4. **Real-world Trials**: Test with actual FraiseQL

---

## Timeline Estimate

| Phase | Work | Timeline |
|-------|------|----------|
| 7 (Stabilization) | Performance, security, testing | 2-4 weeks |
| 8 (Features) | Based on feedback, 1-2 features | 4-8 weeks |
| 9 (Production) | API finalization, audits, release | 2-4 weeks |
| **Total** | **MVP to v1.0.0** | **8-16 weeks** |

*Actual timeline depends on:*
- Feedback from real-world usage
- Number of issues discovered
- Community contributions
- Team capacity

---

## Next Immediate Steps

1. **Publish v0.1.0**
   - Finalize any last-minute fixes
   - Create comprehensive release notes
   - Publish to crates.io
   - Announce to community

2. **Gather Real-World Feedback**
   - Deploy to FraiseQL production (staging first)
   - Monitor for issues
   - Collect usage metrics
   - Gather feature requests

3. **Start Phase 7 Work**
   - Set up benchmarking infrastructure
   - Run performance profiling
   - Conduct security review
   - Plan stabilization improvements

4. **Plan Phase 8**
   - Prioritize feature requests
   - Design APIs for top features
   - Estimate effort
   - Create implementation plans

---

## Questions for Stakeholders

Before proceeding, consider:

1. **What's the primary use case?**
   - Pure streaming performance?
   - Cost reduction vs. other drivers?
   - Specific data shapes or sizes?

2. **What's the target deployment?**
   - Cloud (AWS/GCP/Azure)?
   - On-premise?
   - Embedded in applications?

3. **What are the SLAs?**
   - Throughput requirements?
   - Latency requirements?
   - Reliability/uptime?

4. **Who are the users?**
   - FraiseQL only?
   - General-purpose Rust community?
   - Specific industries?

5. **What features are must-have for production?**
   - TLS?
   - Connection pooling?
   - Better auth?
   - Metrics?

---

## Conclusion

fraiseql-wire has achieved MVP status with solid fundamentals:
- **Minimal scope** keeps code maintainable
- **Comprehensive testing** ensures reliability
- **Clear documentation** enables adoption
- **Production-ready design** supports growth

The path to v1.0.0 is clear, with stabilization first, then selective feature expansion based on real-world needs.

**Ready to ship! 🚀**
