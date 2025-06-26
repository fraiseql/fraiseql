# FraiseQL Documentation Improvement Plan

Based on the personas team assessment and real-world user feedback (PrintOptim team), this plan addresses critical documentation gaps that are blocking adoption and causing user confusion.

## Priority Matrix

| Gap | Priority | Impact | Effort | Timeline |
|-----|----------|--------|--------|----------|
| API Reference Missing | P0 | High | 5 days | Week 1 |
| Pattern Confusion | P0 | High | 3 days | Week 1 |
| Learning Progression | P1 | Medium | 4 days | Week 2 |
| Context Documentation | P1 | Medium | 2 days | Week 2 |
| Migration Guides | P2 | Medium | 3 days | Week 3 |
| Quick Start Issues | P2 | Low | 2 days | Week 3 |

**Total Estimated Effort: 19 days (3-4 weeks)**

---

## Gap 1: Missing API Reference (P0 - Critical)

### Current Problem
- No comprehensive function/method reference
- Users can't find decorator signatures or options
- Repository API undocumented
- Causes "method not found" errors

### Evidence from Assessment
- Senior Developer: "API Reference: 3/10 (critical gap)"
- QA Lead: "Missing comprehensive API reference"
- PrintOptim issues: Multiple queries about repository methods

### Required Content

#### 1.1 Complete Decorator Reference (`docs/api/decorators.md`)
```markdown
# Decorator Reference

## @fraiseql.type
**Signature**: `@fraiseql.type`
**Purpose**: Defines a GraphQL type from a Python class

### Basic Usage
```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
```

### Advanced Options
```python
@fraiseql.type(name="CustomName")  # Override GraphQL type name
class User:
    # ...
```

### Field Types Supported
- Basic: str, int, float, bool
- UUID: Automatically converts to GraphQL ID
- datetime: ISO string format
- Optional[T]: Makes field nullable
- list[T]: GraphQL list type
```

#### 1.2 Repository API Reference (`docs/api/repository.md`)
```markdown
# FraiseQLRepository API

## Core Methods

### find(view_name, **kwargs) -> list[Union[dict, Type]]
Fetch multiple records from a database view.

**Parameters:**
- `view_name` (str): Database view name
- `limit` (int, optional): Maximum records to return
- `offset` (int, optional): Number of records to skip
- `where` (WhereInput, optional): Filter object
- `**kwargs`: Simple field=value filters

**Returns:**
- Development mode: List of instantiated type objects
- Production mode: List of dictionaries

**Example:**
```python
# Simple filtering
users = await db.find("user_view", status="active")

# With limit and offset
users = await db.find("user_view", limit=10, offset=20)

# With where object
users = await db.find("user_view", where=UserWhere(age={"gte": 18}))
```
```

#### 1.3 Context API Reference (`docs/api/context.md`)
```markdown
# GraphQL Context Reference

## Standard Context Properties

| Property | Type | Description | Always Available |
|----------|------|-------------|------------------|
| `db` | FraiseQLRepository | Database access | ✅ |
| `user` | UserContext \| None | Authenticated user | ✅ |
| `authenticated` | bool | Authentication status | ✅ |
| `loader_registry` | LoaderRegistry | DataLoader registry | ✅ |
| `mode` | str | "development" or "production" | ✅ |

## Safe Access Patterns
```python
# ✅ Safe - these are always available
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    user = info.context.get("user")  # May be None
    
    if info.context["authenticated"]:
        return await db.find("user_view")
    else:
        return []

# ❌ Unsafe - custom context may not be available
@fraiseql.query  
async def users(info) -> list[User]:
    tenant_id = info.context["tenant_id"]  # May not exist!
```
```

**Effort Estimate: 5 days**

---

## Gap 2: Pattern Confusion (P0 - Critical)

### Current Problem
- Users try resolver class patterns that don't work
- Mixed examples showing both old and new patterns  
- Query class confusion causes "no fields" errors
- Users attempt GraphQL patterns from other frameworks

