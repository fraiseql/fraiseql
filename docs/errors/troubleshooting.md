---
← [Error Types](./error-types.md) | [Errors Index](./index.md) | [PostgreSQL Functions →](../mutations/postgresql-function-based.md)
---

# Troubleshooting Guide

> **In this section:** Diagnose and resolve common FraiseQL issues with step-by-step solutions
> **Prerequisites:** Basic debugging skills and familiarity with error messages
> **Time to complete:** 25 minutes

Common issues and their solutions when working with FraiseQL.

## Connection Issues

### Problem: "connection refused" Error

**Symptoms:**
```
psycopg.OperationalError: connection to server at "localhost" (127.0.0.1), port 5432 failed: Connection refused
```

**Causes:**
- PostgreSQL not running
- Wrong port number
- Firewall blocking connection

**Solutions:**

1. **Check PostgreSQL status:**
```bash
# Linux/Mac
pg_isready
sudo systemctl status postgresql

# Docker
docker ps | grep postgres
```

2. **Verify connection string:**
```bash
# Test connection
psql $DATABASE_URL

# Check environment variable
echo $DATABASE_URL
```

3. **Start PostgreSQL:**
```bash
# System service
sudo systemctl start postgresql

# Docker
docker start postgres_container
```

### Problem: "password authentication failed"

**Symptoms:**
```
psycopg.OperationalError: FATAL: password authentication failed for user "fraiseql"
```

**Solutions:**

1. **Verify credentials:**
```python
# Check connection string format
# postgresql://username:password@host:port/database
DATABASE_URL = "postgresql://fraiseql:correct_password@localhost:5432/fraiseql_db"
```

2. **Reset password:**
```sql
-- As PostgreSQL superuser
ALTER USER fraiseql PASSWORD 'new_password';
```

3. **Check pg_hba.conf:**
```bash
# Ensure authentication method is correct
# Location: /etc/postgresql/14/main/pg_hba.conf
local   all   all   md5
host    all   all   127.0.0.1/32   md5
```

## Schema Issues

### Problem: "relation does not exist"

**Symptoms:**
```
asyncpg.exceptions.UndefinedTableError: relation "v_user" does not exist
```

**Causes:**
- View not created
- Wrong schema
- Migrations not run

**Solutions:**

1. **Check if view exists:**
```sql
-- List all views
\dv

-- Check specific view
SELECT * FROM information_schema.views
WHERE table_name = 'v_user';
```

2. **Create missing view:**
```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,
    name,
    created_at
FROM users
WHERE deleted_at IS NULL;
```

3. **Run migrations:**
```bash
# If using migration tool
alembic upgrade head

# Or run SQL directly
psql $DATABASE_URL < migrations/001_create_views.sql
```

### Problem: "column does not exist"

**Symptoms:**
```
asyncpg.exceptions.UndefinedColumnError: column "email" does not exist
```

**Solutions:**

1. **Check view definition:**
```sql
-- Show view definition
\d+ v_user

-- Or
SELECT pg_get_viewdef('v_user'::regclass, true);
```

2. **Update view to include column:**
```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,  -- Add missing column
    name,
    created_at
FROM users;
```

## Type Issues

### Problem: "Cannot instantiate type"

**Symptoms:**
```
PartialInstantiationError: Cannot instantiate User - missing required fields: ['email', 'name']
```

**Causes:**
- View not returning all required fields
- NULL values in non-nullable fields
- Type mismatch

**Solutions:**

1. **Check type definition:**
```python
@fraise_type
class User:
    id: str
    email: str  # Required field
    name: str   # Required field
    bio: Optional[str] = None  # Optional field
```

2. **Ensure view returns all fields:**
```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id::text,           -- Cast to match type
    COALESCE(email, '') as email,  -- Handle NULLs
    COALESCE(name, 'Unknown') as name,
    bio
FROM users;
```

3. **Add default values:**
```python
@fraise_type
class User:
    id: str
    email: str = ""  # Default value
    name: str = "Unknown"
    bio: Optional[str] = None
```

## Query Issues

