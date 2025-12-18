# Phase 4: Integration - Complete GraphQL Pipeline

**Phase**: 4 of 5
**Effort**: 8 hours
**Status**: Blocked until Phase 3 complete
**Prerequisite**: Phases 1-3 complete
**Companion Docs**: FEATURE-FLAGS.md, TESTING_STRATEGY.md

---

## Objective

Integrate all components into complete GraphQL query/mutation execution pipeline:
1. Integrate streaming results with JSON transformation
2. Implement mutations in Rust (INSERT, UPDATE, DELETE)
3. Complete end-to-end GraphQL query → HTTP response
4. Validate parity with psycopg backend
5. Performance validation (20-30% improvement)

**Success Criteria**:
- ✅ All GraphQL queries execute end-to-end in Rust
- ✅ All mutations work correctly (with transactions)
- ✅ All 5991+ tests pass with Rust backend
- ✅ Parity tests pass (Rust == psycopg output)
- ✅ Performance: 20-30% faster than psycopg
- ✅ Memory usage: 10-15% lower

---

## Architecture Overview

### Complete Query Pipeline

```
GraphQL Query (from Client)
    ↓
FastAPI Endpoint (Python)
    ├─ Parse query (graphql-core)
    ├─ Validate schema
    ├─ Extract QueryDef
    ├─ Prepare parameters
    ↓
Single Async Call → Rust (via PyO3)
    ├─ Acquire connection from pool (Arc<Pool>)
    ├─ Build WHERE clause (from filter)
    ├─ Build SELECT SQL
    ├─ Execute query (streaming)
    ├─ Transform results (snake_case → camelCase)
    ├─ Build GraphQL response JSON
    ↓
Response bytes (streaming)
    ↓
HTTP Response (200 OK)
```

### Complete Mutation Pipeline

```
GraphQL Mutation (from Client)
    ↓
FastAPI Endpoint (Python)
    ├─ Parse mutation
    ├─ Validate schema
    ├─ Extract MutationDef
    ├─ Prepare input variables
    ↓
Single Async Call → Rust (via PyO3)
    ├─ Acquire connection from pool
    ├─ BEGIN transaction
    ├─ Build INSERT/UPDATE/DELETE SQL
    ├─ Execute mutation (with parameters)
    ├─ Execute post-mutation query (to get final state)
    ├─ Transform results
    ├─ Build GraphQL response
    ├─ COMMIT transaction
    ↓
Response bytes (mutation result)
    ↓
HTTP Response (200 OK)
```

---

## Implementation Details

### Step 1: Consolidate Python Layer

**File**: `src/fraiseql/core/rust_pipeline.py` (NEW - Unified interface)

```python
"""Unified Rust database pipeline for all operations"""

from typing import Dict, Any, Optional, List
from _fraiseql_rs import execute_query_async, execute_mutation_async
import asyncio

class RustGraphQLPipeline:
    """Complete GraphQL query/mutation execution via Rust"""

    async def execute_query(self, query_def: Dict[str, Any]) -> Dict[str, Any]:
        """
        Execute GraphQL query via Rust backend.

        Args:
            query_def: {
                'operation': 'query',
                'table': 'users',
                'fields': ['id', 'name', 'email'],
                'filters': {...},  # WHERE clause
                'pagination': {'limit': 10, 'offset': 0},
                'sort': [{'field': 'name', 'direction': 'ASC'}]
            }

        Returns:
            {
                'data': {...},
                'errors': None
            }
        """
        try:
            result = await execute_query_async(query_def)
            return {'data': result, 'errors': None}
        except Exception as e:
            return {
                'data': None,
                'errors': [{'message': str(e), 'extensions': {'code': 'INTERNAL_ERROR'}}]
            }

    async def execute_mutation(self, mutation_def: Dict[str, Any]) -> Dict[str, Any]:
        """
        Execute GraphQL mutation via Rust backend.

        Args:
            mutation_def: {
                'operation': 'mutation',
                'type': 'insert' | 'update' | 'delete',
                'table': 'users',
                'input': {...},  # Data to insert/update
                'filters': {...},  # WHERE clause for update/delete
                'return_fields': ['id', 'name', 'email']
            }

        Returns:
            {
                'data': {'createUser': {...}},
                'errors': None
            }
        """
        try:
            result = await execute_mutation_async(mutation_def)
            return {'data': result, 'errors': None}
        except Exception as e:
            return {
                'data': None,
                'errors': [{'message': str(e), 'extensions': {'code': 'INTERNAL_ERROR'}}]
            }


# Global instance
pipeline = RustGraphQLPipeline()
```

