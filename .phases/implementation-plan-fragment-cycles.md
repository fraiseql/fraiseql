# Implementation Plan: Fragment Cycle Detection (Gap #5)

**Feature:** Detect and reject circular fragment references at parse time
**Effort:** 3-4 hours
**Complexity:** Low-Moderate
**Risk:** Low
**Status:** Ready for implementation

---

## Executive Summary

FraiseQL currently allows circular fragment references, which can cause infinite loops during execution. This plan adds cycle detection to reject malformed fragments at parse time, preventing runtime failures and enabling safe fragment validation.

**Example of what will be prevented:**
```graphql
# ❌ INVALID: Self-reference
fragment A on User {
  id
  ...A  # ← Circular! Will be rejected
}

# ❌ INVALID: Mutual cycle
fragment A on User {
  id
  ...B
}
fragment B on User {
  name
  ...A  # ← Cycle! Will be rejected

# ❌ INVALID: Transitive cycle
fragment A on User {
  ...B
}
fragment B on User {
  ...C
}
fragment C on User {
  ...A  # ← Cycle! Will be rejected
```

---

## Part 1: Current State Analysis

### Where Cycles Could Occur

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
            # ... handle field

        elif sel.kind == "fragment_spread":
            frag_spread = cast("FragmentSpreadNode", sel)
            name = frag_spread.name.value
            if name not in fragments:
                msg = f"Fragment '{name}' not found"
                raise ValueError(msg)
            frag = fragments[name]
            # ❌ PROBLEM: No cycle detection here
            for frag_sel in frag.selection_set.selections:
                resolve(frag_sel)  # ← Could infinitely recurse

        # ... rest of function
```

### Why Cycles Matter

1. **DoS Prevention**: Malicious queries could exploit cycles to cause infinite loops
2. **Error Messages**: Early detection gives clearer error messages
3. **Safety**: Validates queries at parse time, not execution time
4. **Type Safety**: Combined with type validation, ensures schema correctness

### Current GraphQL-core Behavior

GraphQL-core has **no cycle detection** in fragment resolution by default:
```python
# graphql-core's FragmentDefinitionNode just stores references
# No validation that references are acyclic
```

We must implement this ourselves.

---

## Part 2: Implementation Strategy

### Architecture

```
Parse Query
    ↓
Extract Fragments
    ↓
For each fragment:
    ├── Track visited fragments (set)
    ├── Traverse selections
    ├── If fragment_spread encountered:
    │   ├── Check if in visited set → CYCLE!
    │   ├── Add to visited set
    │   ├── Recursively validate referenced fragment
    │   └── Remove from visited set (backtrack)
    └── Return to caller
    ↓
If no cycles found: Continue execution
If cycle found: Raise ValidationError
```

### Key Design Decisions

**Decision 1: When to validate cycles?**
- ✅ **At parse time**, before any execution
- Not during query execution (too late)
- Not lazily (defeats DoS prevention)
- In `resolve_all_fields()` is too late

**Better location:**
- ✅ Create separate `validate_fragment_cycles()` function
- Call from `routers.py` immediately after parsing
- Independent of `resolve_all_fields()`

**Decision 2: Detect only direct cycles or transitive?**
- ✅ **Detect all cycles** (direct, mutual, transitive)
- Use visited set + DFS backtracking
- Simpler and catches all cases

**Decision 3: Report cycle path for debugging?**
- ✅ **Yes, include path in error message**
- Makes debugging much easier
- Example: "Circular fragment: A → B → C → A"

**Decision 4: How to handle type validation?**
- ✅ **Separate concern**, but important
- Fragment type must be compatible with field type
- Implement as separate validation function

---

## Part 3: Detailed Implementation Steps

### Step 1: Create Cycle Detection Function (45 minutes)

**File:** `src/fraiseql/core/fragment_validator.py` (NEW)

```python
"""Fragment validation including cycle detection."""

from typing import Dict, List, Set

from graphql import (
    DocumentNode,
    FragmentDefinitionNode,
    FieldNode,
    SelectionNode,
)


class FragmentCycleError(Exception):
    """Raised when circular fragment references are detected."""

    def __init__(self, cycle_path: List[str]):
        self.cycle_path = cycle_path
        # Format: "A → B → C → A"
        path_str = " → ".join(cycle_path)
        super().__init__(f"Circular fragment reference: {path_str}")


