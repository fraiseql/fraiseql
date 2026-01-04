# Row-Level Auth Middleware - Revised Implementation Plan

**Approach**: Pragmatic 3-component solution leveraging existing RBAC infrastructure
**Total Effort**: 6-8 hours
**Phases**: 5 (as planned, but simpler scope)

---

## ARCHITECTURE

```
GraphQL Query with WHERE clause
    ↓
RbacMiddleware (EXISTING)
    ├─ Extract user context
    ├─ Inject PermissionResolver
    └─ Store in info.context
    ↓
RowLevelAuthMiddleware (NEW - 250 LOC)
    ├─ Read user context from RbacMiddleware
    ├─ Call RowFilterResolver
    ├─ Call RowWhereClauseBuilder
    └─ Store merged filters in context
    ↓
GraphQL Field Resolver (MODIFIED - 20 LOC)
    ├─ Get explicit WHERE from args
    ├─ Get merged filters from context
    ├─ Pass to Rust pipeline
    └─ Execute query
    ↓
Rust WHERE Pipeline (NO CHANGES)
    └─ Standard SQL WHERE execution
```

---

## COMPONENT 1: RowFilterResolver

**File**: `src/fraiseql/enterprise/rbac/row_filter_resolver.py` (300 LOC)

**Purpose**: Given user + table + roles, determine what rows they can access

**Key Methods**:
```python
async def get_row_filters(
    user_id: UUID,
    table_name: str,
    roles: list[Role],
    context: dict
) -> Optional[dict]
    """
    Returns WHERE clause fragment for row access, e.g.:
    {
        "field": "tenant_id",
        "operator": "eq",
        "value": context["tenant_id"]
    }
    """

async def _query_row_constraints(
    table_name: str,
    roles: list[Role]
) -> list[RowConstraint]
    """Query database for table row constraints"""

def _build_where_fragment(constraint, context) -> dict
    """Convert constraint to WHERE clause"""
```

**Data it queries**:
- Assumes a `row_constraints` table exists (or will create):
  ```
  CREATE TABLE row_constraints (
    id UUID PRIMARY KEY,
    table_name VARCHAR,
    role_id UUID,
    constraint_type ENUM('ownership', 'tenant', 'expression'),
    field_name VARCHAR,
    expression VARCHAR,  -- for custom expressions
    FOREIGN KEY (role_id) REFERENCES roles(id)
  )
  ```

**Caching**:
- Request-level: Cache resolved filters per user+table
- PostgreSQL: Use existing PermissionCache layer

---

## COMPONENT 2: RowWhereClauseBuilder

**File**: `src/fraiseql/enterprise/rbac/auth_where_builder.py` (200 LOC)

**Purpose**: Merge explicit WHERE clauses with row-level auth filters

**Key Methods**:
```python
def merge_where_clauses(
    explicit_where: Optional[dict],
    row_filters: Optional[dict]
) -> dict:
    """
    Input:
      explicit_where: {status: {eq: "active"}}
      row_filters:    {tenant_id: {eq: user_tenant_id}}

    Output:
      {AND: [{status: {eq: "active"}}, {tenant_id: {eq: user_tenant_id}}]}
    """

def detect_conflicts(explicit_where, row_filters) -> list[Conflict]:
    """Find where explicit WHERE conflicts with auth filter"""
```

**Handles**:
- Empty/None filters
- AND composition
- Conflict detection (e.g., explicit owner_id conflicts with auth filter)
- Complex nested WHERE clauses

---

## COMPONENT 3: RowLevelAuthMiddleware

**File**: `src/fraiseql/enterprise/rbac/row_level_middleware.py` (250 LOC)

**Purpose**: Orchestrate the above in Strawberry middleware stack