---

### Step 2: Concrete Query Resolver Examples

**File**: `src/fraiseql/resolvers/users.py`

```python
"""User query resolvers - examples of integration"""

from fraiseql.core.rust_pipeline import pipeline

async def resolve_user(obj, info, id: int):
    """Resolve single user query: query { user(id: 1) { id, name, email } }"""

    query_def = {
        'operation': 'query',
        'table': 'users',
        'fields': ['id', 'name', 'email', 'created_at'],
        'filters': {
            'field': 'id',
            'operator': 'eq',
            'value': id
        }
    }

    result = await pipeline.execute_query(query_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    # Result is list, return first item
    data = result['data']
    return data[0] if data else None


async def resolve_users(obj, info, limit: int = 10, offset: int = 0, sort_by: str = 'name'):
    """Resolve users list query: query { users(limit: 10) { id, name, email } }"""

    query_def = {
        'operation': 'query',
        'table': 'users',
        'fields': ['id', 'name', 'email', 'created_at'],
        'filters': None,  # No WHERE clause
        'pagination': {'limit': limit, 'offset': offset},
        'sort': [{'field': sort_by, 'direction': 'ASC'}]
    }

    result = await pipeline.execute_query(query_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return result['data']


async def resolve_users_by_domain(obj, info, domain: str):
    """Resolve users filtered by email domain"""

    query_def = {
        'operation': 'query',
        'table': 'users',
        'fields': ['id', 'name', 'email'],
        'filters': {
            'field': 'email',
            'operator': 'like',
            'value': f'%@{domain}'
        }
    }

    result = await pipeline.execute_query(query_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return result['data']


async def resolve_users_with_complex_filter(obj, info, filter_input: dict):
    """Resolve users with complex nested filters"""

    # Handle complex GraphQL input type
    filters = _convert_graphql_filter(filter_input)

    query_def = {
        'operation': 'query',
        'table': 'users',
        'fields': ['id', 'name', 'email', 'is_active'],
        'filters': filters  # Complex AND/OR/NOT structure
    }

    result = await pipeline.execute_query(query_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return result['data']


def _convert_graphql_filter(graphql_filter: dict) -> dict:
    """Convert GraphQL filter input to Rust query filter"""
    # Implementation depends on your GraphQL filter schema
    # Example: { and: [{ field: 'is_active', eq: true }, { field: 'created_at', gte: '2025-01-01' }] }
    return graphql_filter
```

---

### Step 3: Concrete Mutation Resolver Examples

**File**: `src/fraiseql/resolvers/mutations.py`