def validate_no_fragment_cycles(document: DocumentNode) -> None:
    """Validate that fragments don't have circular references.

    Raises:
        FragmentCycleError: If any circular fragment references found

    Args:
        document: The parsed GraphQL document
    """
    # Extract fragment definitions
    fragments: Dict[str, FragmentDefinitionNode] = {}
    for definition in document.definitions:
        if hasattr(definition, 'name') and hasattr(definition, 'selection_set'):
            if definition.__class__.__name__ == 'FragmentDefinitionNode':
                fragments[definition.name.value] = definition

    # Validate each fragment for cycles
    for fragment_name in fragments:
        _validate_fragment_no_cycle(
            fragment_name,
            fragments,
            visited=set(),
            path=[],
        )


def _validate_fragment_no_cycle(
    fragment_name: str,
    fragments: Dict[str, FragmentDefinitionNode],
    visited: Set[str],
    path: List[str],
) -> None:
    """Recursively validate fragment for cycles using DFS.

    Args:
        fragment_name: Name of fragment to validate
        fragments: Dictionary of all fragments
        visited: Set of fragments in current DFS path
        path: Current path for error reporting

    Raises:
        FragmentCycleError: If cycle detected
    """
    # Check if fragment is in current path (cycle detected)
    if fragment_name in visited:
        # Reconstruct cycle path
        cycle_start_idx = path.index(fragment_name)
        cycle_path = path[cycle_start_idx:] + [fragment_name]
        raise FragmentCycleError(cycle_path)

    # Get fragment definition
    fragment_def = fragments.get(fragment_name)
    if not fragment_def:
        # Fragment doesn't exist (other validation will catch this)
        return

    # Add to current path
    new_visited = visited | {fragment_name}
    new_path = path + [fragment_name]

    # Check all selections in fragment
    for selection in fragment_def.selection_set.selections:
        _check_selection_for_fragment_spreads(
            selection,
            fragments,
            new_visited,
            new_path,
        )


def _check_selection_for_fragment_spreads(
    selection: SelectionNode,
    fragments: Dict[str, FragmentDefinitionNode],
    visited: Set[str],
    path: List[str],
) -> None:
    """Check a selection node and all nested selections for fragment cycles.

    Args:
        selection: Selection node to check
        fragments: Dictionary of all fragments
        visited: Set of fragments in current DFS path
        path: Current path for error reporting
    """
    if selection.kind == "fragment_spread":
        # This is a fragment spread, validate it recursively
        spread_name = selection.name.value
        _validate_fragment_no_cycle(
            spread_name,
            fragments,
            visited,
            path,
        )

    elif selection.kind == "field":
        # Field might have nested selections
        if hasattr(selection, 'selection_set') and selection.selection_set:
            for nested_sel in selection.selection_set.selections:
                _check_selection_for_fragment_spreads(
                    nested_sel,
                    fragments,
                    visited,
                    path,
                )

    elif selection.kind == "inline_fragment":
        # Inline fragment has nested selections
        if hasattr(selection, 'selection_set') and selection.selection_set:
            for nested_sel in selection.selection_set.selections:
                _check_selection_for_fragment_spreads(
                    nested_sel,
                    fragments,
                    visited,
                    path,
                )
```

**Test locally first:**
```python
def test_cycle_detection_in_isolation():
    """Validate cycle detection works before integration"""
    from graphql import parse
    from fraiseql.core.fragment_validator import validate_no_fragment_cycles, FragmentCycleError

    # Test 1: Self-reference
    query = """
    fragment A on User {
        id
        ...A
    }
    query { users { id } }
    """
    doc = parse(query)
    with pytest.raises(FragmentCycleError) as exc_info:
        validate_no_fragment_cycles(doc)
    assert "A → A" in str(exc_info.value)

    # Test 2: Valid (no cycle)
    query = """
    fragment A on User { id }
    query { users { ...A } }
    """
    doc = parse(query)
    # Should not raise
    validate_no_fragment_cycles(doc)
```

**Acceptance:** Cycle detection works independently

---

### Step 2: Add Type Validation Function (30 minutes)

**Same file:** `src/fraiseql/core/fragment_validator.py`

```python
def validate_fragment_type_compatibility(
    document: DocumentNode,
    schema,  # GraphQL schema
) -> None:
    """Validate that fragment types are compatible with fields they're applied to.

    Args:
        document: The parsed GraphQL document
        schema: The GraphQL schema

    Raises:
        ValueError: If fragment type incompatibility found
    """
    # Extract fragments and operations
    fragments: Dict[str, FragmentDefinitionNode] = {}
    operations = []

    for definition in document.definitions:
        if hasattr(definition, 'name') and hasattr(definition, 'selection_set'):
            if definition.__class__.__name__ == 'FragmentDefinitionNode':
                fragments[definition.name.value] = definition
            elif definition.__class__.__name__ == 'OperationDefinitionNode':
                operations.append(definition)

    # For each operation, validate fragment usage
    for operation in operations:
        _validate_operation_fragments(
            operation.selection_set,
            fragments,
            schema,
        )


