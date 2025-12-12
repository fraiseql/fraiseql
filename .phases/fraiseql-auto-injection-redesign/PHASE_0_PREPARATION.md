# Phase 0: Preparation & Diagnostic Tooling

**Objective**: Set up diagnostic logging and understand current system behavior
**Duration**: 2 hours
**Dependencies**: None

---

## Tasks

### Task 1: Add Diagnostic Logging to Python Field Extraction (30 min)

**File**: `src/fraiseql/mutations/mutation_decorator.py`

**Location**: Lines 19-66 in `_extract_mutation_selected_fields()`

**Add logging**:

```python
def _extract_mutation_selected_fields(info: GraphQLResolveInfo, type_name: str) -> list[str] | None:
    """Extract fields selected on a mutation response type from GraphQL query."""

    # ADD THIS LOGGING
    logger.warning(f"🔍 FIELD EXTRACTION DEBUG:")
    logger.warning(f"  Type name: {type_name}")

    if not info or not info.field_nodes:
        logger.warning(f"  No info or field_nodes - returning None")
        return None

    selected_fields = set()

    # Mutations typically have one field_node (the mutation field)
    for field_node in info.field_nodes:
        logger.warning(f"  Field node: {field_node.name.value if hasattr(field_node, 'name') else 'no name'}")

        if not field_node.selection_set:
            logger.warning(f"    No selection_set on field_node")
            continue

        # Look through selections for fragments matching our type
        for selection in field_node.selection_set.selections:
            logger.warning(f"    Selection: {type(selection).__name__}")

            # InlineFragment with type condition (e.g., "... on CreateMachineSuccess")
            if hasattr(selection, "type_condition") and selection.type_condition:
                fragment_type = selection.type_condition.name.value
                logger.warning(f"      Type condition: {fragment_type}")
                logger.warning(f"      Matches {type_name}? {fragment_type == type_name}")

                if fragment_type == type_name and selection.selection_set:
                    # Extract fields from this fragment
                    for field_selection in selection.selection_set.selections:
                        if hasattr(field_selection, "name"):
                            field_name = field_selection.name.value
                            # Skip __typename (always included by GraphQL)
                            if field_name != "__typename":
                                selected_fields.add(field_name)

    # ADD THIS LOGGING
    if not selected_fields:
        logger.warning(f"  Extracted fields: None (no fields found - backward compat mode)")
        return None

    result = list(selected_fields)
    logger.warning(f"  Extracted fields: {result}")
    logger.warning(f"  Field count: {len(result)}")

    return result
```

**Verification**:
```bash
cd /home/lionel/code/fraiseql
pytest tests/mutations/test_field_extraction.py -xvs --log-cli-level=WARNING
```

**Expected output**:
```
🔍 FIELD EXTRACTION DEBUG:
  Type name: CreateMachineSuccess
  Field node: createMachine
    Selection: InlineFragment
      Type condition: CreateMachineSuccess
      Matches CreateMachineSuccess? True
  Extracted fields: ['status', 'message', 'machine']
  Field count: 3
```

---

### Task 2: Enhance Rust Diagnostic Logging (30 min)

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Current logging** (lines 98-125) is already good. Optional enhancements:

**Add database response logging** at start of `build_success_response`:

```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    // 🔍 DIAGNOSTIC LOGGING
    eprintln!("🔍 RUST SUCCESS RESPONSE DEBUG:");
    eprintln!("  Type: {}", success_type);
    eprintln!("  Entity field name: {:?}", entity_field_name);
    eprintln!("  success_type_fields: {:?}", success_type_fields);
    eprintln!("  should_filter: {}", success_type_fields.is_some());

    // ADD: Log database response
    eprintln!("  Database result:");
    eprintln!("    status: {:?}", result.status);
    eprintln!("    message: {:?}", result.message);
    eprintln!("    entity_id: {:?}", result.entity_id);
    eprintln!("    has entity: {}", result.entity.is_some());
    eprintln!("    updated_fields: {:?}", result.updated_fields);

    let mut obj = Map::new();
    // ... rest of function
}
```

