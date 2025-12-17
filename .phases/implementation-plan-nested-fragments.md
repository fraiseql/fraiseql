# Implementation Plan: Nested Field Fragments (Gap #1)

**Feature:** Support fragment spreads in nested field selections
**Effort:** 2-3 hours
**Complexity:** Low
**Risk:** Low
**Status:** Ready for implementation

---

## Executive Summary

Currently, FraiseQL expands fragment spreads **only at the root query level**. This plan adds recursive fragment expansion so fragments work in nested field selections—critical for complex denormalized view queries.

**Example of what will work after implementation:**
```graphql
fragment UserFields on User {
  id
  name
  email
}

query {
  users {
    ...UserFields          # ← Will now work in nested selection
    created_at
    posts {
      ...PostFields        # ← And in deeply nested selections
      comments {
        ...CommentFields   # ← Recursively through all levels
      }
    }
  }
}
```

---

## Part 1: Current State Analysis

### Current Implementation

**File:** `src/fraiseql/core/fragment_resolver.py`

```python
def resolve_all_fields(
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
    typename: str | None = None,
) -> list[FieldNode]:
    """Resolve all fields from a selection set, including fragments."""

    result: list[FieldNode] = []

    def resolve(sel: SelectionNode) -> None:
        if sel.kind == "field":
            field_node = cast("FieldNode", sel)
            result.append(field_node)
            # ❌ MISSING: Doesn't process field_node.selection_set

        elif sel.kind == "fragment_spread":
            frag_spread = cast("FragmentSpreadNode", sel)
            name = frag_spread.name.value
            if name not in fragments:
                msg = f"Fragment '{name}' not found"
                raise ValueError(msg)
            frag = fragments[name]
            for frag_sel in frag.selection_set.selections:
                resolve(frag_sel)

        elif sel.kind == "inline_fragment":
            inline_frag = cast("InlineFragmentNode", sel)
            type_condition = (
                inline_frag.type_condition.name.value if inline_frag.type_condition else None
            )
            if typename is None or type_condition is None or type_condition == typename:
                for frag_sel in inline_frag.selection_set.selections:
                    resolve(frag_sel)

    for sel in selection_set.selections:
        resolve(sel)

    return deduplicate_fields(result)
```

### Problem

When `resolve()` encounters a `FieldNode`, it appends it to results **without processing nested selections**. If that field has a `selection_set` containing fragments, those fragments are never expanded.

### Impact on FraiseQL

With complex denormalized views like:
```python
@fraiseql.type(sql_source="tv_user_with_extended_profile")
class UserWithProfile:
    id: UUID
    name: str
    email: str
    created_at: datetime
    profile: dict  # Nested object with many subfields
    posts: list[dict]  # Nested array with many subfields
```

Developers must repeat fragment definitions for each nested level, defeating fragment reuse.

---

## Part 2: Implementation Strategy

### Architecture

The fix involves **recursive fragment resolution** at the field level:

```
Root Selection Set
├── Direct fields → append to result
├── Fragment spread → expand and append
└── For each field with nested selection_set:
    ├── Recursively resolve nested selection set
    ├── Attach resolved fields to field node
    └── Return field with resolved children
```

### Key Design Decisions

**Decision 1: Mutate field nodes or return new ones?**
- ✅ **Mutate existing FieldNode** (graphql-core allows modification)
- Simpler and maintains object identity
- FieldNode is already mutable during parsing phase

**Decision 2: When to resolve nested fragments?**
- ✅ **During initial fragment resolution** (in `resolve_all_fields`)
- Not during query execution (would be too late for routing)
- Matches existing pattern of resolving at parse time

**Decision 3: How to handle field deduplication with nested fields?**
- ✅ **Deduplicate at each level** (root and nested)
- Prevents duplicate nested selections
- Maintains aliasing support

---

## Part 3: Detailed Implementation Steps

### Step 1: Understand Current Fragment Structure (15 minutes)

