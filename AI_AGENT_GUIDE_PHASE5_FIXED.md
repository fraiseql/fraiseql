# AI Agent Guide: Phase 5 Migration - GraphQL/Auth Tests

## 🤖 For: Agentic AI Systems (opencode, Claude Code, etc.)

This guide provides task specifications for AI agents to autonomously migrate ~70 GraphQL and authentication test files.

---

## 📋 Overview

**Objective:** Migrate all GraphQL and authentication test files to use `class_db_pool` architecture

**Files to Migrate:** ~70 files
- **Location A:** `tests/integration/graphql/queries/*.py` (~30 files)
- **Location B:** `tests/integration/graphql/mutations/*.py` (~15 files)
- **Location C:** `tests/integration/graphql/subscriptions/*.py` (~5 files)
- **Location D:** `tests/integration/auth/*.py` (~20 files)

**Strategy:** Batch migration in groups of 10-15 files, organized by category

**Prerequisites:** Phase 4 must be complete

---

## 🎯 Transformation Patterns (Same Core as Phase 4)

### Core Pattern (Identical to Phase 4)

The fundamental transformation is the same - see Phase 4 guide for details:

1. Update imports: add `pytest_asyncio`
2. Transform fixtures: `@pytest.fixture` → `@pytest_asyncio.fixture(scope="class")`
3. Update parameters: `db_pool` → `class_db_pool, test_schema, clear_registry_class`
4. Transform connections: `acquire/release` → `async with connection()`
5. Add schema isolation: `SET search_path TO {test_schema}, public`
6. Add class decorators: `@pytest.mark.asyncio`
7. Replace return with yield in fixtures

### Additional Phase 5 Patterns

#### Pattern A: GraphQL Schema Generation

**FIND:**
```python
@pytest.fixture(scope="module")
async def graphql_schema(db_pool):
    conn = await db_pool.acquire()
    try:
        await conn.execute("CREATE TABLE users (...)")
        from fraiseql import build_schema
        schema = build_schema()
        return schema
    finally:
        await db_pool.release(conn)
```

**REPLACE WITH:**
```python
@pytest_asyncio.fixture(scope="class")
async def graphql_schema(class_db_pool, test_schema, clear_registry_class):
    """Generate GraphQL schema with proper isolation."""
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")
        await conn.execute("CREATE TABLE users (...)")
        from fraiseql import build_schema
        schema = build_schema()
        yield schema
```

#### Pattern B: GraphQL Execution with Context

**FIND:**
```python
async def test_query(self, graphql_schema, db_pool):
    conn = await db_pool.acquire()
    try:
        result = await graphql_schema.execute(
            "query { users { id } }",
            context_value={"db": conn}
        )
    finally:
        await db_pool.release(conn)
```

**REPLACE WITH:**
```python
async def test_query(self, graphql_schema, class_db_pool, test_schema):
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")
        result = await graphql_schema.execute(
            "query { users { id } }",
            context_value={"db": conn, "schema": test_schema}
        )
```

**Note:** Add `"schema": test_schema` to context_value if GraphQL resolvers need it

#### Pattern C: FastAPI/Test Client Usage

**FIND:**
```python
async def test_endpoint(self, db_pool, test_client):
    conn = await db_pool.acquire()
    try:
        response = await test_client.post("/graphql", json={"query": "..."})
    finally:
        await db_pool.release(conn)
```

**REPLACE WITH:**
```python
async def test_endpoint(self, class_db_pool, test_schema, test_client):
    # If test_client handles connections internally, no manual conn needed
    response = await test_client.post("/graphql", json={"query": "..."})
    # Otherwise:
    # async with class_db_pool.connection() as conn:
    #     await conn.execute(f"SET search_path TO {test_schema}, public")
    #     response = await test_client.post("/graphql", json={"query": "..."})
```

#### Pattern D: WebSocket/Subscription Tests

**FIND:**
```python
async def test_subscription(self, db_pool, test_client):
    conn = await db_pool.acquire()
    try:
        async with test_client.websocket_connect("/graphql/ws") as ws:
            await ws.send_json({"type": "subscribe", "payload": {...}})
            await conn.execute("UPDATE users SET name = 'Updated'")
            message = await ws.receive_json()
    finally:
        await db_pool.release(conn)
```

