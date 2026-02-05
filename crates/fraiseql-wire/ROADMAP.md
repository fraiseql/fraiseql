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

### Status: ✅ COMPLETE (All 7.1-7.6 phases finished)

### Tasks

#### 7.1 Performance Profiling & Optimization ✅ COMPLETE

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

##### 7.1.3 Comparison Benchmarks (vs tokio-postgres) ✅

- [x] Set up tokio-postgres comparison suite
- [x] Connection setup comparison (TCP vs Unix socket)
- [x] Query execution overhead comparison
- [x] Protocol overhead comparison (minimal vs full feature)
- [x] JSON parsing performance comparison
- [x] Memory usage comparison (critical: O(chunk_size) vs O(result_size))
- [x] Feature completeness matrix
- [x] Comprehensive COMPARISON_GUIDE.md documentation
- [x] Manual/pre-release execution only (not in CI)

**Status**: Complete - 6 benchmark groups with full market positioning analysis

Key Finding: fraiseql-wire achieves 1000x-20000x memory savings for large result sets

##### 7.1.4 Documentation & Optimization ✅

- [x] Profile hot paths with flamegraph (not needed: micro-benchmarks show negligible overhead)
- [x] Optimize identified bottlenecks (optimization: use SQL predicates to reduce network)
- [x] Update README with benchmark results
- [x] Create performance tuning guide
- [x] Publish baseline results in CHANGELOG

**Status**: Complete - Comprehensive performance tuning guide with practical guidance for production use

**Deliverables**:

- PERFORMANCE_TUNING.md: ~450 lines of practical optimization guidance
- README.md: Updated with benchmarked performance tables
- CHANGELOG.md: Phase 7.1 completion with key results
- Key finding: WHERE clause optimization is most important (10-100x throughput gains)

#### 7.2 Security Audit ✅

- [x] Review all unsafe code
  - ✅ ZERO unsafe code found
  - Safety guaranteed by Rust type system

- [x] Authentication review
  - ✅ CleartextPassword properly implemented
  - ✅ No credential leakage
  - ✅ Error messages safe

- [x] Connection validation
  - ✅ TLS can be added safely
  - ✅ No connection hijacking issues
  - ✅ Cancellation mechanism safe (process_id + secret_key validation)

- [x] Dependencies audit
  - ✅ `cargo audit`: 157 crates, 0 vulnerabilities
  - ✅ All dependencies current (January 2026)
  - ✅ Recommendations for version pinning

**Status**: Complete - Comprehensive security audit passed

**Deliverables**:

- SECURITY_AUDIT.md: Detailed technical audit (~500 lines)
- SECURITY.md: User security guidance (~300 lines)
- Key finding: Zero critical/high-severity issues; TLS required for production TCP
- Verdict: ✅ PASS - Ready for Phase 7.3

#### 7.3 Real-World Testing ✅ COMPLETE

- [x] Set up staging database for testing
  - ✅ Created schema with 4 entity tables (projects, users, tasks, documents)
  - ✅ Seeded realistic data (JSON sizes: 1KB to 100KB+)
  - ✅ Support for 1K, 100K, 1M row test sets

- [x] Load testing
  - ✅ 10 test scenarios covering various workloads
  - ✅ Throughput measurements (100K-500K rows/sec)
  - ✅ Memory scaling tests with chunk size analysis
  - ✅ Sustained streaming and connection management tests

- [x] Stress testing
  - ✅ 20+ stress test scenarios
  - ✅ Connection drops, invalid strings, empty results
  - ✅ Edge cases and error recovery testing

**Deliverables**: TESTING_GUIDE.md (500+ lines), load_tests.rs, stress_tests.rs

#### 7.4 Error Message Refinement ✅ COMPLETE

- [x] Audit all error messages
  - ✅ Added 4 new helper methods to FraiseError
  - ✅ Enhanced context and actionability

- [x] Document common error scenarios
  - ✅ 7 major error categories documented
  - ✅ Connection, auth, query, schema, performance, network errors

- [x] Create TROUBLESHOOTING.md
  - ✅ 1400+ lines of detailed error diagnosis
  - ✅ 30+ specific error scenarios with solutions
  - ✅ Diagnostic commands and verification steps