**What to review:**
1. How `FieldNode` represents selection_set
2. How `FragmentDefinitionNode` stores selections
3. How deduplication works

**Files to examine:**
```bash
# Review graphql-core structure
python3 -c "
from graphql import FieldNode, SelectionSetNode, FragmentDefinitionNode
import inspect
print('FieldNode attributes:')
print([m for m in dir(FieldNode) if not m.startswith('_')])
"

# Review current tests
grep -n "resolve_all_fields" tests/unit/core/test_fragment_resolver.py | head -20
```

**Acceptance:** Understand how FieldNode.selection_set works

---

### Step 2: Add Recursive Resolution to resolve() (30 minutes)

**File:** `src/fraiseql/core/fragment_resolver.py`

**Current code (lines 40-62):**
```python
def resolve(sel: SelectionNode) -> None:
    if sel.kind == "field":
        field_node = cast("FieldNode", sel)
        result.append(field_node)

    elif sel.kind == "fragment_spread":
        # ... handle spreads
```

**Change to:**
```python
def resolve(sel: SelectionNode) -> None:
    if sel.kind == "field":
        field_node = cast("FieldNode", sel)

        # ✅ NEW: Recursively resolve nested selections
        if field_node.selection_set:
            nested_resolved = resolve_all_fields(
                field_node.selection_set,
                fragments,
                typename=None  # Type info from schema if available
            )
            # Update field node with resolved nested fields
            # Note: FieldNode is immutable in graphql-core 3.x
            # Must reconstruct with new selection_set
            field_node = FieldNode(
                name=field_node.name,
                alias=field_node.alias,
                arguments=field_node.arguments,
                directives=field_node.directives,
                selection_set=SelectionSetNode(
                    selections=tuple(nested_resolved)
                ) if nested_resolved else None,
                loc=field_node.loc,
            )

        result.append(field_node)

    elif sel.kind == "fragment_spread":
        # ... existing code
```

**Test this step:**
```python
def test_field_with_nested_selection_preserved():
    """Field with nested selections is preserved"""
    query = """
    query {
      users {
        id
        posts {
          id
          title
        }
      }
    }
    """
    # Parse and resolve
    # Assert: users field has selection_set with id, posts
    # Assert: posts field has selection_set with id, title
```

**Acceptance:** Field nodes with nested selections are preserved

---

### Step 3: Handle Fragment Spreads in Nested Selections (30 minutes)

**File:** `src/fraiseql/core/fragment_resolver.py`

Now that we recurse into nested selections, fragment spreads within them will be automatically expanded by the recursive call to `resolve_all_fields()`.

**Verify this works:**
```python
def test_nested_fragment_spread_basic():
    """Fragment spread in nested field selection"""
    query = """
    fragment PostFields on Post {
      id
      title
    }

    query {
      users {
        id
        posts {
          ...PostFields  # ← Should be expanded
        }
      }
    }
    """
    # Parse and resolve
    # Assert: users.posts contains [id, title] from fragment
```

**Acceptance:** Fragment spreads in nested selections are expanded

---

### Step 4: Handle Inline Fragments in Nested Selections (15 minutes)

**Current code already handles inline fragments in the resolve loop:**
```python
elif sel.kind == "inline_fragment":
    inline_frag = cast("InlineFragmentNode", sel)
    # ... type condition check
    for frag_sel in inline_frag.selection_set.selections:
        resolve(frag_sel)
```

**Verify it works with nested inline fragments:**
```python
def test_nested_inline_fragment():
    """Inline fragment in nested selection"""
    query = """
    query {
      users {
        id
        ... on AdminUser {
          adminLevel
          permissions
        }
        posts {
          ... on PublishedPost {
            publishedAt
          }
        }
      }
    }
    """
    # Parse and resolve
    # Assert: Inline fragments expanded at all levels
```

**Acceptance:** Inline fragments in nested selections work

---

### Step 5: Test Deduplication at Nested Levels (20 minutes)

**File:** `tests/unit/core/test_fragment_resolver.py`

