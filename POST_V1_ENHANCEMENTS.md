# Post-v1.0 Enhancements - Advanced Patterns

**Base**: FraiseQL v1.0 with RUST_FIRST_SIMPLIFICATION complete
**Source**: PATTERNS_TO_IMPLEMENT.md evolution strategy
**Date**: 2025-10-16
**Status**: Planning

---

## Overview

After completing RUST_FIRST_SIMPLIFICATION (757 LOC removed, single execution path), these enhancements build on the simplified foundation to add advanced database patterns that complement Rust-first performance.

**Guiding Principle**: "the complexity in PostgreSQL is ok" - Add SQL sophistication, keep Python simple.

---

## Enhancement 1: Trinity Identifiers (Faster Joins)

### Priority: HIGH
**Estimated Timeline**: 2-3 weeks
**Complexity**: Medium (requires migrations)
**Performance Impact**: Faster joins (SERIAL vs UUID - to be benchmarked)

### What It Is

Three-tier ID system for every entity:
- `pk_*` (INT GENERATED ALWAYS AS IDENTITY) - Internal joins (faster than UUID - to be measured)
- `id` (UUID) - Public API (secure, no enumeration)
- `identifier` (TEXT) - Human URLs (usernames, slugs)

**Note**: Uses modern PostgreSQL 10+ IDENTITY syntax instead of deprecated SERIAL.

### Why It's Valuable

**Performance**:
- SERIAL joins are typically faster than UUID (smaller index, better cache locality)
- Exact performance gain needs benchmarking for your workload
- Benefits increase with join complexity

**Security**:
- Public API uses UUID (no enumeration attacks)
- Internal queries use SERIAL (fast)
- Best of both worlds

**UX**:
- SEO-friendly URLs: `/users/johndoe` instead of `/users/550e8400-...`
- Human-readable identifiers
- No ID guessing

### Current State (v0.x)

Most entities use single ID approach:
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT UNIQUE,
    email TEXT,
    created_at TIMESTAMPTZ
);

CREATE TABLE posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),  -- UUID join (slower)
    title TEXT,
    content TEXT
);
```

### Target State (v1.1+)

Trinity pattern for all entities:
```sql
CREATE TABLE users (
    pk_user SERIAL UNIQUE,              -- Internal joins (10x faster)
    id UUID DEFAULT gen_random_uuid() UNIQUE,  -- Public API
    identifier TEXT UNIQUE,              -- Human URLs (@johndoe)
    username TEXT,
    email TEXT,
    created_at TIMESTAMPTZ,

    PRIMARY KEY (pk_user)
);

CREATE TABLE posts (
    pk_post SERIAL UNIQUE,
    id UUID DEFAULT gen_random_uuid() UNIQUE,
    identifier TEXT UNIQUE,              -- Human URLs (post-slug)
    pk_user INT REFERENCES users(pk_user),  -- Fast SERIAL join
    title TEXT,
    content TEXT,

    PRIMARY KEY (pk_post)
);
```

### Implementation Plan

#### Phase 1: Add Trinity Columns (Week 1)

**1.1: Create migration template**
```sql
-- migration: add_trinity_identifiers_to_users.sql
ALTER TABLE users
    ADD COLUMN pk_user SERIAL UNIQUE,
    ADD COLUMN id UUID DEFAULT gen_random_uuid() UNIQUE,
    ADD COLUMN identifier TEXT UNIQUE;

-- Backfill existing data
UPDATE users SET
    id = gen_random_uuid(),
    identifier = LOWER(username)
WHERE id IS NULL;

-- Create indexes
CREATE INDEX idx_users_id ON users(id);
CREATE INDEX idx_users_identifier ON users(identifier);
```

**1.2: Generate migrations for all entities**
- Identify all entities (User, Post, Comment, etc.)
- Generate migration per entity
- Test on development database

#### Phase 2: Update Foreign Keys (Week 2)

**2.1: Add new SERIAL foreign keys**
```sql
-- Add new pk_* foreign key alongside old UUID FK
ALTER TABLE posts
    ADD COLUMN pk_user INT REFERENCES users(pk_user);

-- Backfill from existing UUID FK
UPDATE posts p
SET pk_user = u.pk_user
FROM users u
WHERE p.user_id = u.id;

