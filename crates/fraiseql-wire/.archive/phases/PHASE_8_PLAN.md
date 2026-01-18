# Phase 8: Feature Additions (v0.1.x patch releases)

**Status**: Ready to Begin
**Target**: Add production-ready features to v0.1.0 while maintaining backward compatibility
**Version Strategy**: Stay at v0.1.0 with patch releases (v0.1.1, v0.1.2, v0.1.3, etc.)
**Approach**: Each feature is optional and can be adopted independently

---

## Overview

After completing Phase 7 (Stabilization), fraiseql-wire has:

- ✅ Solid performance benchmarks
- ✅ Comprehensive test coverage
- ✅ Production security audit passed
- ✅ Excellent documentation

Phase 8 focuses on **feature expansion** based on production feedback and common use cases. We recommend implementing features **in priority order**, starting with the most impactful.

---

## Feature Priority Matrix

### Priority 1: TLS Support (8.1)

**Impact**: 🔴 **Critical** - Required for cloud/remote deployments
**Effort**: 🟡 **Medium** (~1-2 weeks)
**Complexity**: Medium (TLS negotiation, certificate handling)
**Blocker**: No - current cleartext OK for development/internal
**Users**: Cloud users (AWS, GCP, Azure), corporate networks
**Recommendation**: **START HERE**

### Priority 2: Connection Configuration (8.3)

**Impact**: 🟢 **High** - Better control over timeouts/keepalive
**Effort**: 🟢 **Low** (~3-5 days)
**Complexity**: Low (API expansion)
**Blocker**: No - defaults work for most cases
**Users**: All (better defaults, production SLAs)
**Recommendation**: **Quick win after TLS**

### Priority 3: Query Metrics (8.5)

**Impact**: 🟢 **High** - Essential for observability
**Effort**: 🟡 **Low-Medium** (~1 week)
**Complexity**: Low-Medium (metrics collection)
**Blocker**: No - tracing works but metrics preferred
**Users**: Production operators, monitoring systems
**Recommendation**: **Implement with TLS**

### Priority 4: Typed Streaming (8.2)

**Impact**: 🟡 **Medium** - Nice to have for type safety
**Effort**: 🟡 **Medium** (~1-2 weeks)
**Complexity**: Medium (generic trait bounds)
**Blocker**: No - JSON approach works
**Users**: Strongly-typed applications
**Recommendation**: **After TLS + Metrics**

### Priority 5: SCRAM Authentication (8.4)

**Impact**: 🟡 **Medium** - Security improvement
**Effort**: 🟡 **Medium** (~2 weeks)
**Complexity**: High (authentication protocol)
**Blocker**: No - Cleartext acceptable with TLS
**Users**: High-security environments
**Recommendation**: **If user demands it**

### Priority 6: Connection Pooling (8.6)

**Impact**: 🟢 **High** - Common production need
**Effort**: 🔴 **High** (~4-6 weeks)
**Complexity**: High (state management, concurrency)
**Blocker**: No - separate crate possible
**Users**: Application servers
**Recommendation**: **Defer to separate crate**

---

## Recommended Implementation Plan

### Release Schedule (v0.1.x patch releases)

#### v0.1.1: TLS + Connection Config - Weeks 1-2

**Deliverables**:

1. **TLS Support** (`FraiseClient::connect_tls`)
   - rustls backend (cross-platform, pure Rust)
   - Certificate validation
   - Optional client certificates
   - Tests with self-signed certs
   - Example program: `examples/tls.rs`

2. **Connection Configuration** (Optional)
   - `FraiseClient::connect_with_config(config)` or similar
   - Timeout settings (connect_timeout, statement_timeout)
   - Keepalive options
   - Application name
   - Tests for all options
   - Example program: `examples/config.rs`

**Release Notes**: Add optional TLS support with system root certs by default. Requires no changes to existing code.

#### v0.1.2: Query Metrics - Week 3

**Deliverables**:

1. **Query Metrics**
   - Per-query metrics (rows, bytes, duration, throughput)
   - `stream.metrics()` API after execution
   - Integration with tracing
   - Benchmarks showing < 1% overhead
   - Example program: `examples/metrics.rs`

**Release Notes**: Add optional query metrics for production monitoring. Zero overhead if unused.

