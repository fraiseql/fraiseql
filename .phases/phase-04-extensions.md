# Phase 4: Extension Features

**Status**: 📋 PLANNED (After Phase 3)
**Objective**: Complete optional features and integrations
**Expected Duration**: 4-5 days

---

## Success Criteria

- [ ] Arrow Flight analytics integration fully tested
- [ ] Observer system hardened and optimized
- [ ] MySQL adapter fully functional
- [ ] SQLite adapter fully functional
- [ ] SQL Server adapter fully functional
- [ ] Wire protocol improvements implemented
- [ ] All optional features documented
- [ ] Feature flags working correctly

---

## Objective

Phases 1-3 established the core: architecture, correctness, and performance. Phase 4 completes the optional features:

1. Arrow Flight for analytics workloads
2. Observer system for webhooks/events
3. Multiple database backends
4. Wire protocol for PostgreSQL compatibility

---

## TDD Cycles

### Cycle 1: Arrow Flight Completion

**Objective**: Complete and test Arrow Flight integration for analytics

**RED Phase** ✓
- Write failing tests for:
  - Arrow Flight server startup
  - Flight RPC endpoints (GetFlightInfo, DoGet, DoPut)
  - Schema metadata transfer
  - Batch serialization/deserialization
  - Large dataset handling (>1M rows)
  - Multi-client concurrent access
  - Error handling (invalid queries, timeouts)
  - Client libraries (Python PyArrow, R arrow, Rust)

**GREEN Phase**
- Implement Arrow Flight endpoints
- Handle flight protocol handshake
- Serialize schema metadata
- Stream columnar data
- Support concurrent clients

**REFACTOR Phase**
- Improve code organization
- Extract reusable utilities
- Better error handling
- Optimize serialization

**CLEANUP Phase**
- Fix warnings
- Format code
- Commit with test results

### Cycle 2: Observer System

**Objective**: Harden and optimize observer/webhook system

**RED Phase** ✓
- Write failing tests for:
  - Event filtering and routing
  - Webhook delivery and retry
  - Deadletter queue (DLQ) for failed events
  - Event batching and deduplication
  - Action execution (email, SMS, webhook, Slack)
  - Error recovery and resilience
  - Event ordering guarantees
  - Memory/performance under load

**GREEN Phase**
- Implement event filtering
- Add webhook delivery retry
- Implement DLQ
- Add action executors
- Optimize event processing

**REFACTOR Phase**
- Improve action abstraction
- Better error categorization
- Optimize batching strategy
- Add monitoring hooks

**CLEANUP Phase**
- Fix warnings
- Format code
- Document observer patterns
- Commit with metrics

### Cycle 3: Database Backends

**Objective**: Support multiple databases (MySQL, SQLite, SQL Server)

**RED Phase** ✓
- Write failing tests for each database:
  - Connection pooling
  - Query execution
  - Transaction handling
  - Data type mappings
  - Schema inspection
  - Error handling
  - Performance baselines

**GREEN Phase**
- Implement adapters for:
  - MySQL (with SQLx)
  - SQLite (with rusqlite)
  - SQL Server (with SQLx)
- Handle database-specific SQL dialects
- Map GraphQL types to native types
- Implement transactions

**REFACTOR Phase**
- Extract common adapter code
- Improve type mapping
- Better error handling
- Optimize queries per database

**CLEANUP Phase**
- Fix warnings
- Format code
- Document adapter requirements
- Commit with adapter tests

### Cycle 4: Wire Protocol

**Objective**: Add PostgreSQL wire protocol compatibility

**RED Phase** ✓
- Write failing tests for:
  - Wire protocol message parsing
  - Query message handling
  - Result set transmission
  - Error responses
  - Extended query protocol
  - Prepared statement caching
  - Connection state management

**GREEN Phase**
- Implement wire protocol
- Handle query messages
- Send result sets in wire format
- Implement error frames
- Support prepared statements

**REFACTOR Phase**
- Improve message parsing
- Better state management
- Optimize wire format encoding
- Add streaming support

**CLEANUP Phase**
- Fix warnings
- Format code
- Document wire protocol
- Commit with protocol tests

### Cycle 5: Feature Flags & Documentation

**Objective**: Complete feature flag implementation and documentation

**RED Phase** ✓
- Write failing tests for:
  - Feature flag compilation
  - Runtime feature detection
  - Feature interaction matrix
  - Documentation building with different features
  - Performance impact of features

**GREEN Phase**
- Verify feature flags work:
  - `[feature = "arrow"]` enables Arrow Flight
  - `[feature = "observers"]` enables event system
  - `[feature = "wire"]` enables PostgreSQL compatibility