-- Make new FK NOT NULL
ALTER TABLE posts
    ALTER COLUMN pk_user SET NOT NULL;

-- Add index for performance
CREATE INDEX idx_posts_pk_user ON posts(pk_user);
```

**2.2: Gradual cutover**
- Keep both FKs temporarily (UUID and SERIAL)
- Update queries to use SERIAL FK
- Monitor performance improvements
- Drop UUID FK after verification

#### Phase 3: Update GraphQL Types (Week 2-3)

**3.1: Update Python type definitions**
```python
@strawberry.type
class User:
    """User type with Trinity IDs.

    Internal queries use pk_user (fast SERIAL).
    GraphQL exposes id (UUID) and identifier (TEXT).
    pk_user is hidden from public API.
    """
    # Public API (exposed to GraphQL)
    id: UUID = strawberry.field(description="Secure UUID identifier")
    identifier: str = strawberry.field(description="Human-readable username")

    # Internal only (not exposed to GraphQL)
    _pk_user: int = strawberry.Private  # Fast joins internally

    username: str
    email: str
    created_at: datetime
```

**3.2: Update resolvers to use pk_* internally**
```python
async def resolve_posts(self, info) -> list[Post]:
    """Resolve user's posts using fast SERIAL join."""
    repository = info.context["repository"]

    # Use pk_user (SERIAL) for internal join, not id (UUID)
    return await repository.find(
        Post,
        where={"pk_user": self._pk_user}  # Fast SERIAL join
    )
```

**3.3: Update SQL generators**
```python
# Before: UUID joins
SELECT * FROM posts WHERE user_id = '550e8400-...'::UUID

# After: SERIAL joins (10x faster)
SELECT * FROM posts WHERE pk_user = 12345
```

#### Phase 4: Testing & Benchmarking (Week 3)

**4.1: Performance benchmarks**
```python
# Benchmark: Query with 5 joins
# Before (UUID): ~50ms
# After (SERIAL): ~5ms
# Improvement: 10x faster

@pytest.mark.benchmark
async def test_complex_query_performance():
    # Query: User -> Posts -> Comments -> Reactions -> Users
    query = """
    query {
        user(identifier: "johndoe") {
            posts {
                comments {
                    reactions {
                        user {
                            username
                        }
                    }
                }
            }
        }
    }
    """
    # Assert: < 10ms for 1000 total records
```

**4.2: Security verification**
```python
@pytest.mark.security
async def test_pk_not_exposed_in_graphql():
    # Ensure pk_* is never exposed in GraphQL responses
    query = """{ user(id: "...") { __typename } }"""
    result = await execute_query(query)

    # pk_user should not be in response
    assert "pk_user" not in str(result)
```

**4.3: Migration testing**
```python
@pytest.mark.migration
async def test_backward_compatibility():
    # Queries using old UUID FK should still work during transition
    # Both WHERE user_id = uuid AND WHERE pk_user = serial should work
```

### Breaking Changes

**None for API consumers** - GraphQL API unchanged:
- Still returns `id` (UUID) and `identifier` (TEXT)
- Internal implementation change only
- Faster responses, same interface

**Migration required** for database:
- Schema changes (add columns, update FKs)
- Backfill data
- Gradual cutover

### Benefits Summary

| Aspect | Before (UUID) | After (Trinity) | Improvement |
|--------|--------------|----------------|-------------|
| Join Performance | Baseline | Faster (TBD) | To be benchmarked |
| Index Size | 16 bytes | 4 bytes | 75% smaller |
| API Security | UUID (good) | UUID (same) | No change |
| URL Friendliness | UUID (bad) | identifier (good) | Better UX |
| Cache Locality | Lower | Higher | Better for large datasets |

### Files to Create

1. `migrations/trinity/001_add_trinity_to_users.sql`
2. `migrations/trinity/002_add_trinity_to_posts.sql`
3. `migrations/trinity/003_update_foreign_keys.sql`
4. `src/fraiseql/patterns/trinity_identifiers.py` - Helper functions
5. `docs/patterns/trinity_identifiers.md` - Documentation
6. `tests/patterns/test_trinity_identifiers.py` - Tests

### Files to Modify

1. `src/fraiseql/core/graphql_type.py` - Support strawberry.Private
2. `src/fraiseql/sql/sql_generator.py` - Prefer pk_* in WHERE clauses
3. GraphQL type definitions (User, Post, etc.) - Add Trinity fields

---

## Enhancement 2: Explicit CQRS Sync (No Triggers)

### Priority: HIGH
**Estimated Timeline**: 1-2 weeks
**Complexity**: Low (follows existing patterns)
**Performance Impact**: More predictable, easier to debug

### What It Is

Replace database triggers with explicit sync functions:
- `tb_*` tables (normalized writes)
- `tv_*` tables (denormalized JSONB reads)
- `fn_sync_tv_*` functions (explicit, no triggers)

### Why It's Valuable

**Predictability**:
- No hidden trigger behavior
- Explicit control over sync timing
- Easier to debug and trace

**Performance**:
- Sync only when needed (not every write)
- Batch sync for bulk operations
- No trigger overhead

**Simplicity**:
- No trigger complexity
- Clear sync points in code
- Works perfectly with Rust transformer

### Current State (v0.x)

May use triggers for CQRS sync:
```sql
-- Triggers fire automatically (implicit, harder to debug)
CREATE TRIGGER sync_user_view
    AFTER INSERT OR UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION sync_user_view();