**Key Method**:
```python
async def resolve(
    self,
    next_: Callable[..., Awaitable[Any]],
    root: Any,
    info: Any,
    **kwargs: Any
) -> Any:
    """
    1. Check if this is a root-level query (avoid running on every field)
    2. Extract user context (already set by RbacMiddleware)
    3. Get table name being queried (from field name)
    4. Call RowFilterResolver to get row filters
    5. Store in info.context["__row_level_filters__"]
    6. Call next resolver
    """
```

**Stacking Order** (in app setup):
```python
schema = strawberry.Schema(
    query=Query,
    mutation=Mutation,
    extensions=[
        RbacMiddleware(),           # Layer 1: Context + Permission resolver
        RowLevelAuthMiddleware(),   # Layer 2: Row filters
    ]
)
```

---

## INTEGRATION: graphql_type.py Resolver (20 LOC)

**File**: `src/fraiseql/core/graphql_type.py` (MODIFY existing)

**Change**: In list query resolver, merge filters:

```python
async def resolve_list_query(info, where=None, **kwargs):
    # Existing code: explicit WHERE from GraphQL args
    explicit_where = where or {}

    # NEW: Get row-level filters from middleware context
    row_filters = info.context.get("__row_level_filters__", {}).get(table_name)

    # NEW: Merge using RowWhereClauseBuilder
    if row_filters:
        merged_where = AuthWhereClauseBuilder.merge(explicit_where, row_filters)
    else:
        merged_where = explicit_where

    # Existing code: normalize and execute
    where_clause = normalize_dict_where(merged_where, table_name)
    return await execute_via_rust_pipeline(where_clause)
```

---

## PHASE BREAKDOWN

### Phase 1: RowFilterResolver (1-2 hours)
- [ ] Create `row_filter_resolver.py`
- [ ] Implement `get_row_filters()` method
- [ ] Query `row_constraints` table from database
- [ ] Add request-level caching
- [ ] Create `RowConstraint` dataclass
- [ ] Unit tests for filter resolution

### Phase 2: RowWhereClauseBuilder (1 hour)
- [ ] Create `auth_where_builder.py`
- [ ] Implement `merge_where_clauses()` function
- [ ] Add conflict detection logic
- [ ] Handle edge cases (None, empty, nested)
- [ ] Unit tests for merging logic

### Phase 3: RowLevelAuthMiddleware (1-2 hours)
- [ ] Create `row_level_middleware.py`
- [ ] Implement Strawberry middleware interface
- [ ] Integrate with RbacMiddleware context
- [ ] Call RowFilterResolver
- [ ] Store filters in context
- [ ] Unit tests for middleware

### Phase 4: Integration (1 hour)
- [ ] Modify `graphql_type.py` resolver (~20 LOC)
- [ ] Register middleware in app setup
- [ ] Create `row_constraints` table schema
- [ ] Integration tests with real GraphQL queries

### Phase 5: Testing & Documentation (1-2 hours)
- [ ] Unit tests (300 LOC)
- [ ] Integration tests (400 LOC)
- [ ] Security tests (200 LOC)
- [ ] Documentation & examples

---

## DATA SCHEMA

### Create row_constraints table:
```sql
CREATE TABLE row_constraints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    table_name VARCHAR NOT NULL,
    role_id UUID NOT NULL,
    constraint_type VARCHAR NOT NULL,  -- 'ownership', 'tenant', 'expression'
    field_name VARCHAR,                 -- For ownership/tenant constraints
    expression VARCHAR,                 -- For custom expression constraints
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
    UNIQUE(table_name, role_id, constraint_type)
);

-- Example data:
INSERT INTO row_constraints VALUES
    -- Admin: can see all rows
    -- (no constraint = no WHERE filter)

    -- Manager: can only see tenant's rows
    ('...', 'documents', manager_role_id, 'tenant', 'tenant_id', NULL),

    -- User: can only see their own rows
    ('...', 'documents', user_role_id, 'ownership', 'owner_id', NULL),

    -- Analyst: can see published docs in their tenant
    ('...', 'documents', analyst_role_id, 'expression', NULL, 'status = "published" AND tenant_id = :user_tenant_id');
```