Add test to verify deduplication works at each nesting level:

```python
def test_deduplicate_nested_repeated_fields():
    """Repeated fields in nested selections are deduplicated"""
    query = """
    fragment PostBase on Post {
      id
      title
    }

    query {
      users {
        id
        posts {
          ...PostBase
          id      # ← Duplicate of id from fragment
          title   # ← Duplicate of title from fragment
          content
        }
      }
    }
    """
    # Parse and resolve
    # Assert: users.posts contains [id, title, content]
    # Assert: No duplicates (dedup by alias/name)
```

**Acceptance:** Deduplication works at all levels

---

### Step 6: Test Aliasing in Nested Fragments (20 minutes)

**File:** `tests/unit/core/test_fragment_resolver.py`

Aliases are critical for denormalized view queries:

```python
def test_nested_fragment_with_alias():
    """Aliases in nested fragment selections"""
    query = """
    fragment UserData on User {
      userId: id
      userName: name
    }

    query {
      users {
        ...UserData
        posts {
          postId: id
          title
        }
      }
    }
    """
    # Parse and resolve
    # Assert: userId alias preserved for id field
    # Assert: postId alias preserved in nested posts
```

**Acceptance:** Aliases work correctly in nested fragments

---

### Step 7: Integration with Multi-Field Queries (30 minutes)

**Files:**
- `src/fraiseql/fastapi/routers.py` (multi-field query handler)
- `tests/integration/fastapi/test_nested_fragments.py` (NEW)

Verify nested fragments work with FraiseQL's multi-field query routing:

```python
async def test_multi_field_query_with_nested_fragments():
    """End-to-end: multi-field query with nested fragments"""

    payload = {
        "query": """
        fragment AllocationData on Allocation {
          id
          startDate
          endDate
          machineId
        }

        query {
          allocations {
            ...AllocationData
            currentStatus {
              ...CurrentStatusData
            }
          }
          machines {
            id
            name
            allocations {
              ...AllocationData
            }
          }
        }
        """,
        "variables": {}
    }

    response = await client.post("/graphql", json=payload)

    # Assert: Response contains all requested fields
    # Assert: Fragments expanded correctly
    # Assert: Nested fragments resolved
```

**Acceptance:** Multi-field queries with nested fragments work end-to-end

---

### Step 8: Benchmark Query Resolution Time (15 minutes)

**File:** `tests/performance/test_fragment_resolution_perf.py` (NEW)

Ensure recursive resolution doesn't cause performance regression:

```python
def test_fragment_resolution_performance():
    """Fragment resolution time doesn't regress"""
    import time

    # Generate deeply nested query with fragments
    # (3-5 levels deep, 20+ fields per level)
    query = generate_deeply_nested_query_with_fragments()

    start = time.perf_counter()
    for _ in range(100):  # 100 iterations
        document = parse(query)
        resolve_all_fields(
            document.definitions[1].selection_set,  # query operation
            fragments,
        )
    elapsed = time.perf_counter() - start

    # Assert: < 100ms for 100 iterations (1ms per query)
    assert elapsed < 0.1, f"Fragment resolution too slow: {elapsed:.2f}s"
```

**Acceptance:** Performance < 5% variance from baseline

---

## Part 4: Complete Code Changes

### Modified: `src/fraiseql/core/fragment_resolver.py`

