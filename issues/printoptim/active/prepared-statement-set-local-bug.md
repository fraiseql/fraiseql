# FraiseQL v0.1.0b1: Prepared Statement Error with SET LOCAL Commands

## Issue Description
When executing GraphQL queries that trigger database functions, FraiseQL v0.1.0b1 is attempting to use prepared statements with `SET LOCAL` commands, which causes a PostgreSQL syntax error.

## Error Details
```
psycopg.errors.SyntaxError: syntax error at or near "$1"
LINE 1: SET LOCAL statement_timeout = $1
                                     ^
```

## Stack Trace
```
File "/home/runner/work/printoptim-backend/printoptim-backend/.venv/lib/python3.13/site-packages/fraiseql/db.py", line 77, in run
    await cursor.execute(
    ...<2 lines>...
    )
File "/home/runner/work/printoptim-backend/printoptim-backend/.venv/lib/python3.13/site-packages/psycopg/cursor_async.py", line 97, in execute
    raise ex.with_traceback(None)
psycopg.errors.SyntaxError: syntax error at or near "$1"
```

## Expected Behavior
`SET LOCAL` commands should be executed with literal values, not prepared statement parameters. PostgreSQL does not support parameterized `SET LOCAL` statements.

## Actual Behavior
FraiseQL is trying to execute something like:
```sql
SET LOCAL statement_timeout = $1
```

Instead of:
```sql
SET LOCAL statement_timeout = '30s'
```

## Impact
This bug is causing integration tests to fail in PrintOptim Backend CI when using FraiseQL v0.1.0b1. Specifically:
- 3 out of 30 integration tests are failing due to this issue
- Tests that query GraphQL endpoints that use database functions are affected

## Reproduction Steps
1. Create a GraphQL query that triggers a database function
2. The database function or FraiseQL internals attempt to set a statement timeout
3. The error occurs when FraiseQL tries to use a prepared statement for the SET LOCAL command

## Suggested Fix
When executing `SET LOCAL` or `SET` commands, FraiseQL should:
1. Detect these commands and exclude them from prepared statement logic
2. Use literal value substitution instead of parameterized queries for SET commands
3. Or provide a configuration option to disable prepared statements for certain query patterns

## Workaround
Currently, there's no known workaround while staying on v0.1.0b1. The issue did not occur in v0.1.0a20.

## Environment
- FraiseQL version: 0.1.0b1
- PostgreSQL version: 17
- Python version: 3.13.5
- psycopg version: 3.2.3

## Related Test Output
```
tests/entrypoints/api_fraiseql/test_context_parameters.py::TestFraiseQLContextParameters::test_context_passed_to_database_function FAILED
tests/entrypoints/api_fraiseql/test_context_parameters.py::TestFraiseQLContextParameters::test_context_null_handling FAILED
```

## Priority
Medium - This is blocking some integration tests but not preventing development. However, it may affect production queries that rely on statement timeouts or other SET LOCAL commands.