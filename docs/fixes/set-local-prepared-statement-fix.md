# SET LOCAL Prepared Statement Fix

## Issue

FraiseQL v0.1.0b1 had a bug where it attempted to use prepared statements with PostgreSQL's `SET LOCAL` command:

```python
await cursor.execute(
    "SET LOCAL statement_timeout = %s",
    (f"{self.query_timeout * 1000}ms",),
)
```

This caused the error:
```
psycopg.errors.SyntaxError: syntax error at or near "$1"
LINE 1: SET LOCAL statement_timeout = $1
```

## Root Cause

PostgreSQL does not support parameterized queries for `SET` and `SET LOCAL` commands. These commands require literal values.

## Fix

The fix changes the SET LOCAL execution to use literal value substitution:

```python
# Use literal value, not prepared statement parameters
# PostgreSQL doesn't support parameters in SET LOCAL
timeout_ms = int(self.query_timeout * 1000)
await cursor.execute(
    f"SET LOCAL statement_timeout = '{timeout_ms}ms'"
)
```

## Security Considerations

- The `query_timeout` value comes from the FraiseQL context and is always an integer
- We explicitly cast to `int()` to ensure type safety
- No user input is directly interpolated into the SQL string

## Testing

Tests were added to verify:
1. SET LOCAL is executed without prepared statement parameters
2. The timeout value is correctly formatted
3. When `query_timeout` is None, no SET LOCAL is executed

## Impact

This fix allows FraiseQL to work correctly with query timeouts, which is important for:
- Preventing long-running queries from blocking the database
- Meeting SLA requirements in production environments
- Protecting against accidental expensive queries

## Version

Fixed in: v0.1.0b2 (upcoming)