```python
"""Resolve GraphQL selection sets by expanding fragments and deduplicating fields."""

from typing import cast

from graphql import (
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    InlineFragmentNode,
    SelectionNode,
    SelectionSetNode,
)


def resolve_all_fields(
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
    typename: str | None = None,
) -> list[FieldNode]:
    """Resolve all fields from a selection set, including fragments.

    This function recursively expands both named and inline fragments
    within the given selection set, including in nested field selections.
    It ensures that fields from fragments are included alongside explicitly
    selected fields. When a `typename` is provided, it filters inline
    fragments to only include those matching the type condition, helping
    to accurately reflect the queried GraphQL schema's polymorphic behavior.

    Args:
        selection_set: The selection set node to resolve fields from.
        fragments: A dictionary of named fragment definitions by name.
        typename: Optional GraphQL type name to filter inline fragments.

    Returns:
        A list of unique FieldNode instances, combining explicit fields and
        expanded fragments (including nested selections), with duplicates
        removed based on alias or name.
    """
    result: list[FieldNode] = []

    def resolve(sel: SelectionNode) -> None:
        if sel.kind == "field":
            field_node = cast("FieldNode", sel)

            # ✅ NEW: Recursively resolve nested field selections
            if field_node.selection_set:
                nested_fields = resolve_all_fields(
                    field_node.selection_set,
                    fragments,
                    typename=None,  # Type info from schema if available
                )
                # Reconstruct field node with resolved nested selections
                # (graphql-core 3.x FieldNode is immutable)
                field_node = FieldNode(
                    name=field_node.name,
                    alias=field_node.alias,
                    arguments=field_node.arguments,
                    directives=field_node.directives,
                    selection_set=SelectionSetNode(
                        selections=tuple(nested_fields)
                    ) if nested_fields else None,
                    loc=field_node.loc,
                )

            result.append(field_node)

        elif sel.kind == "fragment_spread":
            frag_spread = cast("FragmentSpreadNode", sel)
            name = frag_spread.name.value
            if name not in fragments:
                msg = f"Fragment '{name}' not found"
                raise ValueError(msg)
            frag = fragments[name]
            # Recursively resolve nested selections within fragment
            for frag_sel in frag.selection_set.selections:
                resolve(frag_sel)

        elif sel.kind == "inline_fragment":
            inline_frag = cast("InlineFragmentNode", sel)
            type_condition = (
                inline_frag.type_condition.name.value if inline_frag.type_condition else None
            )
            if typename is None or type_condition is None or type_condition == typename:
                # Recursively resolve nested selections within inline fragment
                for frag_sel in inline_frag.selection_set.selections:
                    resolve(frag_sel)

    for sel in selection_set.selections:
        resolve(sel)

    return deduplicate_fields(result)


def deduplicate_fields(fields: list[FieldNode]) -> list[FieldNode]:
    """Remove duplicated fields by alias (or name if alias is not present).

    Preserves the first occurrence of each field and maintains order.
    """
    seen: set[str] = set()
    deduped: list[FieldNode] = []

    for field in fields:
        key = field.alias.value if field.alias else field.name.value
        if key not in seen:
            seen.add(key)
            deduped.append(field)

    return deduped
```

**Changes:**
- Lines 31-48: Add recursive resolution of `field_node.selection_set`
- Reconstruct FieldNode with resolved nested selections
- Preserve all metadata (alias, arguments, directives, location info)

---

## Part 5: Test Suite

### New Test File: `tests/unit/core/test_nested_fragments.py`

