# FraiseQL Quick Reference

One-page cheatsheet for common FraiseQL patterns, commands, and advanced type operations.

## Essential Commands

```bash
# Database setup
createdb mydb                                    # Create database
psql mydb < schema.sql                          # Load schema
psql mydb -c "\dv v_*"                          # List views
psql mydb -c "\dt tb_*"                         # List tables

# Run application
pip install fraiseql[all]                       # Install
uvicorn app:app --reload                        # Start server
curl http://localhost:8000/graphql              # Test endpoint

# Development
python -c "import app; print('OK')"             # Test imports
make test                                       # Run tests
```

## Essential Patterns

### Define a Type
```python
from fraiseql import type
from typing import List
from uuid import UUID

@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str
    posts: List['Post']  # Forward reference for relationships
```

### Query - Get All Items
```python
from fraiseql import query
from typing import List

@query
def users() -> List[User]:
    """Get all users."""
    pass  # Framework handles this
```

### Query - Get by ID
```python
from fraiseql import query
from uuid import UUID

@query
def user(id: UUID) -> User:
    """Get user by ID."""
    pass  # Framework handles this
```

### Query - Filter by Field
```python
from fraiseql import query
from typing import List

@query
def users_by_status(status: str) -> List[User]:
    """Get users by status."""
    pass  # Framework handles this
```

### Mutation - Create
```python
from fraiseql import mutation, input
from typing import Optional

@input
class CreateUserInput:
    name: str
    email: str

@mutation
def create_user(input: CreateUserInput) -> User:
    """Create a new user."""
    pass  # Framework calls fn_create_user
```

### Mutation - Update
```python
from fraiseql import mutation, input
from uuid import UUID

@input
class UpdateUserInput:
    name: Optional[str] = None
    email: Optional[str] = None

@mutation
def update_user(id: UUID, input: UpdateUserInput) -> User:
    """Update user."""
    pass  # Framework calls fn_update_user
```

### Mutation - Delete
```python
from fraiseql import mutation
from uuid import UUID

class DeleteResult:
    success: bool
    error: Optional[str]

@mutation
def delete_user(id: UUID) -> DeleteResult:
    """Delete user."""
    pass  # Framework calls fn_delete_user
```

## Type System & Custom Types

All custom types available in `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/`:

```python
from fraiseql.types import (
    IpAddress,      # IPv4/IPv6 - PostgreSQL inet/cidr
    LTree,          # Hierarchical paths - PostgreSQL ltree
    DateRange,      # Date ranges - PostgreSQL daterange
    MacAddress,     # MAC addresses - PostgreSQL macaddr
    Port,           # Network ports (1-65535) - smallint
    CIDR,           # CIDR notation - cidr type
    Date,           # ISO 8601 dates - date
    DateTime,       # ISO 8601 timestamps - timestamp
    EmailAddress,   # Email validation - text
    Hostname,       # DNS hostnames - text
    UUID,           # UUIDs - uuid
    JSON,           # JSON objects - jsonb
)
```

### Type Detection Priority

1. **Explicit type hint** (from @fraise_type decorator)
2. **Field name patterns** (contains "ip_address", "mac", "ltree", "daterange", etc.)
3. **Value heuristics** (IP address patterns, MAC formats, LTree notation, DateRange format)
4. **Default to STRING**

## Advanced Type Operators

### IP Address Operations (NetworkOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin", "nin"

# Network operations
"inSubnet",     # IP is in CIDR subnet
"inRange",      # IP in range {"from": "...", "to": "..."}
"isPrivate",    # RFC 1918 private
"isPublic",     # Non-private
"isIPv4",       # IPv4 only
"isIPv6",       # IPv6 only

# Classification (RFC-based)
"isLoopback",       # 127.0.0.0/8, ::1
"isLinkLocal",      # 169.254.0.0/16, fe80::/10
"isMulticast",      # 224.0.0.0/4, ff00::/8
"isDocumentation",  # RFC 3849/5737
"isCarrierGrade",   # RFC 6598 (100.64.0.0/10)
```

### LTree Hierarchical Paths (LTreeOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin"

# Hierarchical
"ancestor_of",     # path1 @> path2
"descendant_of",   # path1 <@ path2

# Pattern matching
"matches_lquery",      # path ~ lquery
"matches_ltxtquery"    # path ? ltxtquery

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
```

### DateRange Operations (DateRangeOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin"