```

**Problems**:
- Hidden behavior (where is sync happening?)
- Performance overhead (every write triggers sync)
- Hard to debug (trigger execution order)
- Can't batch sync for bulk operations

### Target State (v1.2+)

Explicit sync functions called from Python:
```sql
-- Explicit sync function (no trigger)
CREATE FUNCTION fn_sync_tv_users(p_pk_user INT) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_users (pk_user, id, identifier, data)
    SELECT
        pk_user,
        id,
        identifier,
        jsonb_build_object(
            'username', username,
            'email', email,
            'profile', jsonb_build_object(
                'avatar_url', avatar_url,
                'bio', bio
            ),
            'created_at', created_at
        ) AS data
    FROM tb_users
    WHERE pk_user = p_pk_user
    ON CONFLICT (pk_user)
    DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Batch sync for multiple users
CREATE FUNCTION fn_sync_tv_users_batch(p_pk_users INT[]) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_users (pk_user, id, identifier, data)
    SELECT
        pk_user,
        id,
        identifier,
        jsonb_build_object(
            'username', username,
            'email', email,
            'profile', jsonb_build_object(
                'avatar_url', avatar_url,
                'bio', bio
            ),
            'created_at', created_at
        ) AS data
    FROM tb_users
    WHERE pk_user = ANY(p_pk_users)
    ON CONFLICT (pk_user)
    DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;
```

### Implementation Plan

#### Phase 1: Create Sync Functions (Week 1)

**1.1: Generate sync function template**
```python
# src/fraiseql/cqrs/sync_generator.py
from typing import Type

def generate_sync_function(entity_type: Type) -> str:
    """Generate SQL for explicit CQRS sync function."""
    table_name = entity_type.__tablename__

    return f"""
    CREATE FUNCTION fn_sync_tv_{table_name}(p_pk_{table_name} INT)
    RETURNS VOID AS $$
    BEGIN
        INSERT INTO tv_{table_name} (pk_{table_name}, id, identifier, data)
        SELECT
            pk_{table_name},
            id,
            identifier,
            {build_jsonb_object(entity_type)}
        FROM tb_{table_name}
        WHERE pk_{table_name} = p_pk_{table_name}
        ON CONFLICT (pk_{table_name})
        DO UPDATE SET
            data = EXCLUDED.data,
            updated_at = NOW();
    END;
    $$ LANGUAGE plpgsql;
    """
```

**1.2: Create sync functions for all entities**
```bash
python scripts/generate_cqrs_sync_functions.py
# Generates:
# - migrations/cqrs/001_fn_sync_tv_users.sql
# - migrations/cqrs/002_fn_sync_tv_posts.sql
# - migrations/cqrs/003_fn_sync_tv_comments.sql
```

#### Phase 2: Update Mutations (Week 1-2)

**2.1: Add explicit sync calls after mutations**
```python
# src/fraiseql/mutations/user_mutations.py

