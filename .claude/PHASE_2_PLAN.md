# Phase 2: Database & Cache Implementation Plan

## Overview

Phase 2 implements database connectivity and query result caching for the FraiseQL HTTP server. The core database abstraction already exists in `fraiseql-core`, so this phase focuses on:
1. Integrating existing database adapters into the server
2. Implementing connection pooling strategies
3. Adding query result caching with coherency
4. Database health checks and monitoring

## Current State Analysis

### Existing Infrastructure (Phase 0-1)
- ✅ `fraiseql-core/src/db/traits.rs` - DatabaseAdapter trait
- ✅ `fraiseql-core/src/db/postgres/adapter.rs` - PostgreSQL implementation
- ✅ `fraiseql-core/src/cache/` - Cache modules (config, adapter, invalidation, etc.)
- ✅ `fraiseql-server/src/routes/graphql.rs` - GraphQL handler (ready for executor)
- ✅ `fraiseql-server/src/server.rs` - Server setup

### Missing Integration (Phase 2 Tasks)
- ❌ Database adapter initialization in server startup
- ❌ Connection pooling integration
- ❌ Query result caching setup
- ❌ Cache invalidation hooks
- ❌ Health check integration with database

## Implementation Plan

### Phase 2.1: Database Adapter Integration
**Goal**: Wire database adapters into the HTTP server startup

**Tasks**:
1. Update `fraiseql-server/src/main.rs` to initialize database adapter from URL
2. Add database URL configuration to `ServerConfig`
3. Create `fraiseql_core::db::postgres::PostgresAdapter` instance
4. Wire adapter into `Server::new()` and `Executor`
5. Update `AppState` to use real adapter instead of mock

**Files to Modify**:
- `crates/fraiseql-server/src/config.rs` - Add `database_url` field
- `crates/fraiseql-server/src/main.rs` - Initialize adapter from DATABASE_URL env var
- `crates/fraiseql-server/src/server.rs` - Accept Arc<Adapter> in Server::new()

**Expected Output**:
- Server can connect to PostgreSQL database
- Health check endpoint returns actual database status
- GraphQL queries can be executed against real database views

### Phase 2.2: Connection Pooling
**Goal**: Implement connection pooling for PostgreSQL

**Tasks**:
1. Configure deadpool-postgres for connection pooling
2. Set pool size, timeout, and recycling parameters
3. Add pool metrics to health endpoint
4. Implement connection pool monitoring

**Files to Modify**:
- `crates/fraiseql-server/src/config.rs` - Add pool config (min_size, max_size, timeout)
- `crates/fraiseql-server/src/routes/health.rs` - Include pool metrics

**Expected Output**:
- Configurable connection pool sizes
- Health endpoint shows active/idle connections
- Pool metrics tracked and logged

### Phase 2.3: Query Result Caching
**Goal**: Implement query-level caching with coherency

**Tasks**:
1. Integrate `fraiseql_core::cache::CacheAdapter` into executor
2. Cache key generation from GraphQL query + variables
3. Cache invalidation on data mutations
4. TTL-based cache eviction

**Files to Modify**:
- `crates/fraiseql-server/src/routes/graphql.rs` - Check cache before execute
- `crates/fraiseql-server/src/config.rs` - Add cache config (TTL, max_size)

**Expected Output**:
- Query results cached in memory
- Cache hit/miss metrics in logging
- Automatic eviction after TTL

### Phase 2.4: Integration Tests
**Goal**: Test database and cache functionality end-to-end

**Tasks**:
1. Create test database with sample schema
2. Test GraphQL queries against real database
3. Test caching behavior (hits, misses, invalidation)
4. Test connection pool behavior (concurrent requests)
5. Test health check with real database

**Files to Create**:
- `crates/fraiseql-server/tests/database_integration_test.rs`

**Expected Output**:
- Full end-to-end tests with real PostgreSQL
- Cache behavior verified
- Connection pool stress tests

## Architecture Decisions

### 1. Database Initialization
```rust
// Read from environment
let db_url = env::var("DATABASE_URL")
    .unwrap_or_else(|_| "postgresql://localhost/fraiseql".to_string());

// Create adapter
let adapter = Arc::new(PostgresAdapter::new(&db_url).await?);

// Pass to server
let server = Server::new(config, schema, adapter);
```

### 2. Connection Pooling
- Use `deadpool-postgres` (already in Cargo.toml)
- Default: min_size=5, max_size=20
- Configurable via env vars or config file

### 3. Caching Strategy
- In-memory cache for small deployments
- TTL-based eviction (default: 5 minutes)
- Cache key: SHA256(query + variables)
- Coherency: Invalidate on mutations

## Testing Strategy

### Unit Tests
- Database adapter initialization
- Cache key generation
- Invalidation logic

### Integration Tests
- Real PostgreSQL queries
- Connection pool metrics
- Cache hit/miss scenarios
- Concurrent request handling

## Success Criteria

✅ Server connects to PostgreSQL database
✅ GraphQL queries execute against real views
✅ Connection pooling working with metrics
✅ Query results cached with TTL
✅ Health check returns database status
✅ All integration tests passing
✅ No warnings in cargo build

## Timeline & Dependencies

**Phase 2.1**: Database Adapter Integration (1 session)
- Depends on: Phase 0 (HTTP server) completion
- Enables: Phase 2.2, 2.3

**Phase 2.2**: Connection Pooling (1 session)
- Depends on: Phase 2.1
- Enables: Phase 2.3

**Phase 2.3**: Query Result Caching (1 session)
- Depends on: Phase 2.1, 2.2
- Enables: Full operational server

**Phase 2.4**: Integration Tests (1 session)
- Depends on: Phase 2.1, 2.2, 2.3
- Final validation

## Notes

- PostgreSQL adapter already exists and is tested
- Cache modules exist but need executor integration
- No database migrations needed for basic queries
- Schema must be pre-compiled and deployed
- Sample PostgreSQL setup provided in fraiseql-core tests
