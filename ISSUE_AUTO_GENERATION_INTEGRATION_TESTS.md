# Issue: Auto-Generation Integration Tests Failing

## Problem Summary

Phase 3 integration tests for auto-generated WhereInput/OrderBy types are failing when attempting to execute actual database queries and GraphQL queries through FraiseQL's resolver system.

## Current Status

### ✅ Working (4/5 GraphQL Schema Tests Pass)
- Schema generation correctly includes auto-generated WhereInput types
- Nested type auto-generation works in schema
- Multiple queries can use the same auto-generated types
- Introspection correctly shows auto-generated types

### ❌ Failing Tests

#### 1. Database Integration Tests (5/5 failing)
**Location**: `tests/integration/core/test_auto_generation_integration.py`

**Error**: SQL placeholder issue
```
psycopg.ProgrammingError: only '%s', '%b', '%t' are allowed as placeholders, got '%B'
```

**Root Cause**: The auto-generated WhereInput types work correctly, but there's an incompatibility with how the test constructs database queries. The error occurs in FraiseQL's Rust pipeline when processing the where clause.

**Test Pattern**:
```python
WhereInput = CustomerAutoTest.WhereInput
where = WhereInput(name={"eq": "Alice Anderson"})
response = await db.find("tv_customers_auto_test", where=where)
results = response.to_json()["data"]["tv_customers_auto_test"]
```

**Issue**: The `db.find()` method expects where clauses in a specific format that may not be compatible with WhereInput dataclass instances.

#### 2. GraphQL Query Execution Test (1/5 failing)
**Location**: `tests/integration/graphql/test_auto_generated_schema.py::test_auto_generated_types_with_actual_query_execution`

**Error**:
```
GraphQLError("'NoneType' object does not support item assignment",
             locations=[SourceLocation(line=3, column=13)],
             path=['articles'])
```

**Root Cause**: FraiseQL's query resolver wrapper is attempting to process the result in a way that fails. The resolver returns a list of dataclass instances, but something in FraiseQL's result processing pipeline is attempting to assign to None.

**Test Code**:
```python
@fraiseql.query
async def articles(
    where: ArticleSchemaTest.WhereInput | None = None,
) -> list[ArticleSchemaTest]:
    """Query articles."""
    return [
        ArticleSchemaTest(
            id=uuid.uuid4(),
            title="Test Article",
            content="Test content"
        )
    ]

# Execute query
from graphql import graphql
result = await graphql(schema, query)
# Result has errors
```

## Investigation Steps

### Step 1: Understand WhereInput Format Expected by db.find()

**Task**: Determine what format `FraiseQLRepository.find()` expects for the `where` parameter.

**Files to Check**:
- `src/fraiseql/db.py` - Look at the `find()` method signature and how it processes `where`
- Existing integration tests that successfully use `db.find()` with where clauses
- `src/fraiseql/sql/where_generator.py` - How where clauses are converted to SQL

**Questions**:
1. Does `db.find()` expect a dict, a dataclass instance, or something else?
2. Do we need to convert WhereInput dataclass to dict before passing to `db.find()`?
3. Is there a transformation step missing?

**Test**:
```python
# Try different formats
where_dict = {"name": {"eq": "Alice"}}
where_dataclass = WhereInput(name={"eq": "Alice"})

# Which works?
result1 = await db.find("table", where=where_dict)
result2 = await db.find("table", where=where_dataclass)
```

### Step 2: Investigate FraiseQL Query Resolver Processing

**Task**: Understand why returning a list from a `@fraiseql.query` causes NoneType assignment error.

**Files to Check**:
- `src/fraiseql/gql/query_builder.py` or wherever `@fraiseql.query` decorator is defined
- `src/fraiseql/core/rust_pipeline.py` - Result processing
- Working examples in `tests/integration/graphql/` that successfully execute queries

**Questions**:
1. What does FraiseQL's query wrapper expect to be returned?
2. Is there special handling for lists vs RustResponseBytes?
3. Do we need to register the type differently for queries to work?
4. Is there a context or info parameter we're missing?

**Test**:
```python
# Compare with working query pattern
@fraiseql.query
async def working_query(info) -> list[SomeType]:
    # What does a working query look like?
    pass
```

### Step 3: Check Type Registration

**Task**: Verify if auto-generated types need special registration.

**Hypothesis**: The auto-generated WhereInput types might not be properly registered with FraiseQL's type system, causing issues during query execution.

**Files to Check**:
- `src/fraiseql/db.py` - `register_type_for_view()` function
- `src/fraiseql/gql/schema_builder.py` - Type registration

**Questions**:
1. Do WhereInput types need to be registered?
2. Is there metadata that needs to be set?
3. Are there FK relationships that need declaring?

**Test**:
```python
from fraiseql.db import register_type_for_view

register_type_for_view(
    "tv_customers_auto_test",
    CustomerAutoTest,
    table_columns={"id", "name", "email", "data"},
    has_jsonb_data=True
)

# Does this help?
```

### Step 4: Examine Rust Pipeline SQL Generation

**Task**: Understand the SQL placeholder error from the Rust pipeline.

**Error Details**:
```
psycopg.ProgrammingError: only '%s', '%b', '%t' are allowed as placeholders, got '%B'
```