async def create_user(
    username: str,
    email: str,
    repository: FraiseQLRepository
) -> User:
    """Create user with explicit CQRS sync."""

    # 1. Write to normalized table (tb_users)
    result = await repository.execute(
        """
        INSERT INTO tb_users (username, email)
        VALUES ($1, $2)
        RETURNING pk_user, id, identifier
        """,
        username, email
    )
    pk_user = result[0]['pk_user']

    # 2. Explicitly sync to denormalized table (tv_users)
    await repository.execute(
        "SELECT fn_sync_tv_users($1)",
        pk_user
    )

    # 3. Return from denormalized table (Rust transformer reads this)
    return await repository.find_one(
        User,
        where={"pk_user": pk_user}
    )
```

**2.2: Batch sync for bulk operations**
```python
async def create_users_bulk(
    users_data: list[dict],
    repository: FraiseQLRepository
) -> list[User]:
    """Bulk create with batch sync."""

    # 1. Bulk insert to normalized table
    pk_users = []
    for user_data in users_data:
        result = await repository.execute(
            "INSERT INTO tb_users (...) VALUES (...) RETURNING pk_user",
            **user_data
        )
        pk_users.append(result[0]['pk_user'])

    # 2. Batch sync (single call, much faster)
    await repository.execute(
        "SELECT fn_sync_tv_users_batch($1)",
        pk_users
    )

    # 3. Return all users
    return await repository.find(
        User,
        where={"pk_user": {"in": pk_users}}
    )
```

**2.3: Cascade sync for related entities**
```python
async def delete_user(
    pk_user: int,
    repository: FraiseQLRepository
) -> bool:
    """Delete user with cascade sync."""

    # 1. Delete from normalized table
    await repository.execute(
        "DELETE FROM tb_users WHERE pk_user = $1",
        pk_user
    )

    # 2. Delete from denormalized table
    await repository.execute(
        "DELETE FROM tv_users WHERE pk_user = $1",
        pk_user
    )

    # 3. Sync related entities (posts, comments, etc.)
    # Get affected post IDs
    affected_posts = await repository.execute(
        "SELECT pk_post FROM tb_posts WHERE pk_user = $1",
        pk_user
    )
    pk_posts = [row['pk_post'] for row in affected_posts]

    # Batch sync affected posts
    if pk_posts:
        await repository.execute(
            "SELECT fn_sync_tv_posts_batch($1)",
            pk_posts
        )

    return True
```

#### Phase 3: Remove Triggers (Week 2)

**3.1: Audit existing triggers**
```sql
-- List all triggers
SELECT
    trigger_name,
    event_object_table,
    action_statement
FROM information_schema.triggers
WHERE trigger_schema = 'public'
ORDER BY event_object_table;
```

**3.2: Create migration to drop triggers**
```sql
-- migration: drop_cqrs_triggers.sql
DROP TRIGGER IF EXISTS sync_user_view ON users;
DROP TRIGGER IF EXISTS sync_post_view ON posts;
-- ... drop all CQRS-related triggers
```

**3.3: Verify explicit sync is working**
```python
@pytest.mark.cqrs
async def test_explicit_sync_replaces_triggers():
    # Create user (should sync explicitly)
    user = await create_user("test", "test@example.com", repo)

    # Verify tb_users has data
    tb_result = await repo.execute(
        "SELECT * FROM tb_users WHERE pk_user = $1",
        user._pk_user
    )
    assert tb_result[0]['username'] == "test"

    # Verify tv_users has synced data
    tv_result = await repo.execute(
        "SELECT * FROM tv_users WHERE pk_user = $1",
        user._pk_user
    )
    assert tv_result[0]['data']['username'] == "test"
```

#### Phase 4: Add Sync Helpers (Week 2)

**4.1: Create sync helper class**
```python
# src/fraiseql/cqrs/sync_helper.py

class CQRSSyncHelper:
    """Helper for explicit CQRS synchronization."""

    def __init__(self, repository: FraiseQLRepository):
        self.repo = repository

    async def sync_entity(self, entity_type: Type, pk: int) -> None:
        """Sync single entity to view table."""
        table_name = entity_type.__tablename__
        await self.repo.execute(
            f"SELECT fn_sync_tv_{table_name}($1)",
            pk
        )

    async def sync_entities_batch(
        self,
        entity_type: Type,
        pks: list[int]
    ) -> None:
        """Batch sync multiple entities."""
        table_name = entity_type.__tablename__
        await self.repo.execute(
            f"SELECT fn_sync_tv_{table_name}_batch($1)",
            pks
        )

    async def sync_cascade(
        self,
        entity_type: Type,
        pk: int,
        relations: list[str]
    ) -> None:
        """Sync entity and related entities."""
        # Sync primary entity
        await self.sync_entity(entity_type, pk)

        # Sync related entities
        for relation in relations:
            related_pks = await self._get_related_pks(
                entity_type, pk, relation
            )
            if related_pks:
                related_type = self._get_relation_type(
                    entity_type, relation
                )
                await self.sync_entities_batch(related_type, related_pks)