**Deliverables**: TROUBLESHOOTING.md (1400+ lines), enhanced src/error.rs, error tests

#### 7.5 CI/CD Improvement ✅ COMPLETE

- [x] GitHub Actions enhancements
  - ✅ Code coverage reporting (tarpaulin + Codecov)
  - ✅ Security audit in CI (cargo audit)
  - ✅ MSRV testing (Rust 1.70)
  - ✅ Integration tests with Postgres 15

- [x] Docker improvements
  - ✅ Multi-platform builds (amd64, arm64)
  - ✅ Optimized Dockerfile.fraiseql-wire
  - ✅ Enhanced docker-compose.yml with auto-initialization

- [x] Release automation
  - ✅ Release workflow (.github/workflows/release.yml)
  - ✅ Automated crates.io publishing
  - ✅ Release script (scripts/publish.sh) with full automation

**Deliverables**: ci.yml, release.yml, Dockerfile, docker-compose.yml, scripts/publish.sh, CI_CD_GUIDE.md

#### 7.6 Documentation Polish ✅ COMPLETE

- [x] API documentation review
  - ✅ All public APIs have doc comments
  - ✅ Documentation builds with zero warnings

- [x] Create example programs
  - ✅ examples/basic_query.rs
  - ✅ examples/filtering.rs
  - ✅ examples/ordering.rs
  - ✅ examples/streaming.rs
  - ✅ examples/error_handling.rs
  - ✅ All 5 examples compile without errors

- [x] Update README.md
  - ✅ Added Quick Start section with installation
  - ✅ Added Examples and Running Examples sections
  - ✅ Added Documentation & Guides index

- [x] Update CONTRIBUTING.md
  - ✅ Added CI/CD Workflows section
  - ✅ Added Release Process automation details

- [x] Create QUICK_START.md
  - ✅ 700+ lines of getting started guide
  - ✅ Installation, first program, next steps, troubleshooting

**Deliverables**: QUICK_START.md, 5 example programs, updated README.md & CONTRIBUTING.md

---

## Phase 8: Feature Additions (v0.1.x patch releases)

### Goal

Add requested features to v0.1.0 while maintaining API stability and backward compatibility.

### Status: 📋 Planning Phase - Ready to Begin

### Implementation Approach

**Version Strategy**: Stay at v0.1.0 with feature additions in patch releases (v0.1.1, v0.1.2, etc.)

- Each feature is optional and backward compatible
- Existing code continues to work without changes
- New code can opt-in to new features as needed

### Recommended Feature Priority

Based on typical production needs, we recommend prioritizing in this order:

1. **8.1 TLS Support** (Medium effort) - Required for cloud/remote Postgres
   - Optional: `FraiseClient::connect_tls(config)` alongside existing `connect()`
   - Release: v0.1.1

2. **8.3 Connection Configuration** (Low effort) - Better timeout/keepalive control
   - Optional: `connect_with_config()` alongside existing `connect()`
   - Release: v0.1.1 or v0.1.2

3. **8.5 Query Metrics** (Low-Medium effort) - Production observability
   - Optional: `stream.metrics()` after query execution
   - Release: v0.1.2

4. **8.2 Typed Streaming** (Medium effort) - Type safety improvement
   - Optional: Generic `query::<T>()`
   - Release: v0.1.3+ if needed

5. **8.4 SCRAM Authentication** (Medium effort) - Better security than cleartext
   - Optional: `AuthMethod` enum alongside cleartext
   - Release: v0.1.3+ if needed

6. **8.6 Connection Pooling** (High effort) - Complex but requested
   - Recommend: Separate crate `fraiseql-pool` for v1.0+

### Implementation Strategy

Each feature will:

1. Be implemented in a feature branch or main (depending on stability)
2. Include comprehensive tests and documentation
3. Get peer review before merging to main
4. Bump patch version (v0.1.x) when released
5. Maintain 100% backward compatibility with v0.1.0

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

### Stabilization

- Performance benchmarks established
- Real-world testing completed
- Zero critical issues
- Security audit passed

### v1.0.0

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
