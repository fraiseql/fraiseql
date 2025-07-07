# FraiseQL Documentation Implementation Plan

## Overview

This plan addresses the critical documentation gaps identified in the personas team assessment. The goal is to reduce support burden by 50-75% through clear, comprehensive documentation that prevents common errors.

## Priority Matrix

| Issue | Priority | Impact | Timeline | Success Metric |
|-------|----------|--------|----------|----------------|
| API Reference Missing | P0 | High | 5 days | 50% support reduction |
| Pattern Confusion | P0 | High | 3 days | 75% error reduction |
| Learning Progression | P1 | Medium | 4 days | Faster onboarding |
| Context Documentation | P1 | Medium | 2 days | 75% context errors |
| Quick Start Issues | P2 | Low | 2 days | <10 min first query |

## Phase 1: Critical API Reference (P0 - Days 1-5)

### 1.1 Core Decorators Reference
**File**: `docs/api-reference/decorators-complete.md`

```markdown
# Complete Decorator Reference

## @fraiseql.type
Define GraphQL object types...

### Syntax
@fraiseql.type
class TypeName:
    field: type

### Parameters
- No parameters

### Common Mistakes
❌ Using @fraise_type instead of @fraiseql.type
❌ Forgetting type annotations
✅ Always use @fraiseql.type with typed fields

### Examples
[Multiple complete examples]
```

### 1.2 Repository API Reference
**File**: `docs/api-reference/repository.md`

```markdown
# FraiseQLRepository API

## Overview
The repository handles all database operations...

## Methods

### find()
Query multiple records from a view

Parameters:
- view_name: str - The database view to query
- **kwargs: Filtering parameters

Returns: list[T] - List of instantiated objects

Example:
users = await db.find("user_view", tenant_id=tenant_id)

### find_one()
[Complete documentation]

### Common Patterns
[Multi-tenant, filtering, etc.]
```

### 1.3 Context API Reference
**File**: `docs/api-reference/context.md`

```markdown
# GraphQL Context Reference

## Default Context Structure
info.context = {
    "db": FraiseQLRepository,
    "user": UserContext | None,
    "authenticated": bool,
    "loader_registry": DataLoaderRegistry,
    "request": Request
}

## Accessing Context
@fraiseql.query
async def my_query(info) -> Result:
    db = info.context["db"]
    user = info.context.get("user")

## Custom Context
[How to add custom values]
```

## Phase 2: Pattern Documentation (P0 - Days 3-5)

### 2.1 Query Patterns Guide
**File**: `docs/patterns/queries.md`

```markdown
# FraiseQL Query Patterns

## The One True Pattern

### ✅ CORRECT: @fraiseql.query
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

### ❌ WRONG: resolve_ methods
class Query:
    async def resolve_users(self, info):
        # This will NOT work in FraiseQL!

## Why This Matters
FraiseQL uses a different pattern than traditional GraphQL...

## Complete Examples
[5-6 complete query examples]
```

### 2.2 Database Patterns Guide
**File**: `docs/patterns/database.md`

```markdown
# Database Patterns in FraiseQL

## The JSONB Data Column Pattern

### Why JSONB?
1. Type safety
2. Nested data support
3. Consistent API

### Creating Views
CREATE VIEW user_view AS
SELECT
    id,              -- ALWAYS include for filtering
    tenant_id,       -- Include all filter columns
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- REQUIRED: All data here

### Common Mistakes
❌ Missing 'data' column
❌ Returning columns directly
❌ Forgetting filter columns

## Repository Usage
[Complete examples]
```

### 2.3 Error Patterns Guide
**File**: `docs/patterns/error-handling.md`

```markdown
# Common Errors and Solutions

## 'NoneType' object has no attribute 'context'

### Cause
Using resolve_ prefix or wrong parameter order

### Solution
@fraiseql.query
async def query_name(info, param1, param2):
    # info MUST be first parameter

## Connection already closed

### Cause
Passing raw database connection

### Solution
Always use FraiseQLRepository from context
```

## Phase 3: Learning Path (P1 - Days 6-9)

### 3.1 Progressive Tutorial Series
**File**: `docs/tutorials/index.md`

```markdown
# Learning FraiseQL

## Your Journey
1. [Hello World](./01-hello-world.md) - No database
2. [First Database Query](./02-database-basics.md) - Simple view
3. [Adding Authentication](./03-authentication.md) - Secure queries
4. [Complex Queries](./04-complex-queries.md) - Joins and nesting
5. [Production Patterns](./05-production.md) - Multi-tenant, caching
```

### 3.2 Tutorial 1: Hello World
**File**: `docs/tutorials/01-hello-world.md`