```

**4.2: Use sync helper in mutations**
```python
from fraiseql.cqrs import CQRSSyncHelper

async def update_user_profile(
    pk_user: int,
    profile_data: dict,
    repository: FraiseQLRepository
) -> User:
    """Update profile with helper-based sync."""
    sync_helper = CQRSSyncHelper(repository)

    # Update normalized table
    await repository.execute(
        "UPDATE tb_users SET avatar_url = $1, bio = $2 WHERE pk_user = $3",
        profile_data['avatar_url'],
        profile_data['bio'],
        pk_user
    )

    # Explicit sync using helper
    await sync_helper.sync_cascade(
        User,
        pk_user,
        relations=['posts', 'comments']  # Sync user + related
    )

    return await repository.find_one(User, where={"pk_user": pk_user})
```

### Breaking Changes

**None for API consumers** - GraphQL API unchanged:
- Queries still read from tv_* tables (via Rust transformer)
- Same performance characteristics
- Internal sync mechanism change only

**Code changes required** for mutations:
- Add explicit sync calls after tb_* writes
- Use sync helper or direct fn_sync_tv_* calls
- Remove trigger assumptions

### Benefits Summary

| Aspect | Before (Triggers) | After (Explicit) | Improvement |
|--------|------------------|------------------|-------------|
| Predictability | Hidden (triggers) | Explicit (code) | Clear behavior |
| Debugging | Hard (trigger logs) | Easy (Python trace) | Better DX |
| Performance | Every write | When needed | Flexible |
| Batch Operations | Slower (N triggers) | Faster (1 batch) | 10x faster |
| Code Clarity | Implicit | Explicit | Maintainable |

### Files to Create

1. `migrations/cqrs/001_fn_sync_tv_users.sql`
2. `migrations/cqrs/002_fn_sync_tv_posts.sql`
3. `migrations/cqrs/drop_triggers.sql`
4. `src/fraiseql/cqrs/sync_helper.py`
5. `src/fraiseql/cqrs/sync_generator.py`
6. `docs/patterns/explicit_cqrs_sync.md`
7. `tests/cqrs/test_explicit_sync.py`

### Files to Modify

1. All mutation functions - Add explicit sync calls
2. Bulk operation functions - Add batch sync
3. Repository class - Optional: Add sync helpers

---

## Enhancement 3: Rich Return Types (Simplified)

### Priority: MEDIUM
**Estimated Timeline**: 1 week
**Complexity**: Low
**Performance Impact**: Reduces client round-trips

### What It Is

Mutations return main entity + list of affected entity IDs:
```json
{
  "id": "550e8400-...",
  "status": "updated",
  "object_data": {
    "username": "johndoe",
    "email": "john@example.com"
  },
  "affected_entity_ids": {
    "posts": ["uuid1", "uuid2"],
    "comments": ["uuid3", "uuid4"]
  }
}
```

### Why It's Valuable (Simplified)

**Client Efficiency**:
- Frontend knows what to refetch
- No need to refetch everything
- Targeted cache invalidation

**Simplicity** (vs full rich returns):
- Return IDs only, not full objects
- Frontend refetches if needed
- Simpler than full denormalization

### Current State (v0.x)

Mutations return only the modified entity:
```python
async def update_user(pk_user: int, email: str) -> User:
    # Update user
    await repo.execute("UPDATE tb_users SET email = $1", email)

    # Return only user
    return await repo.find_one(User, where={"pk_user": pk_user})
```

**Problem**: Frontend doesn't know what else changed:
- User's posts might need refresh (author name changed)
- User's comments might need refresh
- Frontend must guess or refresh everything

### Target State (v1.3+)

```python
@strawberry.type
class MutationResult:
    """Standard mutation result with affected entities."""
    id: UUID
    status: str  # "created", "updated", "deleted"
    object_data: JSONScalar  # Main entity data
    affected_entity_ids: dict[str, list[UUID]] | None = None

