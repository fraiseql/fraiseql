# HTTP Server Integration Plan

**Date:** January 14, 2026
**Goal:** Complete HTTP server implementation with schema loading, request handling, and full integration testing
**Status:** Planning Phase 0 from completion roadmap
**Scope:** 2-3 days

---

## Current State Assessment

**What's Done:**

- ✅ Server infrastructure (Axum-based, routes defined)
- ✅ Configuration system (ServerConfig with sensible defaults)
- ✅ Middleware (CORS, tracing)
- ✅ GraphQL request/response types defined
- ✅ Handler skeleton (routes/graphql.rs has handler signature)
- ✅ Core Executor exists in fraiseql-core

**What's Missing:**

- ❌ Schema loading from JSON files (critical)
- ❌ Complete graphql_handler implementation
- ❌ Health/introspection endpoints
- ❌ Integration tests
- ❌ Binary main.rs implementation
- ❌ Error handling and response formatting

---

## Implementation Strategy

### Phase 0.1: Schema Loading Module (0.5 days)

**Files to Create/Modify:**

- `crates/fraiseql-server/src/schema/mod.rs` (NEW)
- `crates/fraiseql-server/src/schema/loader.rs` (NEW)
- `crates/fraiseql-server/src/lib.rs` (MODIFY - add schema module)

**Implementation Details:**

1. Create `CompiledSchemaLoader` struct:

   ```rust
   pub struct CompiledSchemaLoader {
       path: PathBuf,
   }

   impl CompiledSchemaLoader {
       pub fn new(path: impl AsRef<Path>) -> Self { ... }
       pub async fn load(&self) -> Result<CompiledSchema> { ... }
   }
   ```

2. Load schema from JSON file:
   - Read file asynchronously
   - Parse JSON into CompiledSchema
   - Handle file not found, JSON parse errors
   - Return Result with proper error messages

3. Add to ServerConfig:
   - New field: `schema_path: PathBuf`
   - Default to "schema.compiled.json"
   - Validate path on server startup

**Tests:**

- Unit test: Load valid schema JSON
- Unit test: Handle missing file error
- Unit test: Handle invalid JSON error

---

### Phase 0.2: Complete GraphQL Handler (0.5 days)

**Files to Modify:**

- `crates/fraiseql-server/src/routes/graphql.rs` (COMPLETE)
- `crates/fraiseql-server/src/server.rs` (ENHANCE)

**Implementation Details:**

1. Update graphql_handler to:
   - ✅ Already parses GraphQL request (query, variables, operation_name)
   - ✅ Already calls executor.execute()
   - ✅ Already returns GraphQL response
   - **Need to add:** Proper error handling and response formatting

2. Fix error handling:
   - Map FraiseQLError to GraphQL error format
   - Ensure GraphQL-compliant error responses
   - Add location tracking for parse errors

3. Enhance ExecutionError variant:
   - Include operation name in logs
   - Track execution time
   - Handle timeout scenarios

**Tests:**

- Unit test: Valid query execution
- Unit test: Query with variables
- Unit test: Invalid query (parse error)
- Unit test: Unknown operation error

---

### Phase 0.3: Health & Introspection Endpoints (0.5 days)

**Files to Modify:**

- `crates/fraiseql-server/src/routes/health.rs` (ENHANCE)
- `crates/fraiseql-server/src/routes/introspection.rs` (ENHANCE)
- `crates/fraiseql-server/src/routes/mod.rs` (if needed)

**Implementation Details:**

1. Health endpoint (`GET /health`):
   - Return 200 OK with status JSON
   - Include schema loaded status
   - Include database connection status
   - Format: `{ "status": "healthy", "schema": "loaded", "database": "connected" }`

2. Introspection endpoint (`GET /introspection`):
   - Return schema introspection data
   - Include available types, queries, mutations
   - Useful for client code generation

**Tests:**

- Integration test: Health check on startup
- Integration test: Health check with no schema
- Integration test: Introspection returns valid schema

---

### Phase 0.4: Main Binary Implementation (0.5 days)

**Files to Modify:**

- `crates/fraiseql-server/src/main.rs` (COMPLETE)

