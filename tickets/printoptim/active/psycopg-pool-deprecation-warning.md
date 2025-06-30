# Psycopg Pool Deprecation Warning

## Issue Description

When FraiseQL initializes the async connection pool, it triggers a deprecation warning from psycopg_pool about opening the pool in the constructor.

## Warning Message

```
/home/lionel/code/printoptim_backend/.venv/lib/python3.13/site-packages/psycopg_pool/pool_async.py:142: RuntimeWarning: opening the async pool AsyncConnectionPool
in the constructor is deprecated and will not be supported anymore in a future release. Please use `await pool.open()`, or use the pool as context manager using: `async with AsyncConnectionPool(...) as pool: `...
```

## Current Implementation (Deprecated)

FraiseQL appears to be using:
```python
pool = AsyncConnectionPool(...)  # Opens in constructor
```

## Recommended Implementation

According to psycopg documentation, the pool should be opened explicitly:

```python
# Option 1: Explicit open
pool = AsyncConnectionPool(...)
await pool.open()

# Option 2: Context manager
async with AsyncConnectionPool(...) as pool:
    # Use pool
```

## Impact

- Currently just a warning, functionality not affected
- Will break in future versions of psycopg when the deprecated pattern is removed

## Environment

- Python 3.13
- psycopg_pool (latest version)
- Async connection pool usage

## Recommendation

Update the connection pool initialization in FraiseQL to use the new pattern before it becomes a breaking change in future psycopg versions.