```python
"""Mutation resolvers - examples of integration"""

from fraiseql.core.rust_pipeline import pipeline
from datetime import datetime

async def resolve_create_user(obj, info, input: dict):
    """Create user mutation: mutation { createUser(input: {name, email}) { id, name, email } }"""

    mutation_def = {
        'operation': 'mutation',
        'type': 'insert',
        'table': 'users',
        'input': {
            'name': input['name'],
            'email': input['email'],
            'is_active': input.get('is_active', True),
            'created_at': datetime.utcnow().isoformat()
        },
        'return_fields': ['id', 'name', 'email', 'is_active', 'created_at']
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return result['data']


async def resolve_update_user(obj, info, id: int, input: dict):
    """Update user mutation: mutation { updateUser(id: 1, input: {name}) { id, name, email } }"""

    mutation_def = {
        'operation': 'mutation',
        'type': 'update',
        'table': 'users',
        'filters': {
            'field': 'id',
            'operator': 'eq',
            'value': id
        },
        'input': {
            key: value for key, value in input.items()
            if value is not None  # Only update provided fields
        },
        'return_fields': ['id', 'name', 'email', 'is_active', 'updated_at']
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return result['data']


async def resolve_delete_user(obj, info, id: int):
    """Delete user mutation: mutation { deleteUser(id: 1) { success, message } }"""

    mutation_def = {
        'operation': 'mutation',
        'type': 'delete',
        'table': 'users',
        'filters': {
            'field': 'id',
            'operator': 'eq',
            'value': id
        },
        'return_fields': None  # No need to return deleted record
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    return {'success': True, 'message': f'User {id} deleted'}


async def resolve_bulk_update_users(obj, info, filter_input: dict, input: dict):
    """Bulk update users matching filter"""

    filters = _convert_graphql_filter(filter_input)

    mutation_def = {
        'operation': 'mutation',
        'type': 'update',
        'table': 'users',
        'filters': filters,  # Can be complex filter
        'input': input,
        'return_fields': ['id', 'name', 'email', 'updated_at']
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result['errors']:
        raise Exception(result['errors'][0]['message'])

    # Result is list of updated records
    updated_count = len(result['data']) if result['data'] else 0
    return {
        'success': True,
        'updated_count': updated_count,
        'records': result['data']
    }


def _convert_graphql_filter(graphql_filter: dict) -> dict:
    """Convert GraphQL filter to Rust query filter"""
    return graphql_filter
```

---

### Step 4: Rust-Side Mutation Execution

**File**: `fraiseql_rs/src/mutations/mod.rs` (NEW)

```rust
//! Mutation execution module (INSERT, UPDATE, DELETE)

use tokio_postgres::Client;
use serde_json::json;

pub enum MutationType {
    Insert,
    Update,
    Delete,
}

pub async fn execute_mutation(
    client: &Client,
    mutation_type: MutationType,
    table: &str,
    input: &serde_json::Value,
    filters: Option<&serde_json::Value>,
    return_fields: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    match mutation_type {
        MutationType::Insert => insert_record(client, table, input, return_fields).await,
        MutationType::Update => update_record(client, table, input, filters, return_fields).await,
        MutationType::Delete => delete_record(client, table, filters).await,
    }
}

async fn insert_record(
    client: &Client,
    table: &str,
    input: &serde_json::Value,
    return_fields: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    // Build INSERT SQL
    let (sql, params) = build_insert_sql(table, input)?;

    // Execute with transaction
    let transaction = client.transaction()
        .await
        .map_err(|e| e.to_string())?;

    let rows = transaction.query(&sql, &[])
        .await
        .map_err(|e| e.to_string())?;

    transaction.commit()
        .await
        .map_err(|e| e.to_string())?;

    // Return inserted record
    transform_rows_to_json(&rows, return_fields)
}

async fn update_record(
    client: &Client,
    table: &str,
    input: &serde_json::Value,
    filters: Option<&serde_json::Value>,
    return_fields: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    // Build UPDATE SQL with WHERE clause
    let (sql, _params) = build_update_sql(table, input, filters)?;

    let transaction = client.transaction()
        .await
        .map_err(|e| e.to_string())?;

    let rows = transaction.query(&sql, &[])
        .await
        .map_err(|e| e.to_string())?;

    transaction.commit()
        .await
        .map_err(|e| e.to_string())?;

    transform_rows_to_json(&rows, return_fields)
}

async fn delete_record(
    client: &Client,
    table: &str,
    filters: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    // Build DELETE SQL with WHERE clause
    let sql = build_delete_sql(table, filters)?;

    let transaction = client.transaction()
        .await
        .map_err(|e| e.to_string())?;

    let _row_count = transaction.execute(&sql, &[])
        .await
        .map_err(|e| e.to_string())?;

    transaction.commit()
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({"success": true}))
}

fn build_insert_sql(table: &str, input: &serde_json::Value) -> Result<(String, Vec<String>), String> {
    // Implementation: Build INSERT ... RETURNING * SQL
    todo!()
}

fn build_update_sql(table: &str, input: &serde_json::Value, filters: Option<&serde_json::Value>) -> Result<(String, Vec<String>), String> {
    // Implementation: Build UPDATE ... WHERE ... RETURNING * SQL
    todo!()
}

fn build_delete_sql(table: &str, filters: Option<&serde_json::Value>) -> Result<String, String> {
    // Implementation: Build DELETE ... WHERE ... SQL
    todo!()
}

fn transform_rows_to_json(rows: &[tokio_postgres::Row], return_fields: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    // Implementation: Convert rows to JSON, select fields
    todo!()
}
```

