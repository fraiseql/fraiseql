# Federation Service Migration Guide

## Overview

This guide provides step-by-step instructions for migrating existing GraphQL services to use Federation. It covers:
- Preparation and assessment
- Lift & shift strategies for resolvers
- Schema updates for federation support
- Testing migration
- Rollback procedures

---

## Pre-Migration Checklist

### Assessment Phase

**1. Identify Service Boundaries**

```python
# Audit your schema to understand service topology
# Example: In a multi-service architecture:

# User Service
- User (root type, can be extended)
- Profile
- Settings

# Post Service
- Post (references User)
- Comment
- Like

# Product Service
- Product (root type, can be extended)
- Inventory
- Pricing
```

**2. Catalog Existing Resolvers**

Create an inventory of all field resolvers:
```bash
# Find all @fraiseql.field decorators
grep -r "@fraiseql.field" src/

# Count resolver types
grep -r "async def" src/fraiseql/ | grep resolver | wc -l
```

**3. Database Schema Review**

Ensure tables support entity resolution:
```sql
-- Required for federation: _key field view
-- Should exist for each entity:
SELECT * FROM tv_user;  -- Creates views like this
SELECT * FROM tv_post;
SELECT * FROM tv_product;
```

**4. API Contract Documentation**

Document the contracts between services:
```
User Service:
- Provides: User entities (id, name, email, etc.)
- Accepts references from: Post (author_id), Comment (creator_id)
- Requires: POST /__entities endpoint

Post Service:
- Provides: Post entities (id, title, content, etc.)
- References: User (author_id → User.id)
- Requires: User federation support
```

---

## Migration Strategies

### Strategy 1: Lift & Shift (Recommended for Simple Services)

**Best for:** Services with <20 resolvers, simple resolver logic.

**Step 1: Add Federation Decorators**

```python
# Before (non-federated)
@fraiseql.type
class User:
    id: ID
    name: str
    email: str

# After (federated)
from fraiseql.federation import entity

@entity(keys=["id"])
@fraiseql.type
class User:
    id: ID
    name: str
    email: str
```

**Step 2: Add Reference Resolver**

```python
# Add resolver for cross-service references
@User.reference_resolver
async def resolve_user_reference(obj, info, key_values):
    """Resolve User entities by ID for federation."""
    # This is automatically batched by DataLoader
    pass
```

**Step 3: Update Schema SDL**

```graphql
# Before
extend schema {
  query: Query
}

# After
extend schema {
  query: Query
  _service: _Service!
}

type Query {
  # ... existing queries
  _entities(representations: [_Any!]!): [_Entity]!
}

type User @key(fields: "id") {
  id: ID!
  name: String!
  email: String
}
```

**Migration Impact:**
- ✅ No resolver logic changes needed
- ✅ Automatic batching via DataLoader
- ✅ Backward compatible (non-federation queries still work)
- ✅ Test coverage unchanged

---

### Strategy 2: Gradual Rollout (Recommended for Large Services)

**Best for:** Services with 20+ resolvers, complex business logic.

**Step 1: Enable Canary Mode**

```python
# In service configuration
config = FederationConfig(
    enabled=True,
    canary_percentage=5,  # Start with 5% traffic
)
```

**Step 2: Migrate One Entity Type**

```python
# Week 1: Migrate User entity
@entity(keys=["id"])
@fraiseql.type
class User:
    # ... same as before
    pass

# Week 2: Migrate Post entity after validation
@entity(keys=["id"])
@fraiseql.type
class Post:
    # ... same as before
    pass
```

**Step 3: Monitor and Validate**

```python
# Monitor federation metrics
@app.on_event("startup")
async def setup_monitoring():
    metrics = FederationMetrics()

    # Track per-entity
    metrics.track("User.federation_queries")
    metrics.track("User.dataloader_batches")
    metrics.track("User.error_rate")
```

**Step 4: Increase Canary Traffic**