**REPLACE WITH:**
```python
async def test_subscription(self, class_db_pool, test_schema, test_client):
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")
        async with test_client.websocket_connect("/graphql/ws") as ws:
            await ws.send_json({"type": "subscribe", "payload": {...}})
            await conn.execute("UPDATE users SET name = 'Updated'")
            message = await ws.receive_json()
```

#### Pattern E: Authentication Context

**FIND:**
```python
@pytest.fixture(scope="module")
async def authenticated_user(db_pool):
    conn = await db_pool.acquire()
    try:
        user_id = await conn.fetchval(
            "INSERT INTO users (email) VALUES ($1) RETURNING id",
            "test@example.com"
        )
        return {"user_id": user_id}
    finally:
        await db_pool.release(conn)
```

**REPLACE WITH:**
```python
@pytest_asyncio.fixture(scope="class")
async def authenticated_user(class_db_pool, test_schema, clear_registry_class):
    """Create authenticated user with proper isolation."""
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")
        user_id = await conn.fetchval(
            "INSERT INTO users (email) VALUES ($1) RETURNING id",
            "test@example.com"
        )
        yield {"user_id": user_id, "schema": test_schema}
```

---

## 🤖 Task Specification Template for Phase 5

```markdown
TASK: Migrate GraphQL/Auth test files to class_db_pool architecture (Batch N - Category X)

CONTEXT:
- Project: FraiseQL (GraphQL framework with PostgreSQL)
- Language: Python 3.10+
- Testing: pytest with pytest-asyncio
- Working directory: /home/lionel/code/fraiseql
- Category: [GraphQL Queries | Mutations | Subscriptions | Auth]

FILES TO MIGRATE (this batch):
1. tests/integration/graphql/queries/test_query_execution.py
2. tests/integration/graphql/queries/test_nested_queries.py
3. tests/integration/graphql/queries/test_connection_queries.py
4. tests/integration/graphql/queries/test_query_complexity.py
5. tests/integration/graphql/queries/test_query_validation.py
6. tests/integration/graphql/queries/test_query_introspection.py
7. tests/integration/graphql/queries/test_field_resolution.py
8. tests/integration/graphql/queries/test_alias_support.py
9. tests/integration/graphql/queries/test_fragment_support.py
10. tests/integration/graphql/queries/test_variable_support.py

TRANSFORMATION RULES:

CORE RULES (same as Phase 4):
1. Add import: pytest_asyncio
2. Transform fixtures: @pytest.fixture → @pytest_asyncio.fixture(scope="class")
3. Update parameters: db_pool → class_db_pool, test_schema, clear_registry_class
4. Transform connections: acquire/release → async with connection()
5. Add schema isolation: SET search_path TO {test_schema}, public
6. Add class decorators: @pytest.mark.asyncio
7. Replace return with yield in fixtures

GRAPHQL-SPECIFIC RULES:
8. GraphQL schema generation:
   - Ensure clear_registry_class is included
   - Add test_schema to fixture parameters
   - yield schema instead of return

9. GraphQL execution context:
   - Add "schema": test_schema to context_value if needed
   - Example: context_value={"db": conn, "schema": test_schema}

10. Test client usage:
    - Check if test_client already handles connections
    - If yes, remove manual connection management from test
    - If no, add async with class_db_pool.connection()

11. WebSocket tests:
    - Keep WebSocket connection inside connection block
    - Ensure database operations use same connection

12. Auth fixtures:
    - Add "schema": test_schema to returned auth data if needed
    - Ensure auth lookups can find users in test_schema

CRITICAL REQUIREMENTS:
- Every db_pool.acquire() must be replaced
- Every db_pool.release() must be replaced
- SET search_path must be added after EVERY connection acquisition
- GraphQL context must include schema if resolvers need it
- Preserve all GraphQL query strings exactly
- Preserve all test assertions exactly
- Do not modify test logic

VERIFICATION STEPS (for each file):
1. python -m py_compile <file_path>
2. grep -n "db_pool.acquire" <file_path> → should be empty
3. grep -n "db_pool.release" <file_path> → should be empty
4. grep -n "SET search_path" <file_path> → should have matches
5. uv run pytest <file_path> -v --tb=short → should pass

ACCEPTANCE CRITERIA:
- All files successfully transformed
- Zero old db_pool patterns remain
- All GraphQL queries execute correctly
- All auth checks work properly
- WebSocket tests complete without hanging
- All syntax checks pass
- All tests pass

COMPLETION SIGNAL:
Write to: /tmp/opencode-phase5-batchN-categoryX.marker
- On success: echo "SUCCESS" > /tmp/opencode-phase5-batchN-categoryX.marker
- On failure: echo "FAILURE:<reason>" > /tmp/opencode-phase5-batchN-categoryX.marker
```

