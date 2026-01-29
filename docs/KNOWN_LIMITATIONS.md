# FraiseQL Known Limitations

**Last Updated**: 2026-01-29

This document outlines known limitations of FraiseQL Phase 16 and provides context for future development phases.

---

## Phase 16 Scope & Limitations

### 1. Authentication & Authorization

**Status**: Basic OIDC support only
**Impact**: No fine-grained role-based access control (RBAC)
**Current Implementation**:
- OIDC token validation
- Bearer token authentication
- Basic token extraction from GraphQL extensions

**Limitations**:
- ❌ No field-level permission enforcement
- ❌ No row-level security (RLS)
- ❌ No custom authorization middleware
- ❌ No attribute-based access control (ABAC)

**Workaround**: Implement authorization in application layer before calling GraphQL endpoint

**Future**: Phase 17+ will add field-level authorization and RLS support

---

### 2. Advanced Caching Strategies

**Status**: Basic in-memory result caching only
**Impact**: Limited performance optimization options

**Current Implementation**:
- Query result caching with TTL
- APQ (Automatic Persisted Queries) support
- No external cache backend

**Limitations**:
- ❌ No Redis cache backend
- ❌ No distributed caching
- ❌ No cache coherency across instances
- ❌ No cache warming/preloading

**Workaround**: Use connection pooling and database indexes for optimization

**Future**: Phase 18 will add Redis support and cache warming

---

### 3. Arrow Flight Integration

**Status**: Partial implementation (stub only)
**Impact**: Alternative execution engine not available

**Current**:
- Arrow Flight service stub exists
- Not fully integrated with query execution
- Passes protocol validation but does not execute queries

**Limitations**:
- ❌ Arrow queries cannot execute
- ❌ Flight data plane integration incomplete
- ❌ Column-oriented execution not available

**Workaround**: Use SQL-based GraphQL execution (primary path, fully functional)

**Future**: Phase 17+ will complete Arrow Flight integration

**Note**: This is a low-priority limitation as SQL-based execution is high-performance and suitable for most use cases.

---

### 4. Real-Time Subscriptions

**Status**: WebSocket transport ready, GraphQL subscriptions not implemented
**Impact**: Real-time push updates not available

**Current**:
- WebSocket transport layer implemented
- GraphQL subscription protocol partially implemented
- No event streaming backend

**Limitations**:
- ❌ No subscription execution
- ❌ No event streaming from database
- ❌ No live query support

**Workaround**: Use polling for periodic updates or implement custom webhooks

**Future**: Phase 19 will implement GraphQL subscriptions with event streaming

---

### 5. Custom Middleware & Webhooks

**Status**: Not implemented
**Impact**: Cannot execute custom logic on requests/responses

**Current**:
- Middleware infrastructure exists
- No custom webhook support
- No pre/post-request hooks

**Limitations**:
- ❌ No custom request transformations
- ❌ No webhook execution on mutations
- ❌ No event-driven workflows

**Workaround**: Implement custom endpoints alongside GraphQL server

**Future**: Phase 18+ will add webhook support

---

### 6. File Upload Support

**Status**: Not implemented
**Impact**: Cannot accept file uploads in mutations

**Current**: No file upload handling

**Limitations**:
- ❌ No multipart form data support
- ❌ No S3/cloud storage integration
- ❌ No file transformation pipeline

**Workaround**: Accept file uploads via separate REST endpoint, store reference

**Future**: Phase 18+ will implement file upload support

---

### 7. Advanced Observability

**Status**: Basic structured logging and tracing
**Impact**: Limited visibility into performance and errors

**Current**:
- Distributed tracing with OpenTelemetry
- Structured logging with tracing crate
- Basic metrics collection

**Limitations**:
- ❌ No APM integration (DataDog, New Relic, etc.)
- ❌ No automatic performance profiling
- ❌ No error tracking integration (Sentry, etc.)
- ❌ No service maps or dependency graphs
- ❌ Limited observability for federation chains

**Workaround**: Export traces to OpenTelemetry-compatible backend (Jaeger, etc.)

**Future**: Phase 17+ will add APM and error tracking integrations

---

### 8. Database Features

#### 8.1 Connection Pooling

**Status**: Implemented via deadpool/pgbouncer
**Impact**: All databases support connection pooling

**Current**: Full support for connection pooling configuration

---

#### 8.2 Transaction Management

**Status**: Not supported for GraphQL operations
**Impact**: Individual mutations are atomic per subgraph, not across federation

**Current**:
- Saga system provides distributed transaction semantics
- Individual database operations are atomic
- Federation mutations are not ACID-compliant across services

**Limitations**:
- ❌ No multi-service ACID transactions
- ❌ No distributed locks
- ❌ Saga-based compensation is best-effort

**Workaround**: Use sagas for distributed transactions with compensation

**Future**: Phase 20+ will explore multi-service transactions

---

#### 8.3 Database Support

**Status**: PostgreSQL (primary), MySQL (secondary), SQLite, SQL Server
**Impact**: All major databases supported

**Limitations**:
- ⚠️ MySQL: Limited full-text search support
- ⚠️ SQLite: No advanced window functions
- ⚠️ SQL Server: Limited JSON operators
- ❌ Oracle: No Rust driver available

---

### 9. Federation Features

#### 9.1 Nested @extends

**Status**: One level only
**Impact**: Cannot extend extended types

**Current**: Direct inheritance supported
- Type A defines @key
- Type B extends A
- Cannot extend B in another service

**Limitations**:
- ❌ No multiple levels of extension
- ❌ No complex inheritance chains

**Workaround**: Flatten inheritance structure in schema design

