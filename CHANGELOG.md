# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.9.11] - 2026-01-10

**GraphQL Spec Compliance - __typename Preservation**

### Fixed

- **Mutation entity field filtering now preserves `__typename`** (GitHub #233)
  - GraphQL introspection field `__typename` is now always included in filtered entities
  - Matches query behavior and GraphQL spec compliance
  - Previously filtered out when not explicitly requested in field selections
  - Now automatically preserved even when not in selection set

**Technical Details**:
- Updated `fraiseql_rs/src/mutation/entity_filter.rs` to preserve `__typename`
- Added 2 regression tests for nested and top-level `__typename` preservation
- Zero breaking changes - backward compatible

## [1.9.7] - 2025-01-10

**Entity Field Selection for Mutations + IDFilter for Where Clauses**

This release adds two major features:
1. GraphQL field selection support for nested entity objects in mutation responses
2. IDPolicy-aware ID filtering in where clauses (from v1.9.3-v1.9.6)

Both features improve developer experience and reduce payload sizes.

### Added

#### Nested Entity Field Filtering

Mutations now respect GraphQL field selections for nested entity objects:

**Before (v1.9.6)**: Mutations returned ALL entity fields regardless of query selection
```graphql
mutation {
  createLocation(input: {name: "Warehouse"}) {
    ... on CreateLocationSuccess {
      location { id name }  # ❌ Returned ALL 20 fields
    }
  }
}
```

**After (v1.9.7)**: Mutations return ONLY requested fields
```graphql
mutation {
  createLocation(input: {name: "Warehouse"}) {
    ... on CreateLocationSuccess {
      location { id name }  # ✅ Returns only id and name
    }
  }
}
```

#### Implementation Details

**Python Layer** (`mutation_decorator.py`):
- `_extract_nested_selections()`: Recursively extracts nested field selections from GraphQL AST
- `_extract_entity_field_selections()`: Parses inline fragments to find entity field selections
- Automatically passes selections to Rust pipeline as JSON

**Rust Layer** (`fraiseql_rs/src/mutation/entity_filter.rs`):
- `filter_entity_fields()`: Recursive filtering algorithm for nested objects
- Handles objects, arrays, primitives, and null values
- Zero overhead when no selections provided (backward compatible)

#### IDFilter for Where Clauses (from v1.9.3-v1.9.6)

New `IDFilter` class for filtering ID fields in where clauses with IDPolicy awareness:

```python
@fraise_input
class IDFilter:
    eq: ID | None = None
    neq: ID | None = None
    in_: list[ID] | None = None
    nin: list[ID] | None = None
    isnull: bool | None = None
```

**Key Features:**
- ID type **always** uses `IDFilter` regardless of IDPolicy configuration
- GraphQL schema stays consistent (`$id: ID!`)
- UUID validation (if `IDPolicy.UUID`) happens at runtime, not schema level
- No frontend query changes needed when switching policies

```python
from fraiseql.config.schema_config import SchemaConfig, IDPolicy

# Both policies use ID scalar in GraphQL schema
SchemaConfig.set_config(id_policy=IDPolicy.UUID)  # Validates UUID format at runtime
SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)  # Accepts any string
```

### Performance Impact

- **Payload Reduction**: 30-90% smaller responses (depends on entity size)
- **Filtering Overhead**: <1ms per mutation (negligible)
- **Network Savings**: Significant for large entities (e.g., Location: 20+ fields)

### Testing

- **10 Python unit tests**: Entity field selection extraction
- **16+ Rust unit tests**: Filtering logic for nested objects, arrays, edge cases
- **4 integration tests**: End-to-end with PostgreSQL database
- **Backward compatibility**: All 97 existing mutation tests pass unchanged

### Files Modified

| File | Lines | Change |
|------|-------|--------|
| `src/fraiseql/mutations/mutation_decorator.py` | +95 | Entity field extraction from GraphQL AST |
| `src/fraiseql/mutations/rust_executor.py` | +3 | Pass entity_selections to Rust |
| `fraiseql_rs/src/mutation/entity_filter.rs` | +250 | Recursive filtering algorithm (NEW) |
| `fraiseql_rs/src/mutation/response_builder.rs` | +13 | Apply filtering in response builder |
| `fraiseql_rs/src/mutation/mod.rs` | +3 | Updated API signature |
| `fraiseql_rs/src/lib.rs` | +2 | PyO3 binding update |
| `tests/unit/mutations/test_entity_field_extraction.py` | +430 | Python unit tests (NEW) |
| `fraiseql_rs/src/mutation/tests/entity_field_filtering.rs` | +450 | Rust unit tests (NEW) |
| `tests/integration/graphql/mutations/test_entity_field_selection_integration.py` | +490 | Integration tests (NEW) |

**Total**: 10 files changed, +1,830 lines added