### Problem: "Invalid WHERE clause"

**Symptoms:**
```
WhereClauseError: Invalid operator 'nearby' for field 'location'
```

**Solutions:**

1. **Use supported operators:**
```python
# Supported operators
where = {
    "id": {"eq": "123"},        # Equals
    "age": {"gte": 18},          # Greater than or equal
    "name": {"like": "%john%"},  # Pattern matching
    "status": {"in": ["active", "pending"]},  # In list
    "deleted": {"is": None}      # IS NULL
}
```

2. **Check field types:**
```python
# Ensure correct type for operator
where = {
    "created_at": {"gte": "2024-01-01"},  # Date comparison
    "price": {"lt": 100.0},                # Numeric comparison
    "tags": {"contains": ["python"]}       # Array contains
}
```

### Problem: N+1 Query Detection

**Symptoms:**
```
Warning: N+1 query pattern detected for User.posts
```

**Solutions:**

1. **Use DataLoader:**
```python
from fraiseql import dataloader_field

@fraise_type
class User:
    id: str
    name: str

    @dataloader_field
    async def posts(self, info) -> List[Post]:
        # Automatically batched
        return await load_user_posts(self.id)
```

2. **Use JOIN in view:**
```sql
CREATE OR REPLACE VIEW v_user_with_posts AS
SELECT
    u.id,
    u.name,
    json_agg(p.*) as posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id, u.name;
```

## Authentication Issues

### Problem: "Authentication required"

**Symptoms:**
```json
{
  "errors": [{
    "message": "Authentication required",
    "extensions": {"code": "UNAUTHENTICATED"}
  }]
}
```

**Solutions:**

1. **Provide authentication token:**
```python
# HTTP header
headers = {
    "Authorization": "Bearer your_token_here"
}

# GraphQL context
context = {
    "user": authenticated_user,
    "token": auth_token
}
```

2. **Check token expiration:**
```python
import jwt
from datetime import datetime

try:
    payload = jwt.decode(token, SECRET_KEY, algorithms=["HS256"])
    if datetime.fromtimestamp(payload['exp']) < datetime.now():
        # Token expired, refresh it
        new_token = refresh_token(token)
except jwt.ExpiredSignatureError:
    # Handle expired token
    pass
```

### Problem: "Permission denied"

**Symptoms:**
```json
{
  "errors": [{
    "message": "Permission denied",
    "extensions": {
      "code": "FORBIDDEN",
      "required_permission": "posts.delete"
    }
  }]
}
```

**Solutions:**

1. **Check user permissions:**
```python
# Verify user has permission
if not user.has_permission("posts.delete"):
    # Request permission or use different user
    pass
```

2. **Use correct role:**
```sql
-- Grant permission to role
GRANT DELETE ON posts TO editor_role;

-- Add user to role
GRANT editor_role TO current_user;
```

## Performance Issues

### Problem: Slow Queries

**Symptoms:**
- Queries taking >1 second
- Timeout errors
- High database CPU usage

**Solutions:**

1. **Add indexes:**
```sql
-- Check query plan
EXPLAIN ANALYZE SELECT * FROM v_user WHERE email = 'test@example.com';

-- Add index if needed
CREATE INDEX idx_users_email ON users(email);
```

2. **Optimize views:**
```sql
-- Use materialized view for complex queries
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
    user_id,
    COUNT(*) as post_count,
    MAX(created_at) as last_post
FROM posts
GROUP BY user_id;

-- Refresh periodically
REFRESH MATERIALIZED VIEW mv_user_stats;
```

3. **Enable query caching:**
```python
from fraiseql.cache import query_cache

@query
@query_cache(ttl=300)  # Cache for 5 minutes
async def get_expensive_data(info):
    # Expensive query
    pass
```

### Problem: Memory Issues

**Symptoms:**
- Out of memory errors
- Process killed
- Slow response times

**Solutions:**

1. **Use pagination:**
```python
@query
async def get_posts(info, first: int = 20, after: Optional[str] = None):
    # Limit result size
    return await repo.find_many(
        "v_post",
        limit=min(first, 100),  # Cap at 100
        cursor=after
    )
```