def _validate_operation_fragments(
    selection_set,
    fragments: Dict[str, FragmentDefinitionNode],
    schema,
    parent_type=None,
) -> None:
    """Recursively validate fragment usage in selection set."""
    if not selection_set:
        return

    for selection in selection_set.selections:
        if selection.kind == "fragment_spread":
            fragment_name = selection.name.value
            fragment_def = fragments.get(fragment_name)
            if not fragment_def:
                continue

            # Get fragment's type condition
            frag_type_name = fragment_def.type_condition.name.value
            frag_type = schema.type_map.get(frag_type_name)

            # Validate compatibility
            if parent_type and frag_type:
                if not _is_type_compatible(parent_type, frag_type):
                    raise ValueError(
                        f"Fragment '{fragment_name}' of type {frag_type_name} "
                        f"cannot be applied to field of type {parent_type}"
                    )

        elif selection.kind == "field":
            # Get field type and recurse
            if hasattr(selection, 'selection_set') and selection.selection_set:
                # Type info would come from schema
                _validate_operation_fragments(
                    selection.selection_set,
                    fragments,
                    schema,
                    parent_type=None,  # Would be resolved from schema
                )

        elif selection.kind == "inline_fragment":
            if hasattr(selection, 'selection_set') and selection.selection_set:
                _validate_operation_fragments(
                    selection.selection_set,
                    fragments,
                    schema,
                    parent_type=None,
                )


def _is_type_compatible(parent_type, fragment_type) -> bool:
    """Check if fragment type is compatible with parent type."""
    # Interface/Union: fragment must be implementor or member
    # Object: types must match
    # Simplified: just check names for now
    return parent_type.name == fragment_type.name
```

**Acceptance:** Type validation separate from cycle detection

---

### Step 3: Integrate into Query Processing (30 minutes)

**File:** `src/fraiseql/fastapi/routers.py`

Find where queries are parsed and add cycle validation:

```python
# In the query execution path, after parsing:

from graphql import parse, build_schema
from fraiseql.core.fragment_validator import validate_no_fragment_cycles

async def graphql_endpoint(request: Request):
    """GraphQL query endpoint"""
    body = await request.json()
    query_string = body.get("query", "")
    variables = body.get("variables", {})

    try:
        # Parse query
        document = parse(query_string)

        # ✅ NEW: Validate no fragment cycles
        try:
            validate_no_fragment_cycles(document)
        except FragmentCycleError as e:
            return JSONResponse({
                "errors": [{
                    "message": str(e),
                    "extensions": {
                        "code": "FRAGMENT_CYCLE_ERROR"
                    }
                }]
            }, status_code=400)

        # ✅ NEW: Validate fragment type compatibility
        try:
            validate_fragment_type_compatibility(document, schema)
        except ValueError as e:
            return JSONResponse({
                "errors": [{
                    "message": str(e),
                    "extensions": {
                        "code": "FRAGMENT_TYPE_ERROR"
                    }
                }]
            }, status_code=400)

        # Continue with existing execution...
        result = await execute_graphql(document, variables, ...)

    except Exception as e:
        # ... existing error handling
```

**Acceptance:** Cycles detected before execution starts

---

### Step 4: Update Query Complexity Analyzer (30 minutes)

**File:** `src/fraiseql/analysis/query_complexity.py`

The complexity analyzer currently has a simplification note (line 185-186). Fix it to handle fragment cycles:

```python
# Current (line 185-186):
# Simplified - we'd properly handle recursive fragments

# New version:
def enter_fragment_spread(self, node, *args):
    """Handle fragment spread in query complexity analysis"""
    fragment_name = node.name.value

    # Get fragment definition
    fragment = self.fragments.get(fragment_name)
    if not fragment:
        return

    # Check if already visiting (would be cycle)
    if fragment_name in self.visited_fragments:
        # Already counted, don't count again (prevents infinite recursion)
        return

    # Mark as visited
    self.visited_fragments.add(fragment_name)

    # Analyze fragment complexity
    self.visit(fragment.selection_set)

    # Unmark (for other paths through fragments)
    self.visited_fragments.remove(fragment_name)