#### v0.1.3+: Typed Streaming & SCRAM (If Needed)

**Deliverables** (Defer if not requested):

1. **Typed Streaming** - Optional generic `query::<T>()`
2. **SCRAM Auth** - Optional `AuthMethod` enum for better security

---

## Feature Specifications

### 8.1: TLS Support

#### API

```rust
use fraiseql_wire::client::{FraiseClient, TlsConfig};

// Connect with TLS
let tls = TlsConfig::builder()
    .ca_cert_path("/path/to/ca.pem")?
    .verify_hostname(true)
    .build()?;

let client = FraiseClient::connect_tls("postgres://localhost/db", tls).await?;
```

#### Implementation Details

- **Library**: rustls (pure Rust, no OpenSSL required)
- **Features**:
  - Server certificate validation
  - CA certificate configuration
  - Optional client certificates
  - Hostname verification

- **Connection Flow**:
  1. TCP connection established
  2. TLS handshake with Postgres
  3. Authentication (cleartext or future SCRAM)
  4. Query execution

- **Configuration**:
  - Default: Verify server certificate
  - Option: Skip verification (dev only)
  - Option: Custom CA bundle
  - Option: Client certificates

#### Tests

- [ ] Connect with valid certificate
- [ ] Reject self-signed (default)
- [ ] Accept self-signed (with config)
- [ ] Certificate validation errors
- [ ] Connection string parsing with `tls://` scheme
- [ ] Performance: TLS overhead < 5% latency

#### Files to Create/Modify

- `src/connection/tls.rs` - TLS handling (new)
- `src/client/tls_config.rs` - TLS configuration (new)
- `src/client/mod.rs` - Add `connect_tls` method
- `tests/tls_integration.rs` - TLS integration tests (new)

---

### 8.3: Connection Configuration

#### API

```rust
use fraiseql_wire::client::ConnectionConfig;
use std::time::Duration;

let config = ConnectionConfig::builder()
    .connect_timeout(Duration::from_secs(10))
    .statement_timeout(Duration::from_secs(30))
    .keepalive_idle(Duration::from_secs(60))
    .application_name("my_app")
    .build()?;

let client = FraiseClient::connect_with_config("postgres://localhost/db", config).await?;
```

#### Implementation Details

- **Configuration Options**:
  - `connect_timeout`: TCP connection timeout
  - `statement_timeout`: Query timeout
  - `keepalive_idle`: TCP keepalive interval
  - `application_name`: Postgres application_name
  - `extra_float_digits`: Float precision (Postgres setting)

- **Defaults**:
  - `connect_timeout`: 10 seconds
  - `statement_timeout`: None (unlimited)
  - `keepalive_idle`: 5 minutes
  - `application_name`: "fraiseql-wire"

#### Tests

- [ ] All options apply correctly
- [ ] Timeout triggers on slow connection
- [ ] Statement timeout kills long queries
- [ ] Keepalive prevents idle disconnects
- [ ] Connection string still works without config

#### Files to Create/Modify

- `src/connection/config.rs` - Connection configuration (new)
- `src/client/mod.rs` - Add `connect_with_config` method
- `tests/config_integration.rs` - Config tests (new)

---

### 8.5: Query Metrics

#### API

```rust
let mut stream = client
    .query("projects")
    .where_sql("status='active'")
    .execute()
    .await?;

let mut count = 0;
while let Some(item) = stream.next().await {
    let _json = item?;
    count += 1;
}

// Get metrics
let metrics = stream.metrics();
println!("Rows: {}", metrics.row_count);
println!("Bytes: {}", metrics.bytes_received);
println!("Duration: {:?}", metrics.duration);
println!("Throughput: {:.0} rows/sec", metrics.throughput());
```

#### Implementation Details

- **Metrics Collected**:
  - `row_count`: Total rows streamed
  - `bytes_received`: Total bytes from Postgres
  - `duration`: Query elapsed time
  - `connection_setup_time`: Time to establish connection
  - `first_row_time`: Time to first row

- **Metrics Structure**:

```rust
pub struct QueryMetrics {
    pub row_count: u64,
    pub bytes_received: u64,
    pub duration: Duration,
    pub connection_setup_time: Duration,
    pub first_row_time: Duration,
}

impl QueryMetrics {
    pub fn throughput(&self) -> f64 { /* rows per second */ }
    pub fn bandwidth(&self) -> f64 { /* megabytes per second */ }
}
```