---

## 📦 Recommended Batch Strategy

### Category 1: GraphQL Query Tests (3-4 batches)

**Batch 1A: Basic Queries (10 files)**
```
tests/integration/graphql/queries/test_query_execution.py
tests/integration/graphql/queries/test_query_validation.py
tests/integration/graphql/queries/test_query_complexity.py
tests/integration/graphql/queries/test_field_resolution.py
tests/integration/graphql/queries/test_nested_queries.py
tests/integration/graphql/queries/test_connection_queries.py
tests/integration/graphql/queries/test_relay_connections.py
tests/integration/graphql/queries/test_pagination_queries.py
tests/integration/graphql/queries/test_sorting_queries.py
tests/integration/graphql/queries/test_filtering_queries.py
```

**Batch 1B: Advanced Queries (10 files)**
```
tests/integration/graphql/queries/test_alias_support.py
tests/integration/graphql/queries/test_fragment_support.py
tests/integration/graphql/queries/test_variable_support.py
tests/integration/graphql/queries/test_directive_support.py
tests/integration/graphql/queries/test_introspection.py
tests/integration/graphql/queries/test_batching.py
tests/integration/graphql/queries/test_caching.py
tests/integration/graphql/queries/test_dataloader.py
tests/integration/graphql/queries/test_n_plus_one.py
tests/integration/graphql/queries/test_query_performance.py
```

**Batch 1C-1D: Remaining Query Tests (~10 files)**

### Category 2: GraphQL Mutation Tests (2 batches)

**Batch 2A: Basic Mutations (10 files)**
```
tests/integration/graphql/mutations/test_mutation_execution.py
tests/integration/graphql/mutations/test_mutation_validation.py
tests/integration/graphql/mutations/test_create_mutations.py
tests/integration/graphql/mutations/test_update_mutations.py
tests/integration/graphql/mutations/test_delete_mutations.py
tests/integration/graphql/mutations/test_upsert_mutations.py
tests/integration/graphql/mutations/test_nested_mutations.py
tests/integration/graphql/mutations/test_batch_mutations.py
tests/integration/graphql/mutations/test_transaction_mutations.py
tests/integration/graphql/mutations/test_conflict_resolution.py
```

**Batch 2B: Remaining Mutations (~5 files)**

### Category 3: Auth Tests (2 batches)

**Batch 3A: Authentication (10 files)**
```
tests/integration/auth/test_auth_enforcement.py
tests/integration/auth/test_auth0_integration.py
tests/integration/auth/test_jwt_tokens.py
tests/integration/auth/test_session_management.py
tests/integration/auth/test_csrf_protection.py
tests/integration/auth/test_rate_limiting.py
tests/integration/auth/test_password_hashing.py
tests/integration/auth/test_oauth_flow.py
tests/integration/auth/test_api_keys.py
tests/integration/auth/test_multi_factor_auth.py
```

**Batch 3B: Authorization (10 files)**
```
tests/integration/auth/test_field_authorization.py
tests/integration/auth/test_query_authorization.py
tests/integration/auth/test_mutation_authorization.py
tests/integration/auth/test_role_based_access.py
tests/integration/auth/test_permission_checks.py
tests/integration/auth/test_security_headers.py
tests/integration/auth/test_cors_handling.py
tests/integration/auth/test_token_refresh.py
tests/integration/auth/test_account_lockout.py
tests/integration/auth/test_password_reset.py
```

### Category 4: GraphQL Subscriptions (1 batch)

**Batch 4A: Subscriptions (5 files)**
```
tests/integration/graphql/subscriptions/test_subscription_execution.py
tests/integration/graphql/subscriptions/test_websocket_subscriptions.py
tests/integration/graphql/subscriptions/test_subscription_filters.py
tests/integration/graphql/subscriptions/test_subscription_auth.py
tests/integration/graphql/subscriptions/test_subscription_performance.py
```

---

## 🚀 Execution Commands