---

## Testing Strategy

### Integration Test Patterns

**File**: `tests/integration/graphql/test_rust_queries.py`

```python
"""Integration tests for complete GraphQL queries"""

import pytest
from fraiseql.core.rust_pipeline import pipeline

class TestGraphQLQueries:
    """Test complete query pipeline"""

    @pytest.mark.asyncio
    async def test_simple_user_query(self):
        """Test: query { user(id: 1) { id, name, email } }"""
        result = await pipeline.execute_query({
            'operation': 'query',
            'table': 'users',
            'fields': ['id', 'name', 'email'],
            'filters': {'field': 'id', 'operator': 'eq', 'value': 1}
        })

        assert result['errors'] is None
        assert len(result['data']) == 1
        assert result['data'][0]['id'] == 1

    @pytest.mark.asyncio
    async def test_users_list_with_pagination(self):
        """Test: query { users(limit: 10, offset: 0) { id, name } }"""
        result = await pipeline.execute_query({
            'operation': 'query',
            'table': 'users',
            'fields': ['id', 'name'],
            'pagination': {'limit': 10, 'offset': 0}
        })

        assert result['errors'] is None
        assert len(result['data']) <= 10

    @pytest.mark.asyncio
    async def test_complex_filter_query(self):
        """Test: Complex AND/OR/NOT filters"""
        result = await pipeline.execute_query({
            'operation': 'query',
            'table': 'users',
            'fields': ['id', 'name', 'is_active'],
            'filters': {
                'and': [
                    {'field': 'is_active', 'operator': 'eq', 'value': True},
                    {'field': 'created_at', 'operator': 'gte', 'value': '2025-01-01'}
                ]
            }
        })

        assert result['errors'] is None
```

**File**: `tests/integration/graphql/test_rust_mutations.py`

```python
"""Integration tests for complete GraphQL mutations"""

@pytest.mark.asyncio
async def test_create_user_mutation(self):
    """Test: mutation { createUser(input: {name, email}) { id, name } }"""
    result = await pipeline.execute_mutation({
        'operation': 'mutation',
        'type': 'insert',
        'table': 'users',
        'input': {'name': 'John', 'email': 'john@example.com'},
        'return_fields': ['id', 'name', 'email']
    })

    assert result['errors'] is None
    assert result['data']['name'] == 'John'
    assert result['data']['email'] == 'john@example.com'
    assert 'id' in result['data']

@pytest.mark.asyncio
async def test_update_user_mutation(self):
    """Test: mutation { updateUser(id: 1, input: {name}) { id, name } }"""
    result = await pipeline.execute_mutation({
        'operation': 'mutation',
        'type': 'update',
        'table': 'users',
        'filters': {'field': 'id', 'operator': 'eq', 'value': 1},
        'input': {'name': 'Jane'},
        'return_fields': ['id', 'name']
    })

    assert result['errors'] is None
    assert result['data']['name'] == 'Jane'

@pytest.mark.asyncio
async def test_delete_user_mutation(self):
    """Test: mutation { deleteUser(id: 1) { success } }"""
    result = await pipeline.execute_mutation({
        'operation': 'mutation',
        'type': 'delete',
        'table': 'users',
        'filters': {'field': 'id', 'operator': 'eq', 'value': 1}
    })

    assert result['errors'] is None
    assert result['data']['success'] is True
```