```
Day 1-2:    5% traffic → federation
Day 3-4:   25% traffic → federation
Day 5-6:   50% traffic → federation
Day 7:    100% traffic → federation (full rollout)
```

---

## Detailed Migration Steps

### Phase 1: Preparation (1-2 days)

**1.1 Create Federation Service Wrapper**

```python
# src/services/federation_service.py
from fraiseql.federation import (
    create_federation_schema,
    EntityDataLoader,
)

class FederationService:
    """Wraps existing service with federation support."""

    def __init__(self, base_schema, db_pool):
        self.base_schema = base_schema
        self.db_pool = db_pool
        self.federation_schema = None

    async def initialize(self):
        """Initialize federation schema."""
        self.federation_schema = create_federation_schema(
            self.base_schema
        )

    async def handle_federation_request(self, request):
        """Handle federation __entities requests."""
        # Implemented by framework automatically
        pass
```

**1.2 Create Entity Registry**

```python
# src/entities.py
from fraiseql.federation import entity, clear_entity_registry

@entity(keys=["id"])
class User:
    """User entity for federation."""
    id: ID
    name: str
    email: str | None = None

@entity(keys=["id"])
class Post:
    """Post entity for federation."""
    id: ID
    title: str
    content: str
    author_id: str
```

**1.3 Set Up Testing Infrastructure**

```python
# tests/federation_migration/conftest.py
import pytest
from fraiseql.federation import EntityDataLoader

@pytest.fixture
def federation_executor(db_pool):
    """Executor for testing federation queries."""
    loader = EntityDataLoader(db_pool)
    return loader
```

### Phase 2: Resolver Migration (2-3 days)

**2.1 Identify Candidate Resolvers**

```python
# Categorize resolvers by complexity
# 1. Simple field resolvers (no dependencies)
#    → Migrate first
#
# 2. Dependent resolvers (need other fields)
#    → Migrate after dependencies
#
# 3. Complex resolvers (multiple queries, business logic)
#    → Migrate last or refactor first

# Examples:
# ✅ Simple: User.name → user.name (field access)
# ✅ Simple: User.email → user.email (field access)
# ⚠️  Dependent: Post.author → User.load(post.author_id)
# ⚠️  Complex: User.posts → db.query("SELECT * FROM posts WHERE author_id = ...")
```

**2.2 Migrate Field Resolvers**

```python
# Before: Traditional resolver
@fraiseql.field(User)
async def email(obj, info):
    # Direct access - no change needed
    return obj.email

# After: With federation context awareness
@fraiseql.field(User)
async def email(obj, info):
    # Federation provides obj directly as entity
    # No change needed - works the same!
    return obj.email
```

**2.3 Migrate Reference Resolvers**

```python
# Before: Each Post resolver loads User separately
@fraiseql.field(Post)
async def author(obj, info):
    db = info.context["db"]
    user = await db.fetchrow(
        "SELECT * FROM users WHERE id = $1",
        obj.author_id
    )
    return user  # N+1 problem!

# After: With DataLoader from federation context
@fraiseql.field(Post)
async def author(obj, info):
    # DataLoader is in context after federation migration
    loader = info.context.get("__dataloader__")
    if loader:
        # Federation: batched query
        user = await loader.load("User", obj.author_id)
    else:
        # Fallback: non-federation path
        db = info.context["db"]
        user = await db.fetchrow(
            "SELECT * FROM users WHERE id = $1",
            obj.author_id
        )
    return user
```

**2.4 Handle Circular References**

```python
# Common pattern: User ↔ Post cross-references
# User has many Posts, Post belongs to User

# User side
@entity(keys=["id"])
class User:
    id: ID
    name: str
    # Don't include posts here - would be circular

@fraiseql.field(User)
async def posts(obj, info):
    loader = info.context.get("__dataloader__")
    # Post.author will use DataLoader
    # So this won't cause N+1
    db = info.context["db"]
    return await db.fetch(
        "SELECT * FROM posts WHERE author_id = $1",
        obj.id
    )

# Post side
@entity(keys=["id"])
class Post:
    id: ID
    title: str
    author_id: str

@fraiseql.field(Post)
async def author(obj, info):
    loader = info.context.get("__dataloader__")
    return await loader.load("User", obj.author_id)
```