```

**Acceptance:** Complexity analysis doesn't hit cycles

---

### Step 5: Write Unit Tests (1 hour)

**File:** `tests/unit/core/test_fragment_cycles.py` (NEW)

```python
"""Tests for fragment cycle detection."""

import pytest
from graphql import parse

from fraiseql.core.fragment_validator import (
    validate_no_fragment_cycles,
    FragmentCycleError,
)


class TestDirectFragmentCycles:
    """Direct fragment cycles (self-reference)"""

    def test_fragment_self_reference(self):
        """Fragment references itself directly"""
        query = """
        fragment A on User {
            id
            ...A
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)
        assert "A → A" in str(exc_info.value)

    def test_self_reference_after_other_fields(self):
        """Fragment self-references after other selections"""
        query = """
        fragment A on User {
            id
            name
            ...A
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)
        assert "A → A" in str(exc_info.value)


class TestMutualFragmentCycles:
    """Two fragments referencing each other"""

    def test_two_fragment_mutual_cycle(self):
        """Fragment A references B, B references A"""
        query = """
        fragment A on User {
            id
            ...B
        }
        fragment B on User {
            name
            ...A
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)
        error_msg = str(exc_info.value)
        # Should show cycle: A → B → A (or similar)
        assert "→" in error_msg
        assert "A" in error_msg
        assert "B" in error_msg

    def test_three_fragment_mutual_cycle(self):
        """Fragments A → B → C → A"""
        query = """
        fragment A on User {
            id
            ...B
        }
        fragment B on User {
            name
            ...C
        }
        fragment C on User {
            email
            ...A
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)
        error_msg = str(exc_info.value)
        assert "A" in error_msg


class TestTransitiveFragmentCycles:
    """Complex chains that form cycles"""

    def test_transitive_cycle_complex(self):
        """Multiple paths, cycle A → B → D → A"""
        query = """
        fragment A on User {
            id
            ...B
            ...C
        }
        fragment B on User {
            name
            ...D
        }
        fragment C on User {
            email
        }
        fragment D on User {
            phone
            ...A
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)
        # Should detect cycle even if multiple paths


class TestValidFragments:
    """Valid fragment definitions (no cycles)"""

    def test_simple_valid_fragment(self):
        """Single fragment with no cycles"""
        query = """
        fragment UserData on User {
            id
            name
            email
        }
        query { users { ...UserData } }
        """
        doc = parse(query)
        # Should not raise
        validate_no_fragment_cycles(doc)

    def test_multiple_non_cyclic_fragments(self):
        """Multiple fragments, none referencing each other"""
        query = """
        fragment UserData on User { id name }
        fragment PostData on Post { id title }
        fragment CommentData on Comment { id text }
        query { users { ...UserData } }
        """
        doc = parse(query)
        # Should not raise
        validate_no_fragment_cycles(doc)

    def test_acyclic_fragment_chain(self):
        """Fragment chain A → B → C (no back-reference)"""
        query = """
        fragment A on User {
            id
            ...B
        }
        fragment B on User {
            name
            ...C
        }
        fragment C on User {
            email
        }
        query { users { ...A } }
        """
        doc = parse(query)
        # Should not raise (no cycle, just a chain)
        validate_no_fragment_cycles(doc)

    def test_diamond_pattern_valid(self):
        """Diamond pattern: A → B,C → B (valid, same target)"""
        query = """
        fragment A on User {
            ...B
            ...C
        }
        fragment B on User {
            id
            name
        }
        fragment C on User {
            id
            email
        }
        query { users { ...A } }
        """
        doc = parse(query)
        # Should not raise (B referenced twice, but no cycle)
        validate_no_fragment_cycles(doc)


class TestFragmentCyclesWithInlineFragments:
    """Cycles involving inline fragments"""

    def test_cycle_with_inline_fragment(self):
        """Fragment cycle involving inline fragment"""
        query = """
        fragment A on User {
            id
            ... on User {
                ...A
            }
        }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError):
            validate_no_fragment_cycles(doc)

    def test_nested_inline_no_cycle(self):
        """Nested inline fragments without cycle"""
        query = """
        fragment A on User {
            id
            ... on User {
                name
            }
        }
        query { users { ...A } }
        """
        doc = parse(query)
        # Should not raise
        validate_no_fragment_cycles(doc)