### Evidence from Assessment
- Senior Developer: "Sometimes uses @fraise_type, sometimes @fraiseql.type"
- PrintOptim issues: "Type Query must define one or more fields"
- Multiple reports of resolver pattern confusion

### Required Content

#### 2.1 Anti-Patterns Guide (`docs/WHAT_NOT_TO_DO.md`)
```markdown
# What NOT to Do in FraiseQL

## ❌ Don't Use Resolver Classes
```python
# WRONG - This doesn't work in FraiseQL
class Query:
    async def resolve_users(self, info):
        pass

class Mutation:
    async def resolve_create_user(self, info, input):
        pass
```

**Why it fails**: FraiseQL doesn't use resolver classes. This pattern is from Strawberry/Graphene.

## ✅ Use Function Decorators Instead
```python
# CORRECT - FraiseQL uses function decorators
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    return await db.execute_function("graphql.create_user", input.dict())
```

## ❌ Don't Use Query Classes Without Fields
```python
# WRONG - Empty query classes cause errors
@fraiseql.type
class Query:
    pass  # Error: "Type Query must define one or more fields"
```

## ✅ Register Queries as Functions
```python
# CORRECT - Register functions directly
@fraiseql.query
async def users(info) -> list[User]:
    # ...

# Then in app creation
app = create_fraiseql_app(
    types=[User],  # Just the data types
    # Queries are auto-registered via decorator
)
```
```

#### 2.2 The FraiseQL Way Guide (`docs/THE_FRAISEQL_WAY.md`)
```markdown
# The FraiseQL Way

## Pattern 1: Types are Just Data
```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
    # No methods, no resolvers - just data structure
```

## Pattern 2: Queries are Functions  
```python
@fraiseql.query
async def users(info) -> list[User]:
    # 'info' is ALWAYS first parameter
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.query  
async def user(info, id: UUID) -> User | None:
    # Parameters after 'info'
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

## Pattern 3: Database Views Provide Data
```sql
-- All data goes in JSONB 'data' column
CREATE VIEW user_view AS
SELECT 
    id,              -- For filtering
    tenant_id,       -- For access control
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- REQUIRED: All object data here
FROM users;
```

## Pattern 4: Repository Handles Everything
```python
@fraiseql.query
async def users(info, status: str | None = None) -> list[User]:
    db = info.context["db"]
    
    if status:
        return await db.find("user_view", status=status)
    return await db.find("user_view")
```
```

**Effort Estimate: 3 days**

---

## Gap 3: Learning Progression Issues (P1 - High)

### Current Problem
- Jumps to complex examples too quickly
- JSONB pattern introduced without explanation
- No gradual learning path
- Users get overwhelmed and give up

### Evidence from Assessment
- Senior Developer: "README jumps to complex examples too quickly"
- Product Manager: "5-minute quick start isn't actually 5 minutes"

### Required Content

#### 3.1 Progressive Tutorial Series

##### Level 1: Hello World (10 minutes)
```markdown
# Level 1: Hello World (No Database)

**Goal**: Get GraphQL working in 10 minutes without any database setup.

## Step 1: Install FraiseQL
```bash
pip install fraiseql
```

## Step 2: Create Your First Type
```python
# app.py
from uuid import UUID, uuid4
from datetime import datetime
import fraiseql

@fraiseql.type
class Book:
    id: UUID
    title: str
    author: str
    published: datetime

@fraiseql.query
async def books(info) -> list[Book]:
    # Return some sample data
    return [
        Book(
            id=uuid4(),
            title="The Great Gatsby", 
            author="F. Scott Fitzgerald",
            published=datetime(1925, 4, 10)
        )
    ]

app = fraiseql.create_fraiseql_app(
    types=[Book],
    production=False  # Enables GraphQL Playground
)
```

## Step 3: Run and Test
```bash
uvicorn app:app --reload
# Open http://localhost:8000/graphql
```

## Step 4: Try Your First Query
```graphql
query {
  books {
    id
    title
    author
  }
}
```