async def update_user(
    pk_user: int,
    username: str,
    repository: FraiseQLRepository
) -> MutationResult:
    """Update user and return affected entity IDs."""

    # 1. Update normalized table
    result = await repository.execute(
        """
        UPDATE tb_users
        SET username = $1
        WHERE pk_user = $2
        RETURNING id
        """,
        username, pk_user
    )
    user_id = result[0]['id']

    # 2. Get affected entity IDs
    affected_posts = await repository.execute(
        """
        SELECT id FROM tb_posts
        WHERE pk_user = $1
        """,
        pk_user
    )
    post_ids = [row['id'] for row in affected_posts]

    affected_comments = await repository.execute(
        """
        SELECT id FROM tb_comments
        WHERE pk_user = $1
        """,
        pk_user
    )
    comment_ids = [row['id'] for row in affected_comments]

    # 3. Sync CQRS
    await repository.execute("SELECT fn_sync_tv_users($1)", pk_user)
    if post_ids:
        pk_posts = await repository.execute(
            "SELECT pk_post FROM tb_posts WHERE id = ANY($1)",
            post_ids
        )
        await repository.execute(
            "SELECT fn_sync_tv_posts_batch($1)",
            [row['pk_post'] for row in pk_posts]
        )

    # 4. Get updated user data
    user = await repository.find_one(User, where={"pk_user": pk_user})

    # 5. Return rich result
    return MutationResult(
        id=user_id,
        status="updated",
        object_data=user,
        affected_entity_ids={
            "posts": post_ids,
            "comments": comment_ids
        }
    )
```

### Frontend Usage

```typescript
// Frontend can now intelligently invalidate cache
const result = await updateUser({ pkUser: 123, username: "newname" });

// Invalidate main entity
cache.invalidate('User', result.id);

// Invalidate affected entities
if (result.affected_entity_ids?.posts) {
    result.affected_entity_ids.posts.forEach(id =>
        cache.invalidate('Post', id)
    );
}
```

### Implementation Plan

#### Phase 1: Create MutationResult Type (Day 1)

```python
# src/fraiseql/types/mutation_result.py

import strawberry
from uuid import UUID
from typing import Optional
from fraiseql.types.json_scalar import JSONScalar

@strawberry.type
class MutationResult:
    """Standard mutation result with affected entities.

    Example:
        {
            "id": "550e8400-...",
            "status": "updated",
            "object_data": { "username": "johndoe", ... },
            "affected_entity_ids": {
                "posts": ["uuid1", "uuid2"],
                "comments": ["uuid3"]
            }
        }
    """
    id: UUID = strawberry.field(
        description="ID of the main entity affected"
    )

    status: str = strawberry.field(
        description="Operation status: created, updated, deleted"
    )

    object_data: JSONScalar = strawberry.field(
        description="Main entity data after mutation"
    )

    affected_entity_ids: Optional[dict[str, list[UUID]]] = strawberry.field(
        default=None,
        description="Map of entity type to list of affected IDs"
    )
```

#### Phase 2: Create Helper for Tracking Affected Entities (Day 2)

```python
# src/fraiseql/mutations/affected_tracker.py

class AffectedEntityTracker:
    """Track entities affected by mutations."""

    def __init__(self, repository: FraiseQLRepository):
        self.repo = repository
        self.affected: dict[str, list[UUID]] = {}

    async def track_relations(
        self,
        entity_type: Type,
        pk: int,
        relations: list[str]
    ) -> None:
        """Track IDs of related entities."""
        for relation in relations:
            table_name = self._get_relation_table(entity_type, relation)
            fk_column = self._get_fk_column(entity_type, relation)

            # Get affected IDs
            result = await self.repo.execute(
                f"SELECT id FROM tb_{table_name} WHERE {fk_column} = $1",
                pk
            )

            ids = [row['id'] for row in result]
            if ids:
                self.affected[table_name] = ids

    def get_affected_ids(self) -> dict[str, list[UUID]]:
        """Get all tracked affected IDs."""
        return self.affected
```

#### Phase 3: Update Mutations to Use Rich Returns (Days 3-5)

```python
# Update all mutations to return MutationResult