### Step 1: Generate File Lists
```bash
# GraphQL queries
find tests/integration/graphql/queries -name "test_*.py" -type f | sort > /tmp/phase5_queries.txt

# GraphQL mutations
find tests/integration/graphql/mutations -name "test_*.py" -type f | sort > /tmp/phase5_mutations.txt

# Auth tests
find tests/integration/auth -name "test_*.py" -type f | sort > /tmp/phase5_auth.txt

# Subscriptions
find tests/integration/graphql/subscriptions -name "test_*.py" -type f | sort > /tmp/phase5_subs.txt

# Count total
echo "Phase 5 files to migrate:"
wc -l /tmp/phase5_queries.txt /tmp/phase5_mutations.txt /tmp/phase5_auth.txt /tmp/phase5_subs.txt
```

### Step 2: Execute Batch (example: Batch 1A)
```bash
# Create task file
cat > /tmp/phase5_batch1A_task.txt << 'EOF'
[Insert task specification from template above with specific files for Batch 1A]
EOF

# Run with opencode
opencode run -m xai/grok-code-fast-1 "$(cat /tmp/phase5_batch1A_task.txt)" &

# Monitor
while ! test -f /tmp/opencode-phase5-batch1A-queries.marker; do
    echo "Batch 1A in progress... ($(date +%H:%M:%S))"
    sleep 15
done

# Check result
cat /tmp/opencode-phase5-batch1A-queries.marker
```

### Step 3: Verify Batch
```bash
# Verify no old patterns in batch
grep -r "db_pool.acquire" tests/integration/graphql/queries/test_query_execution.py tests/integration/graphql/queries/test_query_validation.py [... list all 10 files ...]

# Run batch tests
uv run pytest tests/integration/graphql/queries/test_query_execution.py tests/integration/graphql/queries/test_query_validation.py [... list all 10 files ...] -v
```

### Step 4: Commit Batch
```bash
git add tests/integration/graphql/queries/test_query_{execution,validation,complexity,field_resolution,nested_queries,connection_queries}.py tests/integration/graphql/queries/test_{relay_connections,pagination_queries,sorting_queries,filtering_queries}.py

git commit -m "refactor(tests): migrate Phase 5 Batch 1A - basic GraphQL query tests (10 files)

Category: GraphQL Queries
Files migrated:
- test_query_execution.py
- test_query_validation.py
- test_query_complexity.py
- test_field_resolution.py
- test_nested_queries.py
- test_connection_queries.py
- test_relay_connections.py
- test_pagination_queries.py
- test_sorting_queries.py
- test_filtering_queries.py

All tests verified passing with proper schema isolation.

Part of Phase 5 migration (Batch 1A/10)"
```

---

## 📊 Progress Tracking

```bash
# Create progress tracker
cat > /tmp/phase5_progress.sh << 'EOF'
#!/bin/bash

count_migrated() {
    local dir=$1
    local total=$(find "$dir" -name "test_*.py" | wc -l)
    local old=$(grep -l "db_pool.acquire" "$dir" --include="test_*.py" 2>/dev/null | wc -l)
    local migrated=$((total - old))
    echo "$migrated/$total"
}

echo "Phase 5 Progress:"
echo "  Queries:       $(count_migrated tests/integration/graphql/queries)"
echo "  Mutations:     $(count_migrated tests/integration/graphql/mutations)"
echo "  Subscriptions: $(count_migrated tests/integration/graphql/subscriptions)"
echo "  Auth:          $(count_migrated tests/integration/auth)"
EOF

chmod +x /tmp/phase5_progress.sh

# Run anytime
/tmp/phase5_progress.sh
```

---

## ✅ Category-Specific Success Criteria

### For GraphQL Query Tests
- ✅ All queries execute successfully
- ✅ GraphQL context includes test_schema
- ✅ Schema generation uses clear_registry_class
- ✅ No connection leaks
- ✅ Results are consistent across runs

### For GraphQL Mutation Tests
- ✅ All mutations execute successfully
- ✅ Transaction handling works correctly
- ✅ Conflict resolution functions properly
- ✅ Database changes isolated to test_schema

### For Auth Tests
- ✅ User lookup finds users in test_schema
- ✅ Token generation includes schema context
- ✅ Authorization checks use correct schema
- ✅ No auth data leakage between test classes

### For Subscription Tests
- ✅ WebSocket connections establish successfully
- ✅ Subscriptions receive updates
- ✅ Database triggers fire in correct schema
- ✅ No connection timeouts
- ✅ Cleanup happens properly

---

## 🎯 Final Verification (All Batches Complete)