- **Integration with Tracing**:
  - Emit span events with metrics
  - Compatible with `tracing-subscriber`
  - Low overhead (< 1% performance impact)

#### Tests

- [ ] Metrics collected accurately
- [ ] Throughput calculation correct
- [ ] Tracing integration works
- [ ] Zero overhead for unobserved metrics
- [ ] Large result sets tracked correctly

#### Files to Create/Modify

- `src/stream/metrics.rs` - Query metrics (new)
- `src/stream/mod.rs` - Add metrics collection
- `tests/metrics_integration.rs` - Metrics tests (new)
- `examples/metrics.rs` - Metrics example (new)

---

### 8.2: Typed Streaming

#### API

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
    status: String,
}

// Type-safe streaming
let mut stream = client
    .query::<Project>("projects")
    .where_sql("status='active'")
    .execute()
    .await?;

while let Some(result) = stream.next().await {
    match result {
        Ok(project) => println!("Project: {}", project.name),
        Err(e) => eprintln!("Error: {}", e), // Deserialization errors
    }
}
```

#### Implementation Details

- **Generic Query Builder**:
  - `QueryBuilder<T: DeserializeOwned>`
  - Preserves all filtering/ordering APIs
  - Automatic JSON→T conversion

- **Error Handling**:
  - Type mismatch errors with details
  - Missing fields clearly reported
  - Backward compatible (serde_json::Value still works)

- **Performance**:
  - Zero-copy JSON parsing where possible
  - Minimal overhead vs `serde_json::Value`

#### Trade-offs

- **Pro**: Better type safety, compile-time guarantees
- **Con**: Requires Serde knowledge, more strict types
- **Decision**: Optional - JSON approach still supported

#### Tests

- [ ] Type-safe deserialization works
- [ ] Type mismatch errors are clear
- [ ] Performance matches serde_json::Value
- [ ] Examples with common types (User, Project, etc.)

#### Files to Create/Modify

- `src/client/query_builder.rs` - Refactor to support generics
- `src/stream/typed_stream.rs` - Typed stream implementation (new)
- `examples/typed_streaming.rs` - Typed example (new)
- `tests/typed_integration.rs` - Typed tests (new)

---

### 8.4: SCRAM Authentication

#### API

```rust
use fraiseql_wire::client::AuthMethod;

let client = FraiseClient::connect_with_auth(
    "postgres://user@localhost/db",
    AuthMethod::SCRAM {
        password: "secret".to_string(),
        mechanism: "SCRAM-SHA-256", // or "SCRAM-SHA-1"
    }
).await?;
```

#### Implementation Details

- **Supported Mechanisms**:
  - SCRAM-SHA-256 (recommended)
  - SCRAM-SHA-1 (legacy)
  - Cleartext (existing)

- **Protocol Flow**:
  1. Client sends SCRAM method list
  2. Postgres chooses mechanism
  3. Client/server exchange challenges
  4. Mutual authentication verification

- **Complexity**:
  - HMAC-SHA256 implementations
  - Base64 encoding/decoding
  - Salt handling
  - Iteration count (PBKDF2)

#### Trade-offs

- **Pro**: Better security than cleartext
- **Con**: More dependencies, complex protocol
- **Decision**: Defer if cleartext + TLS sufficient

#### Tests

- [ ] SCRAM-SHA-256 works with Postgres
- [ ] Fallback to cleartext on protocol mismatch
- [ ] Invalid credentials rejected
- [ ] Performance impact minimal

---

### 8.6: Connection Pooling (Deferred)

**Recommendation**: Implement as separate crate `fraiseql-pool`

#### Rationale

1. **Scope**: Adding pooling to fraiseql-wire violates "one query per connection"
2. **Complexity**: Pool management, connection state, thread safety
3. **Flexibility**: Users may prefer different pool implementations
4. **Maintenance**: Separate crate easier to evolve independently

#### Possible Future Design

```rust
// Separate crate: fraiseql-pool
let pool = fraiseql_pool::PoolBuilder::new("postgres://localhost/db")
    .max_size(10)
    .min_idle(2)
    .build()
    .await?;