2. **Stream large results:**
```python
async def export_data(info):
    # Use async generator for large datasets
    async for row in repo.stream("v_large_table"):
        yield process_row(row)
```

## Mutation Issues

### Problem: "Function does not exist"

**Symptoms:**
```
asyncpg.exceptions.UndefinedFunctionError: function fn_create_user does not exist
```

**Solutions:**

1. **Create PostgreSQL function:**
```sql
CREATE OR REPLACE FUNCTION fn_create_user(
    p_email TEXT,
    p_name TEXT
) RETURNS TABLE(status TEXT, id UUID, message TEXT) AS $$
BEGIN
    -- Validation
    IF p_email IS NULL OR p_email = '' THEN
        RETURN QUERY SELECT 'error'::TEXT, NULL::UUID, 'Email required'::TEXT;
        RETURN;
    END IF;

    -- Create user
    INSERT INTO users (email, name)
    VALUES (p_email, p_name)
    RETURNING 'success'::TEXT, id, 'User created'::TEXT;

EXCEPTION
    WHEN unique_violation THEN
        RETURN QUERY SELECT 'duplicate_email'::TEXT, NULL::UUID, 'Email already exists'::TEXT;
END;
$$ LANGUAGE plpgsql;
```

2. **Check function signature:**
```sql
-- List functions
\df fn_create_user

-- Check parameters
SELECT proname, proargnames, proargtypes
FROM pg_proc
WHERE proname = 'fn_create_user';
```

## Development Issues

### Problem: Changes Not Reflected

**Symptoms:**
- Code changes not working
- Old schema still active
- Cached responses

**Solutions:**

1. **Clear caches:**
```python
# Clear query cache
from fraiseql.cache import clear_cache
clear_cache()

# Restart application
import sys
sys.exit(0)  # Let process manager restart
```

2. **Reload schema:**
```python
# Force schema reload
from fraiseql import reload_schema
reload_schema()
```

3. **Check file watchers:**
```bash
# If using development server
export FRAISEQL_AUTO_RELOAD=true

# Or use nodemon/watchdog
nodemon --exec "python app.py"
```

## Recovery Strategies

### Automatic Recovery

```python
from fraiseql.recovery import auto_recover

@auto_recover(max_retries=3, backoff=2.0)
async def unstable_operation(info):
    # Automatically retried on failure
    pass
```

### Manual Recovery

```python
async def recover_from_error(error_type: str):
    """Manual recovery procedures."""

    if error_type == "connection_lost":
        # Reconnect to database
        await reconnect_database()

    elif error_type == "cache_corrupted":
        # Clear and rebuild cache
        await clear_all_caches()
        await warm_up_caches()

    elif error_type == "deadlock":
        # Retry transaction
        await retry_transaction()
```

### Health Checks

```python
async def health_check():
    """System health check."""
    checks = {
        "database": await check_database(),
        "cache": await check_cache(),
        "disk_space": await check_disk_space(),
        "memory": await check_memory()
    }

    if not all(checks.values()):
        # Trigger recovery procedures
        await recover_unhealthy_components(checks)

    return checks
```

## Getting Help

### Enable Debug Logging

```python
import logging

# Set debug level
logging.basicConfig(level=logging.DEBUG)

# Or use environment variable
export FRAISEQL_LOG_LEVEL=DEBUG
```

### Collect Diagnostic Information

```bash
# System info
fraiseql diagnose

# Database info
psql $DATABASE_URL -c "\d+"

# Schema info
fraiseql schema:export > schema.graphql
```

### Report Issues

When reporting issues, include:
1. Error message and stack trace
2. FraiseQL version: `fraiseql --version`
3. PostgreSQL version: `psql --version`
4. Minimal reproduction code
5. Expected vs actual behavior

## Next Steps

- Set up [debugging tools](./debugging.md)
- Implement [error monitoring](./debugging.md#error-monitoring)
- Review [error handling patterns](./handling-patterns.md)