# Range relationships
"contains_date",   # range @> date
"overlaps",        # range1 && range2
"adjacent",        # range1 -|- range2
"strictly_left",   # range1 << range2
"strictly_right",  # range1 >> range2
"not_left",        # range1 &> range2
"not_right"        # range1 &< range2

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
```

### Other Type Operations

**MAC Address (MacAddressOperatorStrategy):**
```python
"eq", "neq", "in", "notin", "isnull"
```

**Generic Types (ComparisonOperatorStrategy):**
```python
"eq", "neq", "gt", "gte", "lt", "lte"
```

**String Operations (PatternMatchingStrategy):**
```python
"matches",      # Regex pattern
"startswith",   # LIKE 'prefix%'
"contains",     # LIKE '%substr%'
"endswith"      # LIKE '%suffix'
```

**List Operations (ListOperatorStrategy):**
```python
"in",   # Value in list
"notin" # Value not in list
```

**All Types:**
```python
"isnull"  # IS NULL / IS NOT NULL
```

## GraphQL Query Examples

### Get all items
```graphql
query {
  users {
    id
    name
    email
  }
}
```

### Get by ID
```graphql
query {
  user(id: "123e4567-e89b-12d3-a456-426614174000") {
    name
    email
  }
}
```

### Filter results
```graphql
query {
  usersByStatus(status: "active") {
    id
    name
  }
}
```

### Create item
```graphql
mutation {
  createUser(input: { name: "Alice", email: "alice@example.com" }) {
    id
    name
    email
  }
}
```

### Update item
```graphql
mutation {
  updateUser(
    id: "123e4567-e89b-12d3-a456-426614174000"
    input: { name: "Alice Smith" }
  ) {
    id
    name
    email
  }
}
```

### Delete item
```graphql
mutation {
  deleteUser(id: "123e4567-e89b-12d3-a456-426614174000") {
    success
    error
  }
}
```

## PostgreSQL Patterns

### Table (Write Model)
```sql
-- tb_user - Write operations (trinity pattern)
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,           -- Internal only
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),  -- Public API
    identifier TEXT UNIQUE,                -- Optional human-readable
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    status TEXT DEFAULT 'active',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

### View (Read Model)
```sql
-- v_user - Read operations (uses public id, not pk_user)
CREATE VIEW v_user AS
SELECT
    jsonb_build_object(
        'id', id,              -- Use public UUID, not internal pk_user
        'name', name,
        'email', email,
        'status', status,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) as data
FROM tb_user
WHERE status != 'deleted';
```

### Function (Business Logic)
```sql
-- fn_create_user - Write operations (returns public UUID)
CREATE OR REPLACE FUNCTION fn_create_user(user_data JSONB)
RETURNS UUID AS $$
DECLARE
    new_id UUID;
BEGIN
    INSERT INTO tb_user (name, email)
    VALUES (user_data->>'name', user_data->>'email')
    RETURNING id INTO new_id;  -- Return public UUID, not pk_user

    RETURN new_id;
END;
$$ LANGUAGE plpgsql;
```

### Trigger (Auto-updates)
```sql
-- Auto-update updated_at
CREATE OR REPLACE FUNCTION fn_update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW
    EXECUTE FUNCTION fn_update_updated_at();
```

## FastAPI Integration

### Basic App
```python
from fastapi import FastAPI
from fraiseql.fastapi import FraiseQLRouter
from fraiseql.db import FraiseQLRepository
import asyncpg

# Database connection
pool = await asyncpg.create_pool("postgresql://user:pass@localhost/mydb")
repo = FraiseQLRepository(pool)

# FastAPI app
app = FastAPI()
router = FraiseQLRouter(repo=repo, schema=fraiseql.build_schema())
app.include_router(router, prefix="/graphql")
```

### With Custom Context
```python
from fraiseql.fastapi import FraiseQLRouter

# Add custom context
router = FraiseQLRouter(
    repo=repo,
    schema=fraiseql.build_schema(),
    context={"user_id": "current_user"}  # Available in resolvers
)
```

## File Structure

```
my-api/
├── app.py              # Main application
├── db/
│   ├── schema.sql     # Database schema
│   └── migrations/    # Schema changes
├── types.py           # GraphQL types
├── resolvers.py       # Queries & mutations
└── config.py          # Configuration
```

## Import Reference

```python
# Core decorators
from fraiseql import type, query, mutation, input, field

# Database
from fraiseql.db import FraiseQLRepository

# FastAPI integration
from fraiseql.fastapi import FraiseQLRouter

# Types
from typing import List, Optional
from uuid import UUID
from datetime import datetime
```

## Need More Help?

- [First Hour Guide](../FIRST_HOUR.md) - Progressive tutorial
- [Troubleshooting](../TROUBLESHOOTING.md) - Common issues
- [Understanding FraiseQL](../UNDERSTANDING.md) - Architecture overview
- [Examples](../../examples/) - Working applications