### Phase 3: Schema Updates (1 day)

**3.1 Generate Federation Schema**

```python
# The schema is generated automatically, but review it:
from fraiseql.federation import create_federation_schema

schema = create_federation_schema(base_schema)

# This adds:
# - @key directives to entity types
# - _entities query
# - _service query
# - Representations input types
```

**3.2 SDL Validation**

```bash
# Validate schema is still valid
apollo schema:validate schema.graphql --tag=production

# Check federation directives
grep "@key" schema.graphql
```

**3.3 Router Configuration**

```yaml
# apollo-router.yaml
federation:
  service_name: user-service

# Apollo Router will:
# 1. Detect federation support
# 2. Fetch SDL from _service.sdl
# 3. Register entity resolvers
```

---

## Testing Migration

### Unit Test Migration

```python
# Before: Non-federation test
@pytest.mark.asyncio
async def test_user_resolver():
    db = MockDB()
    result = await resolve_user({"id": "1"}, {"context": {"db": db}})
    assert result.name == "Alice"

# After: Federation-aware test
@pytest.mark.asyncio
async def test_user_resolver_with_federation(federation_executor):
    # Same test works with federation enabled
    result = await federation_executor.load("User", "1")
    assert result.name == "Alice"
```

### Integration Test Migration

```python
# Before: Direct resolver testing
query = """
query {
    user(id: "1") {
        name
        posts {
            title
        }
    }
}
"""

# After: Federation-aware testing
@pytest.mark.asyncio
async def test_user_with_federation(federation_executor, schema):
    """Test that posts are batched via federation."""
    # This should result in 1 query for users, 1 for posts
    # Instead of N+1 queries

    result = await execute_query(
        schema,
        query,
        executor=federation_executor
    )

    assert federation_executor.stats.total_queries == 2
```

### Migration Validation Checklist

```python
class MigrationValidator:
    """Validates successful federation migration."""

    @staticmethod
    async def validate_service(schema, db_pool):
        """Run all validation checks."""
        checks = [
            check_entity_registration(),      # ✅
            check_key_fields_exist(),          # ✅
            check_federation_schema(),         # ✅
            check_reference_resolvers(),       # ✅
            check_performance_baseline(),      # ✅
            check_error_handling(),            # ✅
        ]

        results = await asyncio.gather(*checks)

        return {
            "status": "ready" if all(results) else "failed",
            "checks": results
        }
```

---

## Rollback Procedures

### Immediate Rollback (Canary Mode)

```python
# If issues detected, disable federation automatically
class FederationHealthCheck:
    async def check_health(self):
        """Monitor federation health."""
        error_rate = await self.get_error_rate()

        if error_rate > 0.05:  # >5% error rate
            # Automatically disable federation
            await self.disable_federation()
            logger.error(
                f"Federation disabled due to high error rate: {error_rate}"
            )
```

### Manual Rollback Steps

**Step 1: Disable Federation**

```python
# In service configuration
config = FederationConfig(
    enabled=False,  # Disable federation
    canary_percentage=0,
)
```

**Step 2: Restart Service**

```bash
# Services will revert to non-federation behavior
# Existing connections finish normally
docker restart user-service
```

**Step 3: Monitor Metrics**

```python
# Verify non-federation metrics return to baseline
metrics = get_federation_metrics()
print(f"Federation disabled: {metrics['federation_enabled']}")
print(f"Error rate: {metrics['error_rate']}")
```

**Step 4: Post-Mortem**