**Files to Check**:
- `src/fraiseql/core/rust_pipeline.py` - Where SQL is generated
- `src/fraiseql/sql/operator_strategies.py` - SQL generation strategies

**Questions**:
1. What is generating '%B' placeholder?
2. Is this related to BOOLEAN type handling?
3. Does the auto-generated WhereInput produce invalid SQL templates?

**Test**:
```python
# Enable SQL logging
import logging
logging.basicConfig(level=logging.DEBUG)

# Try query and capture SQL
try:
    await db.find("table", where=where)
except Exception as e:
    # Check what SQL was generated
    print(f"SQL error: {e}")
```

## Potential Solutions

### Solution A: Convert WhereInput to Dict

If `db.find()` expects plain dicts, we can convert:

```python
from dataclasses import asdict

where_dataclass = WhereInput(name={"eq": "Alice"})
where_dict = asdict(where_dataclass)
# Remove None values
where_dict = {k: v for k, v in where_dict.items() if v is not None}

results = await db.find("table", where=where_dict)
```

**Pros**: Simple, non-invasive
**Cons**: Defeats the purpose of type-safe WhereInput

### Solution B: Make db.find() Accept Dataclasses

Modify `FraiseQLRepository.find()` to detect WhereInput dataclasses and convert them:

```python
# In src/fraiseql/db.py
async def find(self, view_name: str, where=None, **kwargs):
    # Detect if where is a WhereInput dataclass
    if where and hasattr(where, '__dataclass_fields__'):
        from dataclasses import asdict
        where = {k: v for k, v in asdict(where).items() if v is not None}

    # Continue with existing logic
    ...
```

**Pros**: Makes API more user-friendly, type-safe at call site
**Cons**: Requires modifying core FraiseQL code

### Solution C: Create Helper Method

Add a method to WhereInput types for database compatibility:

```python
# In src/fraiseql/types/lazy_properties.py
class LazyWhereInputProperty:
    def __get__(self, obj, objtype):
        # Generate WhereInput class
        where_input_class = create_graphql_where_input(objtype)

        # Add helper method
        def to_db_format(self):
            """Convert WhereInput to format expected by db.find()."""
            from dataclasses import asdict
            return {k: v for k, v in asdict(self).items() if v is not None}

        where_input_class.to_db_format = to_db_format
        return where_input_class
```

**Usage**:
```python
where = WhereInput(name={"eq": "Alice"})
results = await db.find("table", where=where.to_db_format())
```

**Pros**: Explicit, backward compatible
**Cons**: Extra method call

### Solution D: Fix GraphQL Query Resolver

For the GraphQL execution issue, investigate FraiseQL's query processing:

```python
# Check if we need to provide info context
@fraiseql.query
async def articles(
    info,  # Add info parameter?
    where: ArticleSchemaTest.WhereInput | None = None,
) -> list[ArticleSchemaTest]:
    # Maybe the resolver wrapper needs info?
    return [ArticleSchemaTest(...)]
```

Or check if there's a response format expected:

```python
# Maybe return a dict structure?
return {"articles": [ArticleSchemaTest(...)]}
```

## Next Steps

1. **Priority 1**: Investigate Step 1 (WhereInput format for db.find())
   - Add debug logging to db.find() to see what it receives
   - Check existing working tests for the pattern
   - Document the expected format

2. **Priority 2**: Investigate Step 4 (SQL placeholder error)
   - Enable SQL query logging
   - Capture the generated SQL
   - Identify what's producing '%B'

3. **Priority 3**: Investigate Step 2 (GraphQL resolver)
   - Find working query examples
   - Compare resolver registration
   - Test with info parameter

4. **Implementation**: Once root cause identified, implement appropriate solution
   - Solution B (modify db.find()) seems most user-friendly
   - Solution D (fix resolver) needed for GraphQL tests
   - Update integration tests
   - Ensure all tests pass

## Success Criteria

When fixed, these assertions should pass:

```python
# Database integration
where = CustomerAutoTest.WhereInput(name={"eq": "Alice"})
response = await db.find("tv_customers_auto_test", where=where)
results = response.to_json()["data"]["tv_customers_auto_test"]
assert len(results) == 1
assert results[0]["name"] == "Alice"

# GraphQL execution
from graphql import graphql
result = await graphql(schema, query)
assert result.errors is None
assert result.data["articles"] is not None
```

## Files to Review

### Core Files
- `src/fraiseql/db.py` - Repository find() method
- `src/fraiseql/core/rust_pipeline.py` - SQL generation
- `src/fraiseql/gql/query_builder.py` - Query decorator

### Test Files
- `tests/integration/core/test_auto_generation_integration.py` - Database tests
- `tests/integration/graphql/test_auto_generated_schema.py` - GraphQL tests
- `tests/integration/repository/test_repository_find_where_processing.py` - Working db.find() examples

### Implementation Files
- `src/fraiseql/types/lazy_properties.py` - Auto-generation logic
- `src/fraiseql/sql/graphql_where_generator.py` - WhereInput generation

## Related Context

- Auto-generation feature (Phase 1 & 2) works perfectly for schema generation
- Unit tests (8/8) pass for lazy property mechanism
- GraphQL schema tests (4/5) pass
- The core feature is sound; integration layer needs investigation
- This is about making the generated types work with FraiseQL's database/resolver pipeline