**Future**: Phase 16+ enhancement

---

#### 9.2 Interface Federation

**Status**: Not supported
**Impact**: Cannot federate GraphQL interfaces

**Current**: Only object types can be federated

**Limitations**:
- ❌ Cannot extend interfaces
- ❌ No interface-based federation patterns

**Workaround**: Use concrete types instead of interfaces

**Future**: Phase 17+

---

#### 9.3 Union Federation

**Status**: Not supported
**Impact**: Cannot federate union types

**Limitations**:
- ❌ Cannot extend unions

**Workaround**: Use interfaces or concrete types

**Future**: Phase 17+

---

### 10. Saga System Limitations

#### 10.1 Saga Durability

**Status**: Durable across process restarts
**Impact**: Sagas survive server crashes

**Current**: Full implementation

---

#### 10.2 Idempotency

**Status**: Supported via request_id/transactionId
**Impact**: Safe to retry saga steps

**Current**: Full implementation with idempotency key support

---

#### 10.3 Nested Sagas

**Status**: Not supported
**Impact**: Cannot invoke sagas from within sagas

**Current**: Top-level sagas only

**Limitations**:
- ❌ No recursive saga composition
- ❌ Cannot chain sagas programmatically

**Workaround**: Implement multi-step saga instead of nested sagas

**Future**: Phase 20+

---

### 11. Performance Limitations

#### 11.1 Entity Resolution Latency

**Baseline Performance** (Phase 16):
- Local (same database, indexed key): <5ms
- Direct database (different service): <20ms
- HTTP subgraph: <200ms

**Not Optimized For**:
- ❌ Millions of entity references per query
- ❌ Unindexed key lookups
- ❌ High-latency subgraph networks

**Workaround**:
- Add database indexes on @key fields
- Use connection pooling
- Optimize subgraph latency

**Future**: Phase 17+ will add batch entity resolution

---

#### 11.2 Result Set Size

**Current**: No hard limit on result size
**Impact**: Large result sets consume memory

**Limitations**:
- ⚠️ Full result buffering (no streaming responses yet)
- ❌ No pagination enforced at schema level

**Workaround**: Implement pagination in schema design

**Future**: Phase 18+ will add streaming responses

---

### 12. Schema Evolution

**Status**: Basic support
**Impact**: Schema changes require careful management

**Current**:
- Can add new types/fields
- Can deprecate fields
- Schema composition must be recompiled

**Limitations**:
- ❌ No zero-downtime schema updates
- ❌ No schema versioning
- ❌ Breaking changes require service restart

**Workaround**: Plan schema updates during maintenance windows

**Future**: Phase 19+ will add schema versioning and migration tools

---

## Intentional Design Decisions

### Why These Limitations?

1. **Phase-Based Development**: Features are implemented in phases to ensure quality and testing
2. **Scope Management**: Focus on federation & sagas before adding peripheral features
3. **Performance**: Avoid premature optimization
4. **Production Readiness**: Only Phase 16 core features guaranteed for production

---

## Testing & Stability

### What's Guaranteed Stable (Phase 16)

✅ **Stable APIs** (will not change):
- GraphQL execution with federation
- Saga coordination and compensation
- Database connection management
- Entity resolution protocol

✅ **Stable Formats** (backward compatible):
- Compiled schema JSON format
- GraphQL query/mutation execution results
- Saga execution semantics

---

### What's Not Guaranteed Stable (Phase 16)

⚠️ **Internal APIs** (may change):
- Internal Rust trait definitions
- Configuration schema (additions OK, breaking changes possible)
- Observability event structure

⚠️ **Experimental Features**:
- Arrow Flight integration
- Custom middleware (if added in Phase 17)
- Advanced caching (if added in Phase 18)

---

## Workarounds & Alternatives

| Limitation | Workaround | Alternative |
|-----------|-----------|-------------|
| No field-level RBAC | Application-layer auth | Wait for Phase 17 |
| No Redis caching | Use DB indexes + pooling | Wait for Phase 18 |
| No subscriptions | Polling or webhooks | Wait for Phase 19 |
| No file upload | Separate REST endpoint | Wait for Phase 18 |
| No nested sagas | Multi-step saga design | Wait for Phase 20 |
| No streaming responses | Pagination in schema | Wait for Phase 18 |

---

## Migration Path: Current → Future

**Phase 16 → Phase 17** (Code Quality Review):
- Security audit
- Performance optimization
- Arrow Flight completion
- Field-level authorization

**Phase 17 → Phase 18** (Advanced Features):
- Redis caching
- File uploads
- Webhooks
- Advanced logging

**Phase 18 → Phase 19** (Real-Time):
- GraphQL subscriptions
- Event streaming
- Schema versioning

**Phase 20+** (Enterprise):
- Nested sagas
- Multi-service transactions
- Advanced federation patterns

---

## Reporting Limitations

If you encounter a limitation not listed here, please:

1. Check this document for context
2. Review the Phase roadmap in `.phases/`
3. Open a GitHub issue with:
   - Current limitation
   - Use case requiring the feature
   - Suggested Phase for implementation

---

## Questions?

- See [FAQ.md](./FAQ.md) for frequently asked questions
- See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for common issues
- See [MIGRATION_PHASE_15_TO_16.md](./MIGRATION_PHASE_15_TO_16.md) for upgrade guidance
- See [PHASE_16_READINESS.md](./PHASE_16_READINESS.md) for Phase 16 completion status

---

**Document Owner**: FraiseQL Federation Team
**Last Reviewed**: 2026-01-29
**Next Review**: Upon completion of Phase 17