```bash
echo "=== Phase 5 Final Verification ==="

# 1. Check all categories for old patterns
echo "Checking GraphQL queries..."
QUERIES_OLD=$(grep -l "db_pool.acquire" tests/integration/graphql/queries --include="test_*.py" | wc -l)

echo "Checking GraphQL mutations..."
MUTATIONS_OLD=$(grep -l "db_pool.acquire" tests/integration/graphql/mutations --include="test_*.py" | wc -l)

echo "Checking GraphQL subscriptions..."
SUBS_OLD=$(grep -l "db_pool.acquire" tests/integration/graphql/subscriptions --include="test_*.py" | wc -l)

echo "Checking auth tests..."
AUTH_OLD=$(grep -l "db_pool.acquire" tests/integration/auth --include="test_*.py" | wc -l)

TOTAL_OLD=$((QUERIES_OLD + MUTATIONS_OLD + SUBS_OLD + AUTH_OLD))
echo ""
echo "Old patterns remaining: $TOTAL_OLD (should be 0)"

# 2. Run all Phase 5 tests
if [ $TOTAL_OLD -eq 0 ]; then
    echo "Running all GraphQL tests..."
    uv run pytest tests/integration/graphql/ -v --tb=short

    echo "Running all Auth tests..."
    uv run pytest tests/integration/auth/ -v --tb=short

    echo "Verifying isolation (second run)..."
    uv run pytest tests/integration/graphql/ tests/integration/auth/ -v --tb=short

    # 3. Final commit
    echo "✅ Phase 5 Complete - Creating final commit..."
    git add tests/integration/graphql/ tests/integration/auth/
    git commit -m "refactor(tests): complete Phase 5 migration - all GraphQL/Auth tests use class_db_pool

Migrated ~70 files across 10 batches:
- GraphQL query tests: ~30 files (4 batches)
- GraphQL mutation tests: ~15 files (2 batches)
- Auth tests: ~20 files (2 batches)
- GraphQL subscription tests: ~5 files (1 batch)

✅ All tests passing with proper schema isolation
✅ Zero old db_pool patterns remain
✅ WebSocket/subscription tests working correctly
✅ Auth context properly isolated
✅ GraphQL schema generation uses clean registry

Phase 5 of 5 complete - MIGRATION FINISHED! 🎉

Total migration accomplishment:
- Phase 1: Hanging tests (6 files)
- Phase 2: Vector/ML (3 files)
- Phase 3: Enterprise (14 files)
- Phase 4: Database (80 files)
- Phase 5: GraphQL/Auth (70 files)
Total: ~173 files successfully migrated!"

    echo ""
    echo "╔════════════════════════════════════════╗"
    echo "║  🎉 MIGRATION COMPLETE! 🎉            ║"
    echo "║  All 5 phases finished successfully!  ║"
    echo "╚════════════════════════════════════════╝"
else
    echo "❌ Migration incomplete - $TOTAL_OLD old patterns remain"
    echo "Check individual categories above for details"
    exit 1
fi
```

---

## 🤖 AI Agent Best Practices for Phase 5

### Pattern Recognition
1. **Identify GraphQL-specific patterns** before transforming
2. **Check for context_value usage** in GraphQL execution
3. **Detect WebSocket connections** and keep them inside connection blocks
4. **Find auth token generation** and add schema context

### Careful Handling
1. **WebSocket tests** - these are tricky, test thoroughly
2. **Auth lookups** - ensure they can find users in test_schema
3. **GraphQL resolvers** - may need schema parameter in context
4. **Subscription triggers** - ensure they fire in correct schema

### Testing Strategy
1. **Test subscriptions individually first** (most likely to have issues)
2. **Verify auth tests don't leak** data between runs
3. **Check GraphQL introspection** still works
4. **Run category tests twice** to verify isolation

---

## 📝 Summary for AI Orchestrators

**Task:** Migrate ~70 GraphQL and auth test files
**Method:** Category-based batch processing (10 batches total)
**Complexity:** Higher than Phase 4 (GraphQL context, WebSockets, Auth)
**Verification:** More thorough testing required
**Duration:** ~1.5-2 hours of AI agent time per batch
**Total Time:** 15-20 hours for all 10 batches

**Dependencies:** Phase 4 must be complete
**Output:** All GraphQL/Auth tests using class_db_pool with proper isolation
**Completion:** Marks end of entire migration (Phases 1-5 complete!)