✅ **Success criteria**: You see book data in GraphQL Playground
```

##### Level 2: Database Integration (20 minutes)
```markdown
# Level 2: Add Real Database (PostgreSQL Required)

**Goal**: Connect to PostgreSQL and query real data.

## Prerequisites
- PostgreSQL running locally
- Sample database with a `books` table

## Step 1: Create Database View
```sql
-- Required JSONB pattern
CREATE VIEW book_view AS
SELECT 
    id,              -- For filtering
    jsonb_build_object(
        'id', id,
        'title', title,
        'author', author,
        'published', published
    ) as data        -- All data in JSONB column
FROM books;
```

## Step 2: Update Your Query
```python
@fraiseql.query
async def books(info) -> list[Book]:
    db = info.context["db"]  # Get repository
    return await db.find("book_view")  # Query the view
```

## Step 3: Add Database URL
```python
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Book],
    production=False
)
```

✅ **Success criteria**: Query returns data from your PostgreSQL database
```

##### Level 3: Arguments and Filtering (30 minutes)
```markdown
# Level 3: Add Query Arguments

**Goal**: Filter data using query arguments.

## Step 1: Add Arguments to Query
```python
@fraiseql.query
async def books(info, author: str | None = None) -> list[Book]:
    db = info.context["db"]
    
    if author:
        return await db.find("book_view", author=author)
    return await db.find("book_view")
```

## Step 2: Test with Arguments
```graphql
query {
  books(author: "Fitzgerald") {
    title
    author
  }
}
```

## Step 3: Add More Complex Filtering
```python
@fraiseql.query
async def books(
    info, 
    author: str | None = None,
    limit: int = 10
) -> list[Book]:
    db = info.context["db"]
    return await db.find("book_view", author=author, limit=limit)
```

✅ **Success criteria**: Can filter books by author and limit results
```

**Effort Estimate: 4 days**

---

## Gap 4: Context Documentation (P1 - High)

### Current Problem
- Users don't know what's available in `info.context`
- Attribute errors when accessing custom context
- No examples of safe context access patterns

### Evidence from Assessment
- PrintOptim issues: "info.context attribute errors"
- No clear documentation on context structure

### Required Content

#### 4.1 Complete Context Guide (`docs/CONTEXT_GUIDE.md`)
```markdown
# GraphQL Context Guide

## What is Context?
The `info.context` object contains request-scoped data available to all resolvers. FraiseQL provides standard properties and allows custom additions.

## Standard Properties (Always Available)

### Database Access
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]  # FraiseQLRepository
    return await db.find("user_view")
```

### Authentication
```python
@fraiseql.query
async def profile(info) -> User | None:
    user = info.context.get("user")  # UserContext | None
    authenticated = info.context["authenticated"]  # bool
    
    if not authenticated:
        return None
    
    db = info.context["db"]
    return await db.find_one("user_view", id=user.user_id)
```

### Development Tools
```python
@fraiseql.query
async def debug_info(info) -> dict[str, str]:
    return {
        "mode": info.context["mode"],  # "development" | "production"
        "loader_registry": str(type(info.context["loader_registry"]))
    }
```

## Custom Context Patterns

### Multi-Tenant Context
```python
async def get_context(request: Request) -> dict[str, Any]:
    return {
        "tenant_id": request.headers.get("tenant-id"),
        "organization": request.headers.get("org-id"),
    }

@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")  # Safe access
    
    if not tenant_id:
        raise ValueError("Tenant ID required")
    
    return await db.find("user_view", tenant_id=tenant_id)
```

### Safe Context Access
```python
# ✅ Safe patterns
user = info.context.get("user")  # Returns None if not present
tenant_id = info.context.get("tenant_id", "default")  # With default

# Check before accessing
if "custom_property" in info.context:
    value = info.context["custom_property"]