```python
"""Tests for nested fragment resolution."""

import pytest
from graphql import build_schema, parse

from fraiseql.core.fragment_resolver import resolve_all_fields


# Test fixtures
@pytest.fixture
def sample_schema():
    return build_schema("""
    type User {
        id: ID!
        name: String!
        email: String!
        createdAt: String!
        profile: Profile
        posts: [Post!]!
    }

    type Profile {
        bio: String
        avatar: String
        location: String
    }

    type Post {
        id: ID!
        title: String!
        content: String!
        publishedAt: String
        comments: [Comment!]!
    }

    type Comment {
        id: ID!
        text: String!
        author: String!
        createdAt: String!
    }

    type Query {
        users: [User!]!
    }
    """)


class TestNestedFragmentBasics:
    """Basic nested fragment functionality"""

    def test_nested_fragment_spread_in_field(self, sample_schema):
        """Fragment spread in nested field selection"""
        query = """
        fragment PostFields on Post {
            id
            title
        }

        query {
            users {
                id
                posts {
                    ...PostFields
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[1]

        resolved = resolve_all_fields(query_op.selection_set, fragments)

        # Assert: users field present
        users_field = next(f for f in resolved if f.name.value == "users")
        assert users_field is not None

        # Assert: posts field has selection_set
        assert users_field.selection_set is not None
        posts_fields = resolve_all_fields(
            users_field.selection_set, fragments
        )

        # Assert: posts contains id and title (from fragment)
        post_field_names = {f.name.value for f in posts_fields}
        assert "id" in post_field_names
        assert "title" in post_field_names

    def test_deeply_nested_fragments(self, sample_schema):
        """Multiple levels of nested fragments (3+ levels)"""
        query = """
        fragment CommentFields on Comment {
            id
            text
        }

        fragment PostWithComments on Post {
            id
            title
            comments {
                ...CommentFields
            }
        }

        query {
            users {
                id
                posts {
                    ...PostWithComments
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[2]

        resolved = resolve_all_fields(query_op.selection_set, fragments)

        # Navigate: users -> posts -> comments
        users_field = next(f for f in resolved if f.name.value == "users")
        posts_fields = resolve_all_fields(users_field.selection_set, fragments)
        posts_field = next(f for f in posts_fields if f.name.value == "posts")
        comments_fields = resolve_all_fields(posts_field.selection_set, fragments)

        # Assert: comments contains id and text (from fragment)
        comment_field_names = {f.name.value for f in comments_fields}
        assert "id" in comment_field_names
        assert "text" in comment_field_names

    def test_nested_fragment_with_alias(self, sample_schema):
        """Fragment in nested selection with alias"""
        query = """
        fragment UserBaseData on User {
            userId: id
            userName: name
        }

        query {
            users {
                ...UserBaseData
                recentPosts: posts {
                    id
                    title
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[1]

        resolved = resolve_all_fields(query_op.selection_set, fragments)

        # Assert: aliases preserved in expanded fragments
        field_keys = {
            f.alias.value if f.alias else f.name.value
            for f in resolved
        }
        assert "userId" in field_keys
        assert "userName" in field_keys
        assert "recentPosts" in field_keys

    def test_mixed_fragments_and_inline(self, sample_schema):
        """Mix of spread and inline fragments in nested selections"""
        query = """
        fragment PostBase on Post {
            id
            title
        }

        query {
            users {
                id
                posts {
                    ...PostBase
                    ... on Post {
                        content
                    }
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[1]

        resolved = resolve_all_fields(query_op.selection_set, fragments)
        users_field = next(f for f in resolved if f.name.value == "users")
        posts_fields = resolve_all_fields(users_field.selection_set, fragments)

        # Assert: spread fragment fields + inline fragment fields
        post_field_names = {f.name.value for f in posts_fields}
        assert "id" in post_field_names
        assert "title" in post_field_names
        assert "content" in post_field_names


class TestNestedFragmentDeduplication:
    """Fragment deduplication at nested levels"""

    def test_deduplicate_nested_repeated_fields(self, sample_schema):
        """Repeated fields in nested selections are deduplicated"""
        query = """
        fragment PostBase on Post {
            id
            title
        }

        query {
            users {
                id
                posts {
                    ...PostBase
                    id
                    title
                    content
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[1]

        resolved = resolve_all_fields(query_op.selection_set, fragments)
        users_field = next(f for f in resolved if f.name.value == "users")
        posts_fields = resolve_all_fields(users_field.selection_set, fragments)

        # Assert: no duplicates (first occurrence preserved)
        post_field_names = [f.name.value for f in posts_fields]
        assert post_field_names.count("id") == 1
        assert post_field_names.count("title") == 1
        assert "content" in post_field_names


class TestNestedFragmentEdgeCases:
    """Edge cases and error conditions"""

    def test_empty_nested_selection(self, sample_schema):
        """Field with empty nested selection is handled"""
        query = """
        query {
            users {
                id
                name
            }
        }
        """
        document = parse(query)
        query_op = document.definitions[0]

        # Should not raise
        resolved = resolve_all_fields(query_op.selection_set, {})
        assert len(resolved) == 1
        assert resolved[0].name.value == "users"

    def test_fragment_not_found_in_nested(self, sample_schema):
        """Missing fragment in nested selection raises error"""
        query = """
        query {
            users {
                id
                posts {
                    ...NonExistentFragment
                }
            }
        }
        """
        document = parse(query)
        query_op = document.definitions[0]

        with pytest.raises(ValueError, match="Fragment 'NonExistentFragment' not found"):
            resolve_all_fields(query_op.selection_set, {})

    def test_multiple_nested_levels_all_expanded(self, sample_schema):
        """All fragment levels expanded correctly (4+ levels)"""
        query = """
        fragment BaseComment on Comment {
            id
            text
        }

        fragment PostComments on Post {
            id
            comments {
                ...BaseComment
            }
        }

        query {
            users {
                posts {
                    ...PostComments
                }
            }
        }
        """
        document = parse(query)
        fragments = {
            frag.name.value: frag
            for frag in document.definitions
            if hasattr(frag, 'name') and hasattr(frag, 'selection_set')
        }
        query_op = document.definitions[2]

        resolved = resolve_all_fields(query_op.selection_set, fragments)

        # Navigate chain: users -> posts -> comments
        users_field = next(f for f in resolved if f.name.value == "users")
        posts_fields = resolve_all_fields(users_field.selection_set, fragments)
        posts_field = next(f for f in posts_fields if f.name.value == "posts")
        # posts_field should have id and comments (from PostComments fragment)
        post_field_names = {f.name.value for f in resolve_all_fields(posts_field.selection_set, fragments)}
        assert "id" in post_field_names
        assert "comments" in post_field_names
```