- Build documentation for each feature
- Create feature matrix

**REFACTOR Phase**
- Improve feature documentation
- Better feature interaction documentation
- Examples for each feature

**CLEANUP Phase**
- Fix warnings
- Format code
- Commit with feature matrix

---

## Feature Details

### Arrow Flight (`[feature = "arrow"]`)

**What**: High-performance columnar data delivery

**When to use**:
- Analytics workloads (>100K rows)
- Cross-language integration (Python, R, Java)
- Real-time dashboards
- Large dataset exports

**Implementation**:
- Arrow Flight gRPC service
- Columnar encoding
- Efficient memory layout
- Compression options
- Multi-client streaming

**Performance target**:
- > 100K rows/sec
- 15-50x faster than JSON
- < 1MB per 1M rows

### Observers (`[feature = "observers"]`)

**What**: Event-driven actions (webhooks, email, etc.)

**When to use**:
- Sending notifications (email, SMS, Slack)
- Triggering external workflows
- Audit logging
- Real-time integrations

**Implementation**:
- Event filtering and routing
- Action executors (webhook, email, SMS, Slack)
- Retry logic with exponential backoff
- Deadletter queue (DLQ)
- Event deduplication

**Performance target**:
- > 1K events/sec
- < 100ms event latency
- < 100KB per event

### Multi-Database Support

**What**: Support for PostgreSQL, MySQL, SQLite, SQL Server

**When to use**:
- PostgreSQL: Production, federation
- MySQL: Legacy systems, scaling
- SQLite: Development, embedded
- SQL Server: Enterprise deployments

**Implementation**:
- Trait-based `DatabaseAdapter`
- Per-database SQL generation
- Type mapping layer
- Transaction handling

**Target databases**:
- ✅ PostgreSQL (primary)
- ✅ MySQL (secondary)
- ✅ SQLite (development)
- ✅ SQL Server (enterprise)

### Wire Protocol (`[feature = "wire"]`)

**What**: PostgreSQL wire protocol compatibility

**When to use**:
- Drop-in replacement for PostgreSQL
- Compatibility with PostgreSQL tools
- Driver support (psycopg2, pg, pgx, etc.)
- Query editors (DBeaver, pgAdmin)

**Implementation**:
- Wire protocol v3 support
- Query message handling
- Result set encoding
- Extended query protocol (prepared statements)
- Error frame encoding

**Target tools**:
- psycopg2 / psycopg3 (Python)
- pg (Node.js)
- pgx (Rust)
- JDBC (Java)
- Any PostgreSQL-compatible driver

---

## Files to Create/Update

### Arrow Flight
- `crates/fraiseql-arrow/src/flight_server.rs` (implementation)
- `crates/fraiseql-arrow/tests/flight_integration.rs` ✨
- `docs/arrow-flight-guide.md` (documentation)

### Observers
- `crates/fraiseql-observers/src/event_system.rs` (core)
- `crates/fraiseql-observers/src/actions/` (email, webhook, SMS, Slack)
- `crates/fraiseql-observers/tests/observer_integration.rs` ✨
- `docs/observers-guide.md` (documentation)

### Multi-Database
- `crates/fraiseql-core/src/db/mysql.rs` ✨
- `crates/fraiseql-core/src/db/sqlite.rs` ✨
- `crates/fraiseql-core/src/db/sqlserver.rs` ✨
- `tests/db_adapter_tests.rs` ✨
- `docs/database-adapters.md` (documentation)

### Wire Protocol
- `crates/fraiseql-wire/src/protocol.rs` (implementation)
- `crates/fraiseql-wire/tests/wire_protocol.rs` ✨
- `docs/wire-protocol-guide.md` (documentation)

---

## Definition of Done

Phase 4 is complete when:

1. ✅ Arrow Flight fully implemented and tested
2. ✅ Observer system hardened and documented
3. ✅ All database adapters working
4. ✅ Wire protocol compatible with PostgreSQL drivers
5. ✅ Feature flags functioning correctly
6. ✅ Code clean with no warnings
7. ✅ Documentation complete for all features

---

## Next Phase

**Phase 5: Production Hardening** focuses on:
- Security audit and remediation
- Dependency updates
- Performance monitoring
- Observability (OpenTelemetry)

See `.phases/phase-05-hardening.md` for details.

---

## Notes

- Features are optional via Cargo flags
- Core should work without any features enabled
- Each feature should have tests
- Document when to use each feature
- Consider resource usage of features

---

**Phase 4 will be started after Phase 3 completion.**