async def create_post(
    pk_user: int,
    title: str,
    content: str,
    repository: FraiseQLRepository
) -> MutationResult:
    """Create post with rich return."""
    tracker = AffectedEntityTracker(repository)

    # Create post
    result = await repository.execute(
        """
        INSERT INTO tb_posts (pk_user, title, content)
        VALUES ($1, $2, $3)
        RETURNING pk_post, id
        """,
        pk_user, title, content
    )
    pk_post = result[0]['pk_post']
    post_id = result[0]['id']

    # Track affected user (post count changed)
    await tracker.track_relations(User, pk_user, ['posts'])

    # Sync CQRS
    await repository.execute("SELECT fn_sync_tv_posts($1)", pk_post)
    await repository.execute("SELECT fn_sync_tv_users($1)", pk_user)

    # Get post data
    post = await repository.find_one(Post, where={"pk_post": pk_post})

    return MutationResult(
        id=post_id,
        status="created",
        object_data=post,
        affected_entity_ids=tracker.get_affected_ids()
    )
```

### Breaking Changes

**GraphQL Schema Change** (Minor):
- Mutation return types change from `User` to `MutationResult`
- Clients need to access `.object_data` instead of root
- Can provide backward compatibility layer

**Backward Compatibility Option**:
```python
# Option 1: Keep both return styles
@strawberry.mutation
async def update_user_v1(...) -> User:  # Old style
    ...

@strawberry.mutation
async def update_user_v2(...) -> MutationResult:  # New style
    ...

# Option 2: Union type
@strawberry.mutation
async def update_user(...) -> Union[User, MutationResult]:
    ...
```

### Benefits Summary

| Aspect | Before | After | Improvement |
|--------|--------|-------|-------------|
| Client Round-trips | N queries | 1 query | Faster |
| Cache Invalidation | Guess/refresh all | Targeted | Precise |
| Frontend Code | Complex refetch logic | Simple ID-based | Cleaner |
| Network Usage | Multiple queries | Single query | Bandwidth saved |

### Files to Create

1. `src/fraiseql/types/mutation_result.py`
2. `src/fraiseql/mutations/affected_tracker.py`
3. `docs/patterns/rich_mutation_returns.md`
4. `tests/mutations/test_rich_returns.py`

### Files to Modify

1. All mutation functions - Return `MutationResult`
2. GraphQL schema - Update mutation return types

---

## Implementation Priority & Timeline

### Recommended Order

1. **Enhancement 1: Trinity Identifiers** (2-3 weeks)
   - Highest performance impact (10x faster joins)
   - Foundation for other enhancements
   - One-time migration effort

2. **Enhancement 2: Explicit CQRS Sync** (1-2 weeks)
   - Works perfectly with Trinity
   - Simplifies debugging
   - Complements Rust-first architecture

3. **Enhancement 3: Rich Return Types** (1 week)
   - Nice-to-have, not critical
   - Improves frontend DX
   - Can be done incrementally

### Total Timeline: 4-6 weeks

**Week 1-3**: Trinity Identifiers
**Week 4-5**: Explicit CQRS Sync
**Week 6**: Rich Return Types (optional)

---

## Success Metrics

### Performance Metrics
- [ ] Complex queries (5+ joins): 50ms → 5ms (10x improvement)
- [ ] Simple queries: < 1ms
- [ ] Bulk operations: 10x faster with batch sync

### Code Quality Metrics
- [ ] Execution paths: 1 (simplified)
- [ ] Config options: Minimal
- [ ] Test coverage: > 95%
- [ ] Documentation: Complete

### Developer Experience
- [ ] Clear migration path
- [ ] Well-documented patterns
- [ ] Easy to debug (explicit sync)
- [ ] TypeScript-friendly (rich returns)

---

## Compatibility with RUST_FIRST_SIMPLIFICATION

All enhancements are **fully compatible** with Rust-first architecture:

✅ **Trinity Identifiers**: Internal optimization, Rust transformer unaffected
✅ **Explicit CQRS**: Reads from tv_* tables (Rust transforms this)
✅ **Rich Returns**: Python-side only, doesn't affect Rust transformation

**Guiding principle maintained**: "the complexity in PostgreSQL is ok"
- PostgreSQL gets more sophisticated (Trinity, explicit sync)
- Python stays simple (Rust handles transformation)
- Best of both worlds

---

**Last Updated**: 2025-10-16
**Status**: Planning complete - Ready for implementation
