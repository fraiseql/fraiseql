# Troubleshooting Decision Tree

Quick diagnosis for common FraiseQL issues.

## 🚨 Problem Categories

**Choose your problem type:**

1. [Installation & Setup](#1-installation--setup-issues)
2. [Database Connection](#2-database-connection-issues)
3. [GraphQL Queries](#3-graphql-query-issues)
4. [Performance](#4-performance-issues)
5. [Deployment](#5-deployment-issues)
6. [Authentication](#6-authentication-issues)

---

## 1. Installation & Setup Issues

### ❌ "ModuleNotFoundError: No module named 'fraiseql'"

**Diagnosis:**
```bash
pip show fraiseql
```

**If not installed:**
```bash
pip install fraiseql
```

**If installed but still error:**
- ✅ Check you're using correct Python environment
- ✅ Verify virtual environment activated: `which python`
- ✅ Reinstall: `pip install --force-reinstall fraiseql`

---

### ❌ "ImportError: cannot import name 'type' from 'fraiseql'"

**Diagnosis:**
- Check Python version: `python --version`
- **Required**: Python 3.13+

**Fix:**
```bash
# Upgrade Python
pyenv install 3.10
pyenv global 3.10

# Or use system package manager
sudo apt install python3.10  # Ubuntu
brew install python@3.10     # macOS
```

---

### ❌ "Rust pipeline not found" or "RustError"

**Diagnosis:**
```bash
pip show fraiseql | grep Version
```

**Fix:**
```bash
# Install with Rust support
pip install "fraiseql[rust]"

# Verify Rust pipeline
python -c "from fraiseql.rust import RustPipeline; print('Rust OK')"
```

**If still failing:**
- Rust compiler required for building
- Install: https://rustup.rs/
- Then: `pip install --no-binary fraiseql "fraiseql[rust]"`

---

## 2. Database Connection Issues

### Decision Tree

```
❌ Cannot connect to database
    |
    ├─→ "Connection refused"
    |       └─→ PostgreSQL not running
    |           └─→ Start PostgreSQL: systemctl start postgresql
    |
    ├─→ "password authentication failed"
    |       └─→ Check DATABASE_URL credentials
    |           └─→ Verify: psql ${DATABASE_URL}
    |
    ├─→ "database does not exist"
    |       └─→ Create database: createdb fraiseql
    |
    └─→ "too many connections"
            └─→ Use PgBouncer connection pooler
                └─→ See: docs/production/deployment.md#pgbouncer
```

---

### ❌ "asyncpg.exceptions.InvalidPasswordError"

**Diagnosis:**
```bash
# Test connection manually
psql postgresql://user:password@localhost/dbname

# If works, check environment variable
echo $DATABASE_URL
```

**Fix:**
```bash
# Correct format:
export DATABASE_URL="postgresql://user:password@host:5432/database"

# Special characters in password? URL-encode them:
# @ → %40, # → %23, etc.
```

---

### ❌ "relation 'v_user' does not exist"

**Diagnosis:**
```sql
-- Check if view exists
SELECT table_name FROM information_schema.tables
WHERE table_schema = 'public' AND table_name = 'v_user';
```

**Fix:**
```sql
-- Create missing view
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data
FROM tb_user;
```

**Prevention:**
- Run migrations: `psql -f schema.sql`
- Check [DDL Organization Guide](../core/ddl-organization.md)

---

## 3. GraphQL Query Issues

### Decision Tree

```
❌ GraphQL query fails
    |
    ├─→ "Cannot query field 'X' on type 'Y'"
    |       └─→ Field not in GraphQL schema
    |           └─→ Check @type decorator includes field
    |
    ├─→ "Variable '$X' of type 'Y' used in position expecting 'Z'"
    |       └─→ Type mismatch in query
    |           └─→ Fix variable type or make nullable: String | null
    |
    ├─→ "Field 'X' of required type 'Y!' was not provided"
    |       └─→ Missing required field
    |           └─→ Add field or make optional in @input class
    |
    └─→ Query returns null unexpectedly
            └─→ Check PostgreSQL view returns data
                └─→ Run: SELECT data FROM v_table LIMIT 1;
```

---

### ❌ "Cannot return null for non-nullable field"

**Diagnosis:**
```python
import fraiseql

# Check type definition
@fraiseql.type(sql_source="v_user")
class User:
    id: int           # Required (non-nullable)
    name: str         # Required
    email: str | None # Optional (nullable)
```

**Fix:**

**Option 1**: Make field nullable in Python:
```python
@fraiseql.type(sql_source="v_user")
class User:
    name: str | None  # Now nullable
```

**Option 2**: Ensure PostgreSQL view never returns NULL:
```sql
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', COALESCE(name, 'Unknown'),  -- Never null
        'email', email  -- Can be null
    ) as data
FROM tb_user;
```

---

### ❌ "Expected type 'Int', found 'String'"

**Diagnosis:**
- Type mismatch between GraphQL schema and PostgreSQL

**Fix:**

**Python type** → **PostgreSQL type** mapping:
- `int` → `INTEGER`, `BIGINT`
- `str` → `TEXT`, `VARCHAR`
- `float` → `DOUBLE PRECISION`, `NUMERIC`
- `bool` → `BOOLEAN`
- `datetime` → `TIMESTAMP`, `TIMESTAMPTZ`

**Example fix:**
```python
import fraiseql

# Wrong
@fraiseql.type(sql_source="v_user")
class User:
    id: str  # PostgreSQL has INTEGER

# Correct
@fraiseql.type(sql_source="v_user")
class User:
    id: int  # Matches PostgreSQL INTEGER
```

---

## 4. Performance Issues

### Decision Tree

```
❌ Queries are slow
    |
    ├─→ N+1 query problem
    |       └─→ Use JSONB views with nested jsonb_agg
    |           └─→ See: performance/index.md#n-plus-one
    |
    ├─→ Missing database indexes
    |       └─→ Add indexes on foreign keys and WHERE clauses
    |           └─→ CREATE INDEX idx_post_user_id ON tb_post(user_id);
    |
    ├─→ Large result sets
    |       └─→ Implement pagination
    |           └─→ Use LIMIT/OFFSET or cursor-based
    |
    └─→ Connection pool exhausted
            └─→ Use PgBouncer
                └─→ See: production/deployment.md#pgbouncer
```

---

### ❌ "Too many connections to database"

**Diagnosis:**
```sql
-- Check current connections
SELECT count(*) FROM pg_stat_activity;
SELECT max_connections FROM pg_settings WHERE name = 'max_connections';
```

**Immediate fix:**
```sql
-- Kill idle connections
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE state = 'idle' AND state_change < now() - interval '5 minutes';
```

**Permanent fix:**

**Install PgBouncer:**
```bash
# Docker Compose
services:
  pgbouncer:
    image: pgbouncer/pgbouncer
    environment:
      - DATABASE_URL=postgresql://user:pass@db:5432/fraiseql
      - POOL_MODE=transaction
      - DEFAULT_POOL_SIZE=20
    ports:
      - "6432:6432"

# Update DATABASE_URL to use PgBouncer
DATABASE_URL=postgresql://user:pass@pgbouncer:6432/fraiseql
```

---

## 5. Deployment Issues

### ❌ "Health check failing in Kubernetes"

**Diagnosis:**
```bash
# Check pod logs
kubectl logs -f deployment/fraiseql-app -n fraiseql

# Test health endpoint manually
kubectl port-forward deployment/fraiseql-app 8000:8000 -n fraiseql
curl http://localhost:8000/health
```

**Common causes:**

1. **Database not ready:**
   ```yaml
   # Add initContainer to wait for database
   initContainers:
   - name: wait-for-db
     image: busybox
     command: ['sh', '-c', 'until nc -z postgres 5432; do sleep 1; done']
   ```

2. **Wrong DATABASE_URL:**
   ```yaml
   # Check secret
   kubectl get secret fraiseql-secrets -n fraiseql -o yaml
   echo "BASE64_STRING" | base64 -d
   ```

3. **Not enough resources:**
   ```yaml
   resources:
     requests:
       memory: "256Mi"  # Increase if OOMKilled
       cpu: "250m"
   ```

---

### ❌ "Container keeps restarting"

**Diagnosis:**
```bash
# Check exit code
kubectl describe pod <pod-name> -n fraiseql

# Common exit codes:
# 137 → OOMKilled (increase memory)
# 1   → Application error (check logs)
# 143 → SIGTERM (graceful shutdown, normal)
```

**Fix:**
```yaml
# Increase memory limit
resources:
  limits:
    memory: "1Gi"  # Was 512Mi

# Add startup probe (more time to start)
startupProbe:
  httpGet:
    path: /health
    port: 8000
  failureThreshold: 30  # 30 * 5s = 150s max startup
  periodSeconds: 5
```

---

## 6. Authentication Issues

### ❌ "@authorized decorator not working"

**Diagnosis:**
```python
# Check if user context is set
import fraiseql

@authorized(roles=["admin"])
@fraiseql.mutation
class DeletePost:
    async def resolve(self, info):
        # Check context
        print(f"User: {info.context.get('user')}")
        print(f"Roles: {info.context.get('roles')}")
```

**Fix:**

**Ensure context middleware sets user:**
```python
from fraiseql.fastapi import create_fraiseql_app

async def get_context(request):
    # Extract JWT token
    token = request.headers.get("Authorization", "").replace("Bearer ", "")

    # Decode token
    user = decode_jwt(token)

    # Return context with user and roles
    return {
        "user": user,
        "roles": user.get("roles", []),
        "request": request
    }

app = create_fraiseql_app(
    ...,
    context_getter=get_context
)
```

---

### ❌ "Row-Level Security blocking queries"

**Diagnosis:**
```sql
-- Check RLS policies
SELECT tablename, policyname, cmd, qual
FROM pg_policies
WHERE schemaname = 'public';

-- Test as specific user
SET ROLE tenant_user;
SELECT * FROM tb_post;  -- Should only see tenant's posts
```

**Fix:**

**If no rows returned when expected:**
```sql
-- Check if policy is correct
ALTER POLICY tenant_isolation ON tb_post
USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- Ensure tenant_id is set
SET app.current_tenant_id = 'tenant-uuid-here';

-- Test again
SELECT * FROM tb_post;
```

---

## 🆘 Still Stuck?

### Before Opening an Issue

1. **Search existing issues**: [GitHub Issues](../issues)
2. **Check discussions**: [GitHub Discussions](../discussions)
3. **Review documentation**: [Complete Docs](../../README.md)

### Opening a Good Issue

Include:
- **FraiseQL version**: `pip show fraiseql | grep Version`
- **Python version**: `python --version`
- **PostgreSQL version**: `psql --version`
- **Minimal reproduction**:  smallest code that reproduces issue
- **Error messages**: Full stack trace
- **What you've tried**: Show troubleshooting steps attempted

**Template:**
```markdown
## Environment
- FraiseQL: 1.0.0
- Python: 3.10.5
- PostgreSQL: 16.1
- OS: Ubuntu 22.04

## Issue
[Clear description of problem]

## Reproduction
\```python
# Minimal code to reproduce
\```

## Error
\```
Full error message
\```

## Attempted Fixes
- Tried X, result: Y
- Tried Z, result: W
```

---

## 📊 Most Common Issues

| Issue | Frequency | Quick Fix |
|-------|-----------|-----------|
| Wrong Python version | 40% | Use Python 3.13+ |
| DATABASE_URL format | 25% | Check postgresql://user:pass@host/db |
| Missing PostgreSQL view | 15% | Run schema.sql migrations |
| Connection pool exhausted | 10% | Use PgBouncer |
| Type mismatch (GraphQL) | 10% | Align Python types with PostgreSQL |

---

---

## 📖 Related Resources

- **[Detailed Troubleshooting Guide](troubleshooting.md)** - Specific error messages with step-by-step solutions
- **[GitHub Issues](../issues)** - Report bugs and search existing issues
- **[GitHub Discussions](../discussions)** - Ask questions and get help from the community