### Parity Tests

```python
"""Test Rust backend matches psycopg backend"""

@pytest.mark.asyncio
async def test_query_parity(self):
    """Verify Rust query results == psycopg results"""
    query_def = {...}

    rust_result = await pipeline.execute_query(query_def)
    python_result = await psycopg_execute_query(query_def)

    assert rust_result == python_result

@pytest.mark.asyncio
async def test_mutation_parity(self):
    """Verify Rust mutation results == psycopg results"""
    mutation_def = {...}

    rust_result = await pipeline.execute_mutation(mutation_def)
    python_result = await psycopg_execute_mutation(mutation_def)

    assert rust_result == python_result
```

---

## Feature Flag Integration

Use FEATURE-FLAGS.md strategy:

```bash
# Test Rust backend
FRAISEQL_DB_BACKEND=rust uv run pytest tests/integration/ -v

# Test Python backend
FRAISEQL_DB_BACKEND=python uv run pytest tests/integration/ -v

# Test both in parallel (parity testing)
FRAISEQL_PARITY_TESTING=true uv run pytest tests/integration/ -v
```

---

## Performance Validation

```bash
# Benchmark query execution
make bench-queries

# Benchmark mutation execution
make bench-queries  # Extend to include mutations

# Compare against baseline
make bench-compare
```

---

## Verification Checklist

### Before Moving to Phase 5

- [ ] All query resolvers working
- [ ] All mutation resolvers working
- [ ] Complex filters (AND/OR/NOT) working
- [ ] Pagination working correctly
- [ ] Sorting working correctly
- [ ] Transactions working (INSERT rollback on error)
- [ ] Error handling and mapping correct
- [ ] All 5991+ tests passing with Rust backend
- [ ] Parity tests 100% match (Rust == psycopg)
- [ ] Performance within 20-30% target
- [ ] Memory usage within 10-15% target
- [ ] No memory leaks (run 1000+ operations)
- [ ] Code coverage ≥ 85%
- [ ] `make qa` passes (clippy, fmt, tests)

---

## Known Issues & Workarounds

### Issue: Large Mutations Fail
**Cause**: Connection timeout or memory limit
**Workaround**: Batch mutations or increase connection timeout

### Issue: Parity Test Fails on NULL Handling
**Cause**: JSONB NULL representation differs
**Workaround**: Normalize NULL representation before comparison

### Issue: Transaction Rollback Doesn't Work
**Cause**: Error handling not triggering transaction.rollback()
**Fix**: Ensure error propagation in Rust includes rollback

---

## Troubleshooting

### "All tests passing locally but parity test fails"

Check:
1. Type conversion (PostgreSQL → Rust → Python → GraphQL)
2. NULL handling in JSONB
3. Date/time formatting
4. Numeric precision
5. Array handling

### "Performance 5% worse than psycopg"

Check:
1. Connection pool efficiency
2. Query plan optimization
3. Unnecessary string allocations
4. JSON transformation overhead

Use `cargo flamegraph` to identify bottlenecks.

---

## Success Definition

✅ Phase 4 complete when:
- All GraphQL queries work end-to-end
- All GraphQL mutations work end-to-end
- All 5991+ tests pass
- Parity tests 100% match
- Performance target met (20-30% faster)
- Zero regressions

---

## Next Phase

After Phase 4 validated:
→ **Phase 5: Deprecation** - Remove psycopg, achieve evergreen state

---

**Status**: Blocked until Phase 3 complete
**Duration**: 8 hours
**Branch**: `feature/rust-postgres-driver`
**Last Updated**: 2025-12-18