**Implementation Details:**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize tracing (already done)

    // 2. Load configuration from:
    //    - Environment variable: FRAISEQL_CONFIG
    //    - Default config file: fraiseql.toml
    //    - Fall back to defaults

    // 3. Validate paths exist:
    //    - Schema file (config.schema_path)
    //    - Database connection string

    // 4. Load compiled schema
    let loader = CompiledSchemaLoader::new(&config.schema_path);
    let schema = loader.load().await?;

    // 5. Create database adapter
    // (will need to support multiple database types)

    // 6. Create and start server
    let server = Server::new(config, schema, adapter);
    server.serve().await?;

    Ok(())
}
```

**Database Adapter Initialization:**

- Accept DATABASE_URL environment variable
- Parse connection string to determine database type
- Create appropriate adapter (PostgresAdapter, etc.)
- Support connection pooling

**Tests:**

- Integration test: Server startup with valid config
- Integration test: Server rejects invalid schema path
- Integration test: Server rejects invalid database URL

---

### Phase 0.5: Integration Tests (0.5 days)

**Files to Create:**

- `crates/fraiseql-server/tests/integration_test.rs` (NEW)
- `crates/fraiseql-server/tests/fixtures/` (NEW - test schemas)

**Test Scenarios:**

1. Server Startup Tests:
   - Server starts with valid schema and database
   - Server rejects missing schema file
   - Server rejects invalid JSON schema
   - Server logs proper startup messages

2. Query Execution Tests:
   - Simple SELECT query returns results
   - Query with variables works correctly
   - Query with named operation works
   - Multiple concurrent requests handled

3. Error Handling Tests:
   - Invalid GraphQL query returns error
   - Database error returns GraphQL error
   - Server 500 on internal panic (graceful degradation)

4. Endpoint Tests:
   - GraphQL endpoint responds to POST
   - Health endpoint returns status
   - Introspection endpoint returns schema
   - Unknown endpoint returns 404

5. Load Tests:
   - 100 concurrent requests
   - Request timeout handling
   - Connection pool behavior

**Test Database:**

- Use test fixtures with sample schema
- Create minimal test data
- Clean up after each test

---

## Database Adapter Selection

The server needs to determine which database adapter to use. Strategy:

1. **Environment Variable:** `DATABASE_URL` format indicates database type:
   - `postgresql://...` → PostgresAdapter
   - `mysql://...` → MySQLAdapter
   - `sqlite://...` → SqliteAdapter
   - `mssql://...` → SqlServerAdapter

2. **Configuration File:** Optional `[database]` section in TOML:

   ```toml
   [database]
   type = "postgres"
   url = "postgresql://localhost/fraiseql"
   pool_size = 20
   ```

3. **Fallback:** Default to PostgreSQL for development

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Schema file not found on startup | CRITICAL | Validate path at startup, provide helpful error message |
| Invalid JSON schema | CRITICAL | Parse and validate before creating Executor |
| Database connection fails | CRITICAL | Test connection at startup, provide clear error |
| Parser panics on malformed GraphQL | HIGH | Add panic handler, return error instead |
| Memory leak in schema caching | MEDIUM | Use Arc<RwLock<>> correctly, no circular refs |
| Executor.execute() hangs | MEDIUM | Add timeout mechanism to queries |

---

## Success Criteria

✅ Server starts successfully with valid schema and database
✅ GraphQL endpoint returns correct results for queries
✅ Health endpoint reports status accurately
✅ Error responses follow GraphQL specification
✅ 100+ concurrent requests handled without issues
✅ All integration tests pass
✅ No compiler warnings (clippy clean)
✅ Documentation is complete

---

## Dependency Changes

**No new dependencies needed.** Existing stack:

- ✅ `axum` - HTTP server (already present)
- ✅ `tokio` - async runtime (already present)
- ✅ `serde`/`serde_json` - JSON parsing (already present)
- ✅ `fraiseql-core` - execution engine (already present)
- ✅ `tracing` - logging (already present)

---

## Implementation Order

**Recommended sequence to minimize blockers:**

1. **Create schema loader** (used by everything else)
2. **Complete graphql_handler** (core functionality)
3. **Implement health/introspection endpoints** (supporting features)
4. **Write integration tests** (catch integration bugs early)
5. **Implement main.rs** (ties everything together)

This order allows early validation that the core request → schema → executor → response pipeline works correctly.

---

## Files to Create/Modify Summary

| File | Action | Priority |
|------|--------|----------|
| `src/schema/mod.rs` | CREATE | HIGH |
| `src/schema/loader.rs` | CREATE | HIGH |
| `src/lib.rs` | MODIFY | HIGH |
| `src/routes/graphql.rs` | ENHANCE | HIGH |
| `src/server.rs` | ENHANCE | MEDIUM |
| `src/routes/health.rs` | ENHANCE | MEDIUM |
| `src/routes/introspection.rs` | ENHANCE | MEDIUM |
| `src/main.rs` | COMPLETE | MEDIUM |
| `tests/integration_test.rs` | CREATE | HIGH |
| `tests/fixtures/` | CREATE | MEDIUM |