---

## USAGE EXAMPLE

After implementation:

```python
# Configuration (define which table has row constraints)
# This is declarative - no code changes needed!
ROW_LEVEL_AUTH_CONFIG = {
    "enabled": True,
    "tables": ["documents", "projects", "comments"],
}

# Developer code (unchanged!)
@query
async def documents(parent, info: Info, where: Optional[DocumentWhereInput] = None) -> List[Document]:
    """Get documents for current user"""
    # Middleware automatically filters!
    # Even if user queries: documents(where: {owner_id: {eq: "other_user_id"}})
    # The row_level auth filter will STILL apply
    # Result: User can only see their own documents

    docs = await repository.get_documents(where=where)
    return docs

# Client query (same as before):
query {
    documents(where: {status: {eq: "published"}}) {
        id
        title
        owner { name }
    }
}

# What happens behind the scenes:
# 1. RbacMiddleware extracts user context (user_id, tenant_id, roles)
# 2. RowLevelAuthMiddleware resolves filters:
#    - User has role "user"
#    - User role has constraint: owner_id = :user_id
#    - Filter resolved: {owner_id: {eq: "550e8400..."}}
# 3. RowWhereClauseBuilder merges:
#    - Explicit WHERE: {status: {eq: "published"}}
#    - Auth filter:    {owner_id: {eq: "550e8400..."}}
#    - Result:         {AND: [{status: {eq: "published"}}, {owner_id: {eq: "550e8400..."}}]}
# 4. Rust pipeline executes:
#    SELECT * FROM documents WHERE status = 'published' AND owner_id = $1
# 5. Result: Only user's published documents returned
```

---

## TESTING STRATEGY

### Unit Tests (300 LOC)
- RowFilterResolver: filter resolution, caching, constraint evaluation
- RowWhereClauseBuilder: merging, conflict detection, edge cases
- RowLevelAuthMiddleware: context extraction, filter storage

### Integration Tests (400 LOC)
- GraphQL query with row filtering
- Nested query filtering (documents with comments)
- Mutations with filtering (UPDATE/DELETE respects filters)
- Real database interaction

### Security Tests (200 LOC)
- Explicit WHERE bypass prevention
- NULL filter abuse prevention
- Permission escalation prevention
- Unauthorized access denial

---

## SUCCESS CRITERIA

✅ Row-level filters automatically applied
✅ Explicit WHERE clauses merged correctly
✅ <1ms overhead (cached)
✅ <10ms overhead (uncached)
✅ 100% backward compatible
✅ Zero data exposure
✅ Full test coverage

---

## ROLLOUT STRATEGY

**Phase A**: Deploy with `enabled: false` (zero risk)
**Phase B**: Enable for non-critical tables, monitor
**Phase C**: Enable for all tables
**Phase D**: Retire manual WHERE clauses in application code

---

## FILES TO CREATE/MODIFY

### Create (800 LOC):
- `src/fraiseql/enterprise/rbac/row_filter_resolver.py` (300 LOC)
- `src/fraiseql/enterprise/rbac/auth_where_builder.py` (200 LOC)
- `src/fraiseql/enterprise/rbac/row_level_middleware.py` (250 LOC)
- `tests/integration/enterprise/rbac/test_row_level_auth.py` (900 LOC)

### Modify (50 LOC):
- `src/fraiseql/core/graphql_type.py` (20 LOC)
- `src/fraiseql/enterprise/rbac/__init__.py` (10 LOC)
- `src/fraiseql/fastapi/app.py` (10 LOC)
- Database: Create `row_constraints` table (SQL)

---

## NEXT STEPS

1. ✅ Review revised plan
2. ⏳ Begin Phase 1: RowFilterResolver
3. ⏳ Continue through Phases 2-5
4. ⏳ Create comprehensive test suite
5. ⏳ Create PR with all changes
6. ⏳ Code review with security focus