```markdown
# Rollback Post-Mortem

## Timeline
- 2025-01-02 10:00 - Deployed federation v1
- 2025-01-02 10:15 - Error rate increased to 8%
- 2025-01-02 10:20 - Disabled federation
- 2025-01-02 10:30 - Metrics returned to baseline

## Root Cause
[Analyze logs and metrics]

## Fix
[Implement fix on dev branch]

## Re-deploy Plan
[Timeline for re-deployment after fix]
```

---

## Common Issues and Solutions

### Issue 1: N+1 Queries Still Occurring

**Symptom:** DataLoader shows high query count despite federation.

**Root Cause:** Resolver not using DataLoader from context.

**Solution:**
```python
# Check that resolver uses federation context
@fraiseql.field(Post)
async def author(obj, info):
    loader = info.context.get("__dataloader__")
    if not loader:
        logger.warning("DataLoader not in context - falling back to direct query")

    return await loader.load("User", obj.author_id)
```

### Issue 2: Entity Not Registered

**Symptom:** `UnregisteredEntityError: User not found in entity registry`

**Root Cause:** Entity class not decorated with `@entity`.

**Solution:**
```python
# Ensure @entity decorator is present
@entity(keys=["id"])  # ← Required
@fraiseql.type
class User:
    id: ID
```

### Issue 3: Key Field Mismatch

**Symptom:** `KeyMismatchError: Expected key 'id', got 'user_id'`

**Root Cause:** Entity definition doesn't match actual key in data.

**Solution:**
```python
# Use correct key field name
@entity(keys=["user_id"])  # ← Match actual data field
@fraiseql.type
class User:
    user_id: ID
    name: str
```

### Issue 4: Circular Dependencies

**Symptom:** Services waiting for each other's federation schemas.

**Root Cause:** Missing service discovery mechanism.

**Solution:**
```python
# Use federation list instead of router discovery
services = FederationServiceList([
    ("user-service", "http://localhost:4001"),
    ("post-service", "http://localhost:4002"),
])

# Services can be added in any order
```

---

## Performance Expectations

### Before Federation (N+1 Queries)

```
Query: User + Posts + Authors

User Service:
  1 query: SELECT user WHERE id = ?
  N queries: SELECT posts WHERE author_id = ?

Post Service:
  N queries: SELECT post WHERE id = ?
  N queries: SELECT author (via federation?)

Total: 2N+3 queries ❌
```

### After Federation (Batched)

```
Query: User + Posts + Authors

User Service:
  1 query: SELECT users WHERE id IN (...)
  1 query: SELECT posts WHERE author_id IN (...)

Post Service:
  1 query: SELECT posts WHERE id IN (...)
  1 query: SELECT users WHERE id IN (...)  [batched!]

Total: 4 queries ✅
```

**Expected Improvement:**
- Query count: Reduced by 80-95%
- Response time: 7-10x faster
- Database load: Reduced by 70-80%

---

## Validation Checklist

Before considering migration complete:

- [ ] All entities decorated with `@entity`
- [ ] Key fields defined and match data schema
- [ ] Reference resolvers using DataLoader from context
- [ ] Federation tests passing (all scenarios)
- [ ] Performance baseline established (7-10x improvement)
- [ ] Error handling tested (missing entities, timeouts)
- [ ] Rollback procedures documented and tested
- [ ] Monitoring and alerting configured
- [ ] Team trained on federation concepts
- [ ] Post-deployment runbook created

---

## Next Steps

1. **Start with Assessment** - Map your service architecture
2. **Choose Migration Strategy** - Lift & Shift vs Gradual Rollout
3. **Run Tests** - Ensure compatibility before migration
4. **Deploy Canary** - Start with small percentage
5. **Monitor Metrics** - Track performance and errors
6. **Gradual Rollout** - Increase traffic as confidence grows
7. **Full Deployment** - 100% traffic to federation
8. **Monitor Production** - Keep watching metrics for 2 weeks

For testing patterns, see [07-testing-guide.md](07-testing-guide.md).

For release procedures, see [09-release-checklist.md](09-release-checklist.md).