```markdown
# Tutorial 1: Hello World

## What You'll Learn
- Define a GraphQL type
- Create a simple query
- Run your first API

## No Database Required!
[Complete working example]

## What's Next
[Link to database tutorial]
```

### 3.3 Tutorial 2: Database Basics
**File**: `docs/tutorials/02-database-basics.md`

```markdown
# Tutorial 2: Your First Database Query

## What You'll Learn
- Create a JSONB view
- Use FraiseQLRepository
- Handle query parameters

## Prerequisites
- Completed Tutorial 1
- PostgreSQL installed

## Step by Step
[Complete progression]
```

## Phase 4: Context Documentation (P1 - Days 10-11)

### 4.1 Context Customization Guide
**File**: `docs/advanced/context-complete.md`

```markdown
# Complete Context Guide

## Built-in Context

## Adding Custom Context
async def get_context(request):
    return {
        "db": repo,
        "tenant_id": extract_tenant(request),
        "feature_flags": get_features(request),
        # Your custom values
    }

## Multi-Tenant Context
[Complete example]

## Testing with Custom Context
[How to test]
```

### 4.2 Authentication Context
**File**: `docs/advanced/auth-context.md`

```markdown
# Authentication and Context

## How Auth Populates Context
1. Middleware validates token
2. User info added to context
3. Available in all queries

## Accessing User Info
@fraiseql.query
@requires_auth
async def me(info) -> User:
    user = info.context["user"]
    # user.user_id, user.email, user.roles

## Custom Auth Context
[Adding custom claims]
```

## Phase 5: Quick Start Improvements (P2 - Days 12-13)

### 5.1 Streamlined Quick Start
**File**: `docs/getting-started/quick-start-v2.md`

```markdown
# Quick Start (5 Minutes)

## 1. Install
pip install fraiseql

## 2. Create Your First API
# Complete, runnable code
import fraiseql

@fraiseql.type
class Message:
    text: str

@fraiseql.query
async def hello(info) -> Message:
    return Message(text="Hello, FraiseQL!")

app = fraiseql.create_fraiseql_app(
    types=[Message]
)

# Run: uvicorn app:app

## 3. Try It
http://localhost:8000/graphql

query {
  hello {
    text
  }
}

## Next: Add a Database
[Link to database guide]
```

### 5.2 Troubleshooting Guide
**File**: `docs/troubleshooting/common-issues.md`

```markdown
# Troubleshooting Guide

## Query Returns None

### Symptom
Query executes but returns null

### Common Causes
1. Wrong decorator pattern
2. Missing 'data' column in view
3. Parameter order issues

### Solutions
[Step-by-step debugging]

## Info Parameter is None
[Complete troubleshooting]
```

## Implementation Timeline

### Week 1
- Days 1-2: Core API reference (decorators, repository)
- Days 3-5: Pattern documentation (queries, database)

### Week 2
- Days 6-9: Progressive tutorials
- Days 10-11: Context documentation
- Days 12-13: Quick start improvements

## Success Metrics

### Immediate (Week 1)
- Zero "resolve_" pattern errors
- 50% reduction in "NoneType" errors
- Clear understanding of JSONB pattern

### Short-term (Week 2)
- New users create first query in <10 minutes
- 75% reduction in support questions
- Positive feedback on clarity

### Long-term (Month 1)
- Community contributions to docs
- Advanced pattern adoption
- Reduced onboarding time

## Documentation Standards

### Every Page Must Have
1. **Clear objective**: What you'll learn
2. **Complete examples**: Full, runnable code
3. **Common mistakes**: What not to do
4. **Next steps**: Where to go next

### Code Examples
- Must be complete and runnable
- Include all imports
- Show expected output
- Explain each step

### Error Examples
- Show actual error message
- Explain root cause
- Provide correct solution
- Link to relevant docs

## Rollout Plan

### Phase 1: Internal Review
- Core team reviews all P0 docs
- Test with new developer
- Iterate based on feedback

### Phase 2: Beta Release
- Publish to docs site
- Announce in community
- Gather feedback

### Phase 3: Full Release
- Integrate into main site
- Update all examples
- Deprecate old docs

## Maintenance Plan

### Weekly
- Review support questions
- Update FAQ/troubleshooting
- Add new examples

### Monthly
- Review analytics
- Update based on usage
- Add advanced topics

### Quarterly
- Major documentation review
- Community feedback session
- Plan next improvements

## Conclusion

This implementation plan addresses all critical documentation gaps identified by the personas team. By following this structured approach, we can dramatically reduce user confusion and support burden while accelerating adoption of FraiseQL.

The key is to be **explicit about patterns**, provide **complete examples**, and **guide users progressively** from simple to complex use cases.
