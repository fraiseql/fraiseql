# First Real User Explorations of FraiseQL

This document captures the misunderstandings, confusion points, and challenges encountered during the first real-world attempt to migrate a production application from Strawberry GraphQL to FraiseQL.

## Context

- **User**: Migrating PrintOptim backend from Strawberry GraphQL to FraiseQL
- **Date**: December 2024
- **Starting Point**: Existing Strawberry GraphQL API with complex types, queries, and mutations
- **Goal**: Complete migration to FraiseQL while maintaining functionality

## Major Misunderstandings and Pain Points

### 1. Query Registration Pattern Confusion

**What I Expected**: Based on the blog example, I expected to pass query functions directly:
```python
async def get_user(info, id: UUID) -> User:
    return User(...)

app = create_fraiseql_app(
    queries=[get_user, get_posts],  # Just functions
)
```

**What Actually Happened**: 
- The schema builder threw "Type Query must define one or more fields"
- The `build_fraiseql_schema` function calls `register_type()` on query functions
- This suggests queries should be types, not functions

**Attempted Solutions**:
1. Created a `QueryRoot` type with `@fraiseql.field` decorators (failed - no such decorator)
2. Tried passing empty queries list (failed - same error)
3. Tried using types in queries parameter (still unclear what the correct pattern is)

**Confusion**: The blog example clearly shows passing functions, but the code expects types. This fundamental mismatch made it impossible to get even a basic schema working.

### 2. Context and Database Access Pattern

**Initial Assumption**: I thought I needed to provide a `context_getter` function like in Strawberry:
```python
async def get_context(request: Request) -> dict:
    return {
        "db": connection,
        "user": current_user,
    }
```

**Reality**: 
- `create_fraiseql_app()` doesn't accept a `context_getter` parameter
- The CQRS pattern seems to be injected differently
- Blog example shows dependency injection but it's not clear how it connects

### 3. Authentication Integration

**Challenge**: The Auth0 integration in the existing code caused hanging because:
- Module-level Auth0 initialization tries to fetch JWKS immediately
- This blocks the import process
- Had to create a lazy loading pattern to work around it

**What Would Help**: Clear documentation on how to integrate existing auth systems without using FraiseQL's built-in auth.

### 4. Type System Limitations

**Unexpected Restrictions**:
- No support for `dict[str, Any]` or JSON types in GraphQL schema
- Had to remove `details` field from Error type
- Unclear what the alternative is for flexible JSON data

**Missing Guidance**: How to handle dynamic/flexible data structures that were previously JSON fields.

### 5. Environment Variable Conflicts

**Problem**: FraiseQL's `FraiseQLConfig` uses pydantic settings which auto-loads from environment:
```
pydantic_core._pydantic_core.ValidationError: 5 validation errors for FraiseQLConfig
env
  Extra inputs are not permitted [type=extra_forbidden, input_value='local', input_type=str]
```

**Issue**: Common env var names like `ENV`, `DEBUG`, `DB_USER` conflict with FraiseQL's config.

**Workaround Needed**: Had to rename .env file to prevent auto-loading.

### 6. Decorator Confusion

**What I Tried**:
- `@fraiseql.query` - Doesn't exist (though seemed logical)
- `@fraiseql.field` - Doesn't exist (tried for QueryRoot)
- `@fraiseql.type` - Works
- `@fraiseql.input` - Works
- `@fraiseql.mutation` - Exists but wasn't working as expected
- `@fraiseql.success`/`@fraiseql.failure` - Work for result types

**Confusion**: Inconsistent decorator API - why do types and inputs have decorators but queries don't?

### 7. Mutation Pattern Mismatch

**Initial Attempt**: Tried using class-based mutations like Strawberry:
```python
@fraiseql.mutation
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError
    
    async def resolve(self, info, input):
        ...
```

**Error**: "Mutation CreateMachine must define 'error' type"

**Correct Pattern**: Mutations should be functions returning Union types:
```python
async def create_machine(info, input: CreateMachineInput) -> CreateMachineSuccess | CreateMachineError:
    ...
```

### 8. Database URL Format

**Issue**: Settings provided psycopg connection string format:
```
"dbname='printoptim_db_local' user='lionel' host='localhost'..."
```

**Expected**: PostgreSQL URL format:
```
"postgresql://user:pass@host:port/dbname"
```

Had to add a new property to convert between formats.

### 9. Missing Import Location

**Confusion**: Where to import from?
- `fraiseql` - Types and decorators
- `fraiseql.fastapi` - `create_fraiseql_app`
- `fraiseql.cqrs` - `CQRSRepository`

The import structure wasn't immediately clear from docs.

### 10. Async Context Manager Lifecycle

**Problem**: The existing app used FastAPI's lifespan for async connection pool:
```python
@asynccontextmanager
async def lifespan(app: FastAPI):
    pool = AsyncConnectionPool(...)
    await pool.open()
    ...
```

**Unknown**: How FraiseQL handles async resource lifecycle management.

## What Would Have Helped

1. **Migration Guide**: A specific guide for migrating from Strawberry would be invaluable
2. **Query Examples**: Clear examples of how queries should be structured (not just mutations)
3. **Type Restrictions**: Document unsupported types upfront (like arbitrary JSON)
4. **Config Documentation**: List all environment variables that FraiseQL uses
5. **Error Messages**: More helpful error messages (e.g., "queries parameter expects X, got Y")
6. **Complete Working Example**: A full example with queries, mutations, and custom types
7. **Decorator Reference**: Complete list of available decorators and their purposes

## Positive Observations

Despite the challenges:
- The CQRS pattern is interesting and could simplify data access
- Type decorators are clean and familiar
- The migration from Strawberry types was mostly straightforward
- Error handling patterns with success/failure types are well thought out

## Conclusion

The migration attempt revealed that while FraiseQL has promise, the documentation and examples don't fully match the actual implementation. The query registration pattern in particular seems to be a fundamental blocker that prevented completing even a basic migration.

The confusion between what the examples show (passing query functions) and what the code expects (types for queries) suggests either:
1. The examples are outdated
2. There's a missing piece of configuration
3. The API has changed recently

This real-world migration attempt highlights the need for more comprehensive documentation and migration guides for users coming from other GraphQL frameworks.