### New Integration Test File: `tests/integration/fastapi/test_nested_fragments.py`

```python
"""Integration tests for nested fragments with FraiseQL endpoints."""

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
class TestNestedFragmentsIntegration:
    """End-to-end tests with FraiseQL endpoint"""

    async def test_multi_field_query_with_nested_fragments(self, client: AsyncClient):
        """Multi-field query with nested fragments works end-to-end"""
        payload = {
            "query": """
            fragment BaseAllocationData on Allocation {
                id
                startDate
                endDate
            }

            query {
                allocations {
                    ...BaseAllocationData
                    currentStatus {
                        statusCode
                    }
                }
            }
            """,
            "variables": {}
        }

        response = await client.post("/graphql", json=payload)
        assert response.status_code == 200

        data = response.json()
        assert "data" in data
        assert "allocations" in data["data"]

    async def test_nested_fragments_with_variables(self, client: AsyncClient):
        """Nested fragments work with query variables"""
        payload = {
            "query": """
            fragment PostData on Post {
                id
                title
                content
            }

            query GetUserPosts($userId: ID!) {
                user(id: $userId) {
                    id
                    name
                    posts {
                        ...PostData
                    }
                }
            }
            """,
            "variables": {
                "userId": "test-user-id"
            }
        }

        response = await client.post("/graphql", json=payload)
        # Should process without fragment resolution errors
        assert response.status_code == 200
        data = response.json()
        # May have GraphQL errors for missing data, but not fragment errors
        assert "Fragment 'PostData' not found" not in str(data.get("errors", []))

    async def test_deeply_nested_fragments_across_views(self, client: AsyncClient):
        """Deeply nested fragments across denormalized views"""
        payload = {
            "query": """
            fragment MachineInfo on Machine {
                id
                name
                location {
                    building
                    floor
                }
            }

            fragment AllocationDetails on Allocation {
                id
                startDate
                machine {
                    ...MachineInfo
                }
            }

            query {
                allocations {
                    ...AllocationDetails
                }
            }
            """,
            "variables": {}
        }

        response = await client.post("/graphql", json=payload)
        assert response.status_code == 200
        data = response.json()
        # Verify no fragment resolution errors
        if "errors" in data:
            for error in data["errors"]:
                assert "Fragment" not in error.get("message", "")
```

