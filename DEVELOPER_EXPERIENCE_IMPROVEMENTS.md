# Developer Experience Improvements for FraiseQL

This document summarizes the developer experience improvements implemented for FraiseQL based on the requirements in IMPROVEMENT_PLAN.md.

## 1. Enhanced Error Handling (`src/fraiseql/errors/exceptions.py`)

We've created a custom exceptions module that provides:

### Clear Error Messages with Query Context

All exceptions now inherit from `FraiseQLException` which provides:
- **Structured error messages** with error codes
- **Query context** showing relevant information about what failed
- **Helpful hints** suggesting how to fix the issue
- **Cause tracking** to maintain the original exception chain

### Specific Exception Types

1. **PartialInstantiationError**
   - Shows which fields failed during partial object instantiation
   - Lists available vs. requested fields
   - Suggests checking database view configuration

2. **WhereClauseError**
   - Shows invalid operators or fields in WHERE clauses
   - Lists supported operators
   - Provides examples of correct syntax

3. **QueryValidationError**
   - Identifies invalid fields in GraphQL queries
   - Suggests similar field names (typo detection)
   - Shows valid fields for the type

4. **DatabaseQueryError**
   - Provides SQL context when queries fail
   - Suggests solutions for common database errors
   - Helps with missing views or permission issues

5. **TypeRegistrationError**
   - Helps debug type registration issues
   - Shows already registered types
   - Suggests solutions for naming conflicts

6. **ResolverError**
   - Provides context about resolver failures
   - Shows the GraphQL path and arguments
   - Suggests performance optimization tips

## 2. Debugging Utilities (`src/fraiseql/debug/`)

### explain_query() Function
Runs PostgreSQL EXPLAIN on queries to analyze performance:
```python
query = DatabaseQuery(sql="SELECT ...", params={...})
explanation = await explain_query(query, analyze=True)
```

Features:
- Supports EXPLAIN ANALYZE for actual execution times
- Multiple output formats (text, json, xml, yaml)
- Shows query plan with costs and row estimates

### profile_resolver() Decorator
Profile GraphQL resolver performance:
```python
@profile_resolver(threshold_ms=100)
async def resolve_users(parent, info, **kwargs):
    return await fetch_users(**kwargs)
```

Features:
- Logs execution time, arguments, and results
- Configurable threshold for logging
- Tracks errors with full context
- Helps identify N+1 query patterns

### debug_partial_instance() Function
Inspect partial objects to understand what fields are available:
```python
user = create_partial_instance(User, {"id": 1, "name": "John"})
print(debug_partial_instance(user))
```

Output shows:
- Whether instance is partial or full
- Requested fields vs. available fields
- Field values with proper formatting
- Missing fields that weren't requested

### QueryDebugger Context Manager
Capture and analyze all queries in a code block:
```python
async with QueryDebugger() as debugger:
    users = await fetch_users()
    posts = await fetch_posts()

print(debugger.get_summary())
```

Shows:
- Total number of queries
- Total execution time
- Individual query details with timing

### debug_graphql_info() Function
Debug GraphQL query structure:
```python
print(debug_graphql_info(info))
```

Shows:
- Field name, parent type, return type
- Query path
- Operation type and name
- Variables
- Selected fields

## 3. Validation Utilities (`src/fraiseql/validation.py`)

### validate_where_input() Function
Validate WHERE clause objects against type fields:
```python
errors = validate_where_input(
    {"name": {"_eq": "John"}, "invalid": {"_eq": "value"}},
    User
)
```

Features:
- Checks all fields exist on the type
- Validates operators are appropriate for field types
- Provides typo suggestions for field names
- Supports strict mode that raises exceptions

### validate_selection_set() Function
Ensure requested GraphQL fields exist:
```python
errors = validate_selection_set(info, User, max_depth=10)
```

Features:
- Validates field names against type
- Checks query depth limits
- Works with nested selections
- Configurable depth limits

### validate_query_complexity() Function
Calculate and limit query complexity:
```python
complexity, errors = validate_query_complexity(info, max_complexity=1000)
```

Features:
- Calculates complexity score based on fields
- Supports custom field costs
- Accounts for nested lists (multiplier effect)
- Prevents resource-intensive queries

## 4. Integration with Existing Code

### Updated Partial Instantiation
The `partial_instantiation.py` module now uses `PartialInstantiationError` for better error messages:
- Shows which specific field caused failure
- Lists all available and requested fields
- Provides actionable hints

### Error Messages Include Context
All errors now show:
- The operation that failed
- Relevant query/SQL context
- Specific hints for common issues
- Links to documentation (placeholder URLs)

## Usage Examples

See `examples/debug_and_validation_demo.py` for a complete demonstration of all features.

### Quick Example: Better Error Messages

Before:
```
TypeError: __init__() missing required positional argument: 'email'
```

After:
```
PARTIAL_INSTANTIATION_ERROR: Failed to instantiate partial User due to field 'email': Invalid email format

Query Context:
  type: User
  failing_field: email
  available_fields: ['id', 'name']
  requested_fields: ['age', 'email', 'id', 'name']

Hint: Missing fields in data: age, email | Available fields: id, name | Check that your database view returns all requested fields in the 'data' JSONB column
```

### Quick Example: Query Profiling

```python
@profile_resolver(threshold_ms=50)
async def resolve_posts(parent, info, **kwargs):
    # Resolver implementation
    pass

# Logs:
# resolver_profiled: resolver=resolve_posts field=posts path=/posts duration_ms=67.3 args={'limit': 10}
```

## Benefits

1. **Faster Debugging**: Clear error messages with context eliminate guesswork
2. **Performance Insights**: Built-in profiling identifies bottlenecks
3. **Early Error Detection**: Validation catches issues before they hit the database
4. **Better Developer Onboarding**: Helpful hints guide developers to solutions
5. **Production Readiness**: Structured logging and metrics for monitoring

These improvements make FraiseQL significantly more developer-friendly while maintaining its simplicity and performance.