let client = pool.get().await?;
let stream = client.query("projects").execute().await?;
// Connection returned to pool on drop
```

---

## Implementation Strategy

### For Each Feature

1. **Design Phase** (1-2 days)
   - Write API sketches
   - Document design trade-offs
   - Get feedback from team

2. **Implementation** (3-10 days depending on feature)
   - Code implementation
   - Comprehensive unit tests
   - Integration tests

3. **Documentation** (1-2 days)
   - API documentation
   - Example programs
   - CHANGELOG entry
   - Guide/tutorial if needed

4. **Review & Verification** (1-2 days)
   - Performance benchmarks
   - Security review
   - Documentation review

5. **Release** (1 day)
   - Bump version
   - Create GitHub release
   - Publish to crates.io
   - Announce

### Testing Strategy

Each feature gets:

- Unit tests (in-memory, no Postgres)
- Integration tests (with Postgres)
- Example programs (user-facing verification)
- Performance benchmarks (regression detection)
- Error case tests (edge conditions)

### Documentation Requirements

Each feature must document:

- **API**: Full rustdoc with examples
- **Guide**: How/when to use the feature
- **Examples**: Runnable example programs
- **Performance**: Benchmarks and trade-offs
- **Security**: Any security implications
- **FAQ**: Common questions

---

## Feature Dependencies

```
TLS Support (8.1)
  └─ No dependencies

Connection Config (8.3)
  └─ No dependencies

Query Metrics (8.5)
  └─ Optional: Tracing integration

Typed Streaming (8.2)
  ├─ Depends on: Serde ecosystem
  └─ Can be added independently

SCRAM Auth (8.4)
  ├─ Depends on: Ring/Sha2 for crypto
  ├─ Optional: Can coexist with cleartext
  └─ No blocker

Connection Pooling (8.6)
  └─ Future: Separate crate
```

---

## Success Criteria

### Per Feature

Each completed feature must:

- ✅ Have > 90% test coverage
- ✅ Build with zero clippy warnings
- ✅ Have complete rustdoc (zero missing docs)
- ✅ Have at least 1 example program
- ✅ Pass all CI checks
- ✅ Be benchmarked (if performance-critical)

### Overall Phase 8 Success

- ✅ v0.2.0 released to crates.io
- ✅ All recommended features (TLS, Config, Metrics) complete
- ✅ Zero critical bugs reported
- ✅ Community feedback positive
- ✅ Ready for Phase 9 (production readiness)

---

## Timeline Estimate

### Release v0.1.1 (TLS + Config)

| Feature | Estimate | Status |
|---------|----------|--------|
| TLS Support | 1-2 weeks | ✅ Foundation in place |
| Connection Config | 3-5 days | 📋 Planned |
| Testing & Examples | 3-4 days | 📋 Planned |
| **Total for v0.1.1** | **2-3 weeks** | **Ready to start** |

### Release v0.1.2 (Metrics)

| Feature | Estimate | Status |
|---------|----------|--------|
| Query Metrics | 1 week | 📋 Planned |
| Testing & Examples | 3-4 days | 📋 Planned |
| **Total for v0.1.2** | **1-2 weeks** | **Dependent on v0.1.1** |

### Deferred (v0.1.3+)

| Feature | Estimate | Status |
|---------|----------|--------|
| Typed Streaming | 1-2 weeks | ⏳ If requested |
| SCRAM Auth | 2 weeks | ⏳ If requested |
| **Grand Total (all features)** | **4-6 weeks** | **v0.1.0 + all opt-ins** |

---

## Next Steps

1. **Select Priority Features** - Confirm TLS/Config/Metrics priority
2. **Design Review** - Sketch APIs with team
3. **Create Implementation Plan** - Break into PRs
4. **Start Development** - TLS first (critical for production)
5. **Iterate** - Ship v0.2.0 with completed features
6. **Gather Feedback** - Plan Phase 9 based on usage

---

## Related Documentation

- **ROADMAP.md** - Overall project timeline
- **CONTRIBUTING.md** - Development workflow
- **SECURITY.md** - Security considerations
- **PERFORMANCE_TUNING.md** - Performance guidelines
- **CI_CD_GUIDE.md** - CI/CD workflows

---

**Ready to begin Phase 8! 🚀**