---

## Part 6: Migration Guide

### Breaking Changes
**None.** This is purely additive functionality.

### Migration
**No migration needed.** Existing queries continue to work exactly as before.

### For Users
Starting with the next release:
```graphql
# OLD (still works): Repeated fragment definitions
fragment UserFields { id name email }
fragment PostFields { id title }

query {
  users { ...UserFields posts { ...PostFields } }
  admin_users { ...UserFields posts { ...PostFields } }
}

# NEW: Reuse fragments in nested selections
query {
  users {
    ...UserFields
    posts { ...PostFields }  # ← Nested fragment now works!
  }
  admin_users {
    ...UserFields
    posts { ...PostFields }
  }
}
```

---

## Part 7: Success Criteria

### Code Quality
- [ ] All unit tests pass (15+ new tests)
- [ ] All integration tests pass
- [ ] No regressions in existing fragment tests
- [ ] Code coverage > 95% for fragment_resolver.py
- [ ] Passes linting (ruff, black)

### Performance
- [ ] Fragment resolution time < 5% variance from baseline
- [ ] Deeply nested queries (5+ levels) resolve in < 100ms
- [ ] Memory usage stable (no memory leaks)

### Functionality
- [ ] Nested fragment spreads expanded correctly
- [ ] Inline fragments in nested selections work
- [ ] Aliases preserved through all nesting levels
- [ ] Deduplication works at every nesting level
- [ ] Complex denormalized view queries work

### Documentation
- [ ] Docstring updated with examples
- [ ] Implementation notes added to fragment_resolver.py
- [ ] No migration guide needed (backward compatible)

---

## Part 8: Dependencies & Prerequisites

### Code Dependencies
- `graphql-core >= 3.2` (already required by FraiseQL)
- No new external dependencies

### Files Modified
1. `src/fraiseql/core/fragment_resolver.py` (main change)

### Files Added
1. `tests/unit/core/test_nested_fragments.py` (new unit tests)
2. `tests/integration/fastapi/test_nested_fragments.py` (new integration tests)

### Testing Infrastructure
- Existing pytest fixtures
- No new test infrastructure needed

---

## Part 9: Rollout Plan

### Phase 1: Development (Day 1)
- [ ] Implement recursive resolution in fragment_resolver.py
- [ ] Write unit tests
- [ ] Run test suite, fix issues

### Phase 2: Integration (Day 2)
- [ ] Write integration tests
- [ ] Test with real FraiseQL endpoints
- [ ] Benchmark performance

### Phase 3: Validation (Day 3)
- [ ] Run full test suite (6000+ tests)
- [ ] Verify no regressions
- [ ] Code review
- [ ] Merge to dev branch

---

## Part 10: Post-Implementation Verification

### Manual Testing Checklist
```python
# Test 1: Simple nested fragment
query {
  users {
    ...UserFields          # ← Works?
    posts {
      ...PostFields        # ← Works?
    }
  }
}

# Test 2: Multiple nesting levels
query {
  users {
    posts {
      comments {
        ...CommentFields   # ← Works at 3+ levels?
      }
    }
  }
}

# Test 3: Denormalized view with many fields
@fraiseql.type(sql_source="tv_user_with_extended_data")
class UserExtended:
    id: UUID
    name: str
    # ... 30+ more fields
    profile: dict
    posts: list[dict]
    # Can now use fragments effectively
```

### Regression Testing
```bash
# Run full test suite
pytest tests/ -v

# Run only fragment tests
pytest tests/ -k fragment -v

# Benchmark comparison
pytest tests/performance/test_fragment_resolution_perf.py --benchmark-compare
```

---

## Conclusion

This implementation adds recursive fragment resolution to FraiseQL, enabling fragment reuse across nested field selections. The change is **low-risk, backward-compatible, and provides significant value** for complex denormalized view queries.

**Effort estimate: 2-3 hours**
**Complexity: Low**
**Risk: Low**
**Value: High**

Status: ✅ Ready for implementation