# ❌ Unsafe patterns  
user = info.context["user"]  # May raise KeyError
tenant_id = info.context["tenant_id"]  # May not exist
```

## Troubleshooting Context Issues

### "KeyError: 'tenant_id'"
**Problem**: Custom context property not available
**Solution**: Use safe access patterns or check if context getter is configured

### "AttributeError: 'NoneType'"
**Problem**: Trying to access properties on None user
**Solution**: Check authentication status first

```python
# ❌ Causes AttributeError
user_id = info.context["user"].user_id

# ✅ Safe approach
user = info.context.get("user")
if user:
    user_id = user.user_id
```
```

**Effort Estimate: 2 days**

---

## Gap 5: Migration Guides (P2 - Medium)

### Current Problem
- Breaking changes buried in CHANGELOG
- JSONB pattern change not prominently documented
- No clear upgrade paths between versions

### Evidence from Assessment
- Multiple breaking changes in recent versions (a14, a15, a16, a17, a18)
- PrintOptim team struggled with migrations

### Required Content

#### 5.1 Version Migration Guide (`docs/MIGRATION_GUIDE.md`)
```markdown
# FraiseQL Migration Guide

## v0.1.0a18 - Partial Object Instantiation
**Status**: Non-breaking enhancement

### What Changed
- Added partial object instantiation for nested queries
- Missing required fields set to None in development mode

### Action Required
✅ **No action required** - this is a fix, not a breaking change

### Benefits
- Nested queries now work without requesting all fields
- Better GraphQL compliance

---

## v0.1.0a14 - JSONB Data Column Pattern
**Status**: ⚠️ **BREAKING CHANGE**

### What Changed
All database views must now return data in a JSONB `data` column.

### Before (v0.1.0a13)
```sql
CREATE VIEW user_view AS
SELECT id, name, email FROM users;
```

### After (v0.1.0a14+)
```sql
CREATE VIEW user_view AS
SELECT 
    id,              -- For filtering
    tenant_id,       -- For access control
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- REQUIRED: All object data here
FROM users;
```

### Migration Steps
1. **Update all views** to include JSONB data column
2. **Test queries** to ensure data is correctly formatted
3. **Update any raw SQL** to use new pattern

### Migration Script
```sql
-- Example migration for user_view
DROP VIEW IF EXISTS user_view;
CREATE VIEW user_view AS
SELECT 
    id,
    tenant_id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) as data
FROM users;
```

### Validation
```python
# Test that your views work
@fraiseql.query
async def test_users(info) -> list[User]:
    db = info.context["db"]
    users = await db.find("user_view", limit=1)
    print(f"Sample user: {users[0] if users else 'No users found'}")
    return users
```
```

**Effort Estimate: 3 days**

---

## Gap 6: Quick Start Issues (P2 - Medium)

### Current Problem
- "5-minute" claim is unrealistic
- Requires too much setup for first experience
- No validation steps to confirm success

### Evidence from Assessment
- Senior Developer: "5-minute quick start isn't actually 5 minutes"
- Users frustrated by setup complexity

### Required Content

#### 6.1 Realistic Quick Start (`docs/QUICK_START.md`)
```markdown
# FraiseQL Quick Start

Choose your path based on how much time you have:

## ⚡ 5 Minutes: Hello World (No Database)
Perfect for seeing FraiseQL in action without any setup.

**Prerequisites**: Python 3.11+

```python
# hello_fraiseql.py
from uuid import uuid4
import fraiseql

@fraiseql.type
class Greeting:
    id: str
    message: str

@fraiseql.query
async def hello(info, name: str = "World") -> Greeting:
    return Greeting(id=str(uuid4()), message=f"Hello, {name}!")

app = fraiseql.create_fraiseql_app(
    types=[Greeting],
    production=False
)
```

**Run it:**
```bash
pip install fraiseql uvicorn
uvicorn hello_fraiseql:app --reload
```

**Test it:** Open http://localhost:8000/graphql and try:
```graphql
query { hello(name: "FraiseQL") { message } }
```

✅ **Success**: You see "Hello, FraiseQL!" in the response

---

## 🗄️ 15 Minutes: With Database
Add PostgreSQL for real data persistence.

**Prerequisites**: 
- Python 3.11+
- PostgreSQL running locally
- Basic SQL knowledge

### Step 1: Database Setup (5 min)
```sql
-- Create database and table
CREATE DATABASE fraiseql_demo;
\c fraiseql_demo;