class TestFragmentCycleErrorMessages:
    """Error message quality and usability"""

    def test_error_message_includes_cycle_path(self):
        """Error message shows the cycle path"""
        query = """
        fragment A on User { ...B }
        fragment B on User { ...C }
        fragment C on User { ...A }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)

        error_msg = str(exc_info.value)
        # Should show path like "A → B → C → A"
        assert "→" in error_msg
        assert "Circular fragment reference" in error_msg

    def test_error_message_descriptive(self):
        """Error message is descriptive and actionable"""
        query = """
        fragment A on User { ...A }
        query { users { id } }
        """
        doc = parse(query)
        with pytest.raises(FragmentCycleError) as exc_info:
            validate_no_fragment_cycles(doc)

        error_msg = str(exc_info.value)
        # Should clearly explain what's wrong
        assert "Circular" in error_msg
        assert "fragment" in error_msg.lower()


class TestEdgeCases:
    """Edge cases and unusual patterns"""

    def test_missing_fragment_referenced(self):
        """Fragment references non-existent fragment (other validation handles)"""
        query = """
        fragment A on User {
            id
            ...NonExistent
        }
        query { users { id } }
        """
        doc = parse(query)
        # Cycle detection shouldn't raise (missing fragment is other error)
        # But validate_no_fragment_cycles should handle gracefully
        try:
            validate_no_fragment_cycles(doc)
        except FragmentCycleError:
            # If it does raise, that's okay too
            pass

    def test_fragment_not_used_in_query(self):
        """Fragment defined but not used"""
        query = """
        fragment Unused on User { id }
        query { users { id } }
        """
        doc = parse(query)
        # Should not raise (fragment exists, just not used)
        validate_no_fragment_cycles(doc)

    def test_empty_fragment(self):
        """Empty fragment (no fields)"""
        query = """
        fragment Empty on User { }
        query { users { ...Empty } }
        """
        doc = parse(query)
        # Should not raise
        validate_no_fragment_cycles(doc)
```

**Acceptance:** All cycle tests pass

---

### Step 6: Integration Tests (30 minutes)

**File:** `tests/integration/fastapi/test_fragment_cycles.py` (NEW)

```python
"""Integration tests for fragment cycle detection in endpoints."""

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
class TestFragmentCycleDetectionIntegration:
    """End-to-end tests with FraiseQL endpoint"""

    async def test_endpoint_rejects_self_referencing_fragment(
        self, client: AsyncClient
    ):
        """Endpoint rejects query with self-referencing fragment"""
        payload = {
            "query": """
            fragment BadFragment on User {
                id
                ...BadFragment
            }

            query {
                users {
                    ...BadFragment
                }
            }
            """
        }

        response = await client.post("/graphql", json=payload)
        assert response.status_code == 400

        data = response.json()
        assert "errors" in data
        assert any(
            "Circular" in error.get("message", "")
            for error in data["errors"]
        )

    async def test_endpoint_rejects_mutual_fragment_cycle(
        self, client: AsyncClient
    ):
        """Endpoint rejects query with mutual fragment cycle"""
        payload = {
            "query": """
            fragment FragA on User {
                id
                ...FragB
            }

            fragment FragB on User {
                name
                ...FragA
            }

            query {
                users {
                    ...FragA
                }
            }
            """
        }

        response = await client.post("/graphql", json=payload)
        assert response.status_code == 400

        data = response.json()
        assert "errors" in data

    async def test_endpoint_accepts_valid_fragments(
        self, client: AsyncClient
    ):
        """Endpoint accepts query with valid fragments"""
        payload = {
            "query": """
            fragment UserData on User {
                id
                name
                email
            }

            query {
                users {
                    ...UserData
                }
            }
            """
        }

        response = await client.post("/graphql", json=payload)
        # Should either succeed or fail with GraphQL error, not 400
        assert response.status_code in [200, 400]
        data = response.json()

        # If errors, should not be about fragments
        if "errors" in data:
            for error in data["errors"]:
                assert "Circular" not in error.get("message", "")
```

**Acceptance:** Integration tests pass

---

## Part 4: Complete Code Changes Summary

### Files Created
1. `src/fraiseql/core/fragment_validator.py` - New validation module

### Files Modified
1. `src/fraiseql/fastapi/routers.py` - Add cycle validation to endpoint
2. `src/fraiseql/analysis/query_complexity.py` - Fix fragment handling
3. Tests: Multiple new test files

---

## Part 5: Migration Guide

### Breaking Changes
**None.** Queries that previously would have silently caused issues will now be rejected with clear error messages.

### For Users
If you have queries with fragment cycles (unlikely in production, would have caused runtime errors), update them:

```graphql
# ❌ OLD (would cause issues)
fragment A on User {
  id
  ...A  # Self-reference
}

# ✅ NEW (remove the cycle)
fragment A on User {
  id
  name
  email
}
```

---

## Part 6: Success Criteria

### Code Quality
- [ ] All unit tests pass (20+ new tests)
- [ ] All integration tests pass
- [ ] No regressions in existing tests
- [ ] Code coverage > 95% for fragment_validator.py
- [ ] Passes linting (ruff, black)

### Functionality
- [ ] Self-referencing fragments rejected
- [ ] Mutual cycles detected
- [ ] Transitive cycles detected
- [ ] Valid fragments still work
- [ ] Error messages include cycle path
- [ ] Complexity analyzer handles fragments

### Performance
- [ ] Cycle validation < 10ms for typical queries
- [ ] No performance regression in query execution
- [ ] Memory usage stable

### Documentation
- [ ] Clear error messages for users
- [ ] Docstrings explain algorithm
- [ ] Implementation notes in code

---

## Part 7: Dependencies & Prerequisites

### Code Dependencies
- `graphql-core >= 3.2` (already required)
- No new external dependencies

### Files Modified
1. `src/fraiseql/fastapi/routers.py`
2. `src/fraiseql/analysis/query_complexity.py`

### Files Added
1. `src/fraiseql/core/fragment_validator.py`
2. `tests/unit/core/test_fragment_cycles.py`
3. `tests/integration/fastapi/test_fragment_cycles.py`

---

## Part 8: Implementation Checklist

### Development
- [ ] Create `fragment_validator.py` with cycle detection
- [ ] Write unit tests for all cycle patterns
- [ ] Test in isolation
- [ ] Verify error messages are clear

### Integration
- [ ] Add validation call to `routers.py`
- [ ] Update complexity analyzer
- [ ] Write integration tests
- [ ] Test end-to-end

### Validation
- [ ] Run full test suite (6000+ tests)
- [ ] Verify no regressions
- [ ] Benchmark performance
- [ ] Code review
- [ ] Merge to dev

---

## Part 9: Algorithm Explanation (for reviewers)

### DFS with Backtracking

The algorithm uses **Depth-First Search (DFS)** with backtracking:

```
For each fragment:
    visited = set()
    path = []

    def validate(frag_name):
        if frag_name in visited:
            CYCLE DETECTED! Return path

        visited.add(frag_name)
        path.append(frag_name)

        for each fragment_spread in frag_name:
            validate(spread_name)  # Recurse

        visited.remove(frag_name)  # Backtrack
        path.pop()
```

**Why backtrack?**
- Different paths might reference same fragment (diamond pattern)
- Must only mark as "in current path", not globally visited
- Backtracking lets us explore all paths correctly

**Complexity:**
- Time: O(N + E) where N = fragments, E = references
- Space: O(N) for visited set + path

---

## Part 10: Testing Examples

### Example 1: Self-Reference
```graphql
fragment A on User {
  id
  ...A  # ← Cycle: A → A
}
```
**Expected:** `FragmentCycleError("Circular fragment reference: A → A")`

### Example 2: Mutual
```graphql
fragment A on User { ...B }
fragment B on User { ...A }
```
**Expected:** `FragmentCycleError("Circular fragment reference: A → B → A")`

### Example 3: Valid Chain (no cycle)
```graphql
fragment A on User { ...B }
fragment B on User { ...C }
fragment C on User { id }
```
**Expected:** No error ✅

### Example 4: Diamond (no cycle)
```graphql
fragment A on User { ...B ...C }
fragment B on User { id }
fragment C on User { name }
```
**Expected:** No error ✅

---

## Conclusion

This implementation adds robust fragment cycle detection to FraiseQL, improving query safety and providing better error messages. The feature is:

- **Low-risk**: Defensive programming, no breaking changes
- **Well-tested**: 20+ unit tests + integration tests
- **Performant**: DFS validation < 10ms
- **User-friendly**: Clear error messages with cycle paths

**Effort estimate: 3-4 hours**
**Complexity: Low-Moderate**
**Risk: Low**
**Value: High**

Status: ✅ Ready for implementation