**Rebuild Rust extension**:
```bash
cd /home/lionel/code/fraiseql
maturin develop --release
```

**Verification**:
```bash
cargo test --package fraiseql_rs --lib mutation::response_builder::tests -- --nocapture
```

---

### Task 3: Create Test Suite for Edge Cases (1 hour)

**Create file**: `tests/mutations/test_field_extraction_edge_cases.py`

```python
"""Test edge cases in field extraction."""

import pytest
from graphql import parse, execute, build_schema, GraphQLResolveInfo
from fraiseql.mutations.mutation_decorator import _extract_mutation_selected_fields


@pytest.fixture
def mock_info():
    """Create mock GraphQL resolve info."""
    # This will be implemented based on actual GraphQL info structure
    pass


def test_fragment_type_name_mismatch():
    """Test when fragment type doesn't match expected type."""
    # Query: ... on CreateMachineSuccess
    # Type name: "CreateMachineError"
    # Expected: Returns None (no match)
    pass


def test_named_fragments_vs_inline():
    """Test behavior with named fragments."""
    # Query uses named fragment:
    # fragment MachineFields on CreateMachineSuccess { status machine { id } }
    # mutation { createMachine { ...MachineFields } }
    pass


def test_multiple_field_nodes():
    """Test when info has multiple field_nodes."""
    # Edge case: Multiple mutation fields in one query
    pass


def test_nested_mutations():
    """Test field extraction for nested mutation results."""
    # Complex query with nested mutations
    pass


def test_no_inline_fragment():
    """Test when query has no inline fragment (backward compat)."""
    # Query: mutation { createMachine { status machine { id } } }
    # No "... on CreateMachineSuccess"
    # Expected: Returns None (backward compat)
    pass


def test_empty_fragment():
    """Test when fragment has no fields selected."""
    # Query: mutation { createMachine { ... on CreateMachineSuccess { } } }
    # Expected: Returns None or empty list?
    pass


def test_typename_skipped():
    """Test that __typename is not included in extracted fields."""
    # Query: mutation { createMachine { ... on CreateMachineSuccess { __typename status } } }
    # Expected: ['status'] (not ['__typename', 'status'])
    pass
```

**Implementation**:

Implement these tests one by one, using actual GraphQL `parse()` to create AST nodes and `_extract_mutation_selected_fields()` to extract fields.

**Run tests**:
```bash
cd /home/lionel/code/fraiseql
pytest tests/mutations/test_field_extraction_edge_cases.py -xvs --log-cli-level=WARNING
```

---

## Verification

**After completing all tasks**:

```bash
# 1. Python diagnostic logging works
pytest tests/mutations/test_field_extraction.py -xvs --log-cli-level=WARNING | grep "FIELD EXTRACTION"

# 2. Rust diagnostic logging works
cargo test --package fraiseql_rs --lib mutation::response_builder::tests -- --nocapture | grep "RUST"

# 3. Edge case tests pass
pytest tests/mutations/test_field_extraction_edge_cases.py -xvs
```

---

## Acceptance Criteria

- ✅ Python field extraction has comprehensive logging
- ✅ Rust response builder has enhanced logging
- ✅ Edge case test suite created and documented
- ✅ All tests pass
- ✅ Logging helps identify field extraction issues

---

## Deliverables

1. Enhanced `mutation_decorator.py` with diagnostic logging
2. Enhanced `response_builder.rs` with database response logging
3. New test file `test_field_extraction_edge_cases.py`
4. Understanding of field extraction reliability

---

## Rollback Plan

Diagnostic logging is non-breaking - can be left in production or removed later. No rollback needed unless performance issues.

---

## Next Phase

After Phase 0 complete, proceed to **Phase 1: Python Decorator Changes**