CREATE TABLE books (
    id SERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    author TEXT NOT NULL
);

INSERT INTO books (title, author) VALUES 
('1984', 'George Orwell'),
('Brave New World', 'Aldous Huxley');

-- Create FraiseQL view (REQUIRED pattern)
CREATE VIEW book_view AS
SELECT 
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'author', author
    ) as data
FROM books;
```

### Step 2: Python Code (5 min)
```python
# book_api.py
from uuid import UUID
import fraiseql

@fraiseql.type
class Book:
    id: int
    title: str
    author: str

@fraiseql.query
async def books(info, author: str | None = None) -> list[Book]:
    db = info.context["db"]
    
    if author:
        return await db.find("book_view", author=author)
    return await db.find("book_view")

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/fraiseql_demo",
    types=[Book],
    production=False
)
```

### Step 3: Test (5 min)
```bash
uvicorn book_api:app --reload
```

Try these queries at http://localhost:8000/graphql:
```graphql
# Get all books
query { books { id title author } }

# Filter by author  
query { books(author: "George Orwell") { title } }
```

✅ **Success**: You see your database books in the GraphQL response

---

## 🚀 30 Minutes: Production Ready
Add authentication, error handling, and production features.

[Additional content for production setup...]
```

**Effort Estimate: 2 days**

---

## Implementation Timeline

### Week 1: Critical Fixes (P0)
- **Day 1-2**: API Reference documentation
- **Day 3-5**: Pattern confusion fixes and anti-patterns guide

### Week 2: High Priority (P1)  
- **Day 6-9**: Progressive learning tutorial series
- **Day 10-11**: Complete context documentation

### Week 3: Medium Priority (P2)
- **Day 12-14**: Migration guides for all breaking changes
- **Day 15-16**: Realistic quick start guide

### Week 4: Review and Polish
- **Day 17-19**: Cross-reference verification and link checking
- **Day 20**: Final review and publication

## Success Metrics

### User Experience Improvements
- **50% reduction** in support queries about basic patterns
- **75% fewer** "info is None" or context-related errors  
- **90% successful** version upgrades with migration guides
- **Average time to first working query**: < 10 minutes

### Documentation Quality Metrics
- **API coverage**: 100% of public methods documented
- **Example accuracy**: All examples tested and verified
- **Link integrity**: No broken internal/external links
- **Search effectiveness**: Key concepts findable within 2 clicks

## Resource Requirements

### Personnel
- **1 Technical Writer** (full-time, 3-4 weeks)
- **1 Developer** (part-time, review and validation)
- **1 Designer** (part-time, documentation site improvements)

### Tools and Infrastructure
- Documentation site updates (if needed)
- Example validation CI pipeline
- Screenshot automation for visual examples

### Budget Estimate
- **Technical Writer**: $8,000-12,000 (3-4 weeks @ $200-300/day)
- **Developer time**: $2,000-3,000 (review and validation)
- **Tools/Infrastructure**: $500-1,000
- **Total**: $10,500-16,000

## Expected Impact

### Immediate (Week 1)
- Reduced confusion about basic patterns
- Clear API reference available for developers

### Short-term (Month 1)
- Improved onboarding success rate
- Reduced support burden on maintainers
- Better adoption metrics

### Long-term (Quarter 1)
- Faster team onboarding
- Higher developer satisfaction scores
- Reduced churn from documentation frustration
- Foundation for scaling documentation as project grows

This comprehensive plan directly addresses the critical documentation gaps that are currently blocking FraiseQL adoption and causing user frustration. Implementation of the P0 and P1 items alone would resolve most of the major documentation concerns raised by the personas team assessment.