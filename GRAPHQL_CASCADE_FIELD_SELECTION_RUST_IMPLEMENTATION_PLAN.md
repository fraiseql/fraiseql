# GraphQL Cascade Field Selection - Rust Implementation Plan

**Version**: 1.0
**Target Release**: v1.6.0
**Estimated Effort**: 16-24 hours
**Created**: 2025-11-13
**Status**: Planning

---

## 📋 Executive Summary

Implement **GraphQL Cascade field selection** in Rust to allow clients to specify which cascade data they need, reducing response payload size and improving performance. This feature will filter cascade data based on GraphQL query selections before serialization.

### Current State (v1.5.0)
- ✅ Cascade data returned from PostgreSQL in `_cascade` field
- ✅ Python JSON encoder passes cascade through to response
- ❌ **No field selection** - all cascade data returned regardless of query
- ❌ Cascade field not exposed in GraphQL schema

### Target State (v1.6.0)
- ✅ Clients can request specific cascade fields in mutation
- ✅ Rust-based cascade filtering for performance
- ✅ Cascade field properly typed in GraphQL schema
- ✅ Backward compatible - cascade optional in queries

---

## 🎯 Feature Requirements

### GraphQL Query Example
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    post { id title }
    message

    # Client specifies cascade field selections
    cascade {
      # Only include Post and User updates
      updated(include: ["Post", "User"]) {
        __typename
        id
        operation
        entity {
          # Client can select entity fields
          id
          ... on Post { title content }
          ... on User { name postCount }
        }
      }

      # Include deletion info
      deleted {
        __typename
        id
      }

      # Include invalidation hints
      invalidations {
        queryName
        strategy
        scope
      }

      # Skip metadata
      # metadata { ... }
    }
  }
}
```

### Expected Behavior
1. **Field Selection**: Only requested cascade fields returned
2. **Type Filtering**: `include` argument filters by `__typename`
3. **Entity Selection**: GraphQL selection applied to entity objects
4. **Optional**: Cascade field optional in mutations
5. **Performance**: Filtering happens in Rust before Python serialization

---

## 🏗️ Architecture Overview

### Execution Flow

```
┌─────────────────────────────────────────────────────────────┐
│ 1. GraphQL Mutation Query                                   │
│    └─ Contains cascade { ... } field selections             │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Python Mutation Resolver                                 │
│    └─ Execute PostgreSQL function                           │
│    └─ Get result with _cascade JSONB                        │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Extract Cascade Field Selections (PYTHON)                │
│    └─ Parse GraphQL field selections for 'cascade'          │
│    └─ Serialize to JSON for Rust                            │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Rust Cascade Filter (NEW - fraiseql_rs)                  │
│    └─ filter_cascade_data(cascade_json, selections_json)    │
│    └─ Apply type filtering (include/exclude)                │
│    └─ Apply field selections to entities                    │
│    └─ Zero-copy JSON manipulation                           │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. Python JSON Encoder                                      │
│    └─ Attach filtered cascade to response                   │
│    └─ Serialize to HTTP response                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 📦 Component Design

### 1. Rust Module: `cascade_filter.rs`

Location: `fraiseql_rs/src/cascade/mod.rs`

```rust
//! GraphQL Cascade field selection and filtering
//!
//! This module provides high-performance filtering of cascade data based on
//! GraphQL field selections. It operates on raw JSONB from PostgreSQL and
//! applies filtering before Python serialization.

use serde_json::{Value, Map};
use std::collections::HashSet;

/// Cascade field selection metadata from GraphQL query
#[derive(Debug, Clone)]
pub struct CascadeSelections {
    /// Selected fields at cascade root level
    /// e.g., ["updated", "deleted", "invalidations"]
    pub fields: HashSet<String>,

    /// Type filters for 'updated' field
    /// e.g., ["Post", "User"] for include: ["Post", "User"]
    pub updated_include: Option<Vec<String>>,
    pub updated_exclude: Option<Vec<String>>,

    /// Field selections for 'updated.entity' objects
    /// Keyed by __typename, contains field names
    pub entity_selections: Map<String, HashSet<String>>,

    /// Field selections for other cascade fields
    pub deleted_fields: Option<HashSet<String>>,
    pub invalidations_fields: Option<HashSet<String>>,
    pub metadata_fields: Option<HashSet<String>>,
}

impl CascadeSelections {
    /// Parse cascade selections from GraphQL field selection JSON
    ///
    /// Expected JSON format from Python:
    /// ```json
    /// {
    ///   "fields": ["updated", "deleted", "invalidations"],
    ///   "updated": {
    ///     "include": ["Post", "User"],
    ///     "exclude": null,
    ///     "fields": ["__typename", "id", "operation", "entity"],
    ///     "entity_selections": {
    ///       "Post": ["id", "title", "content"],
    ///       "User": ["id", "name", "postCount"]
    ///     }
    ///   },
    ///   "deleted": {
    ///     "fields": ["__typename", "id"]
    ///   }
    /// }
    /// ```
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Invalid cascade selections JSON: {}", e))?;

        // Parse root fields
        let fields = v.get("fields")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Parse updated filters
        let updated_obj = v.get("updated");
        let updated_include = updated_obj
            .and_then(|u| u.get("include"))
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let updated_exclude = updated_obj
            .and_then(|u| u.get("exclude"))
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        // Parse entity selections
        let entity_selections = updated_obj
            .and_then(|u| u.get("entity_selections"))
            .and_then(|e| e.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(typename, fields)| {
                        let field_set = fields.as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (typename.clone(), field_set)
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse deleted, invalidations, metadata fields
        let deleted_fields = Self::parse_field_set(&v, "deleted");
        let invalidations_fields = Self::parse_field_set(&v, "invalidations");
        let metadata_fields = Self::parse_field_set(&v, "metadata");

        Ok(CascadeSelections {
            fields,
            updated_include,
            updated_exclude,
            entity_selections,
            deleted_fields,
            invalidations_fields,
            metadata_fields,
        })
    }

    fn parse_field_set(v: &Value, field_name: &str) -> Option<HashSet<String>> {
        v.get(field_name)
            .and_then(|f| f.get("fields"))
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
    }
}

/// Filter cascade data based on GraphQL field selections
///
/// This is the main entry point called from Python.
///
/// # Arguments
/// * `cascade_json` - Raw JSONB cascade data from PostgreSQL (JSON string)
/// * `selections_json` - Parsed GraphQL field selections (JSON string)
///
/// # Returns
/// Filtered cascade data as JSON string
///
/// # Performance
/// - Zero-copy JSON manipulation where possible
/// - Operates on serde_json::Value for efficiency
/// - Target: < 0.5ms for typical cascade payloads
pub fn filter_cascade_data(
    cascade_json: &str,
    selections_json: Option<&str>,
) -> Result<String, String> {
    // If no selections provided, return original cascade
    let Some(sel_json) = selections_json else {
        return Ok(cascade_json.to_string());
    };

    // Parse cascade data
    let mut cascade: Value = serde_json::from_str(cascade_json)
        .map_err(|e| format!("Invalid cascade JSON: {}", e))?;

    // Parse selections
    let selections = CascadeSelections::from_json(sel_json)?;

    // Filter cascade object
    if let Some(obj) = cascade.as_object_mut() {
        filter_cascade_object(obj, &selections)?;
    }

    // Serialize back to JSON
    serde_json::to_string(&cascade)
        .map_err(|e| format!("Failed to serialize filtered cascade: {}", e))
}

/// Filter the cascade object in place
fn filter_cascade_object(
    obj: &mut Map<String, Value>,
    selections: &CascadeSelections,
) -> Result<(), String> {
    // Remove fields not in selections
    obj.retain(|key, _| selections.fields.contains(key));

    // Filter 'updated' array
    if let Some(updated) = obj.get_mut("updated") {
        filter_updated_array(updated, selections)?;
    }

    // Filter 'deleted' array
    if let Some(deleted) = obj.get_mut("deleted") {
        filter_simple_array(deleted, &selections.deleted_fields)?;
    }

    // Filter 'invalidations' array
    if let Some(invalidations) = obj.get_mut("invalidations") {
        filter_simple_array(invalidations, &selections.invalidations_fields)?;
    }

    // Filter 'metadata' object
    if let Some(metadata) = obj.get_mut("metadata") {
        filter_metadata_object(metadata, &selections.metadata_fields)?;
    }

    Ok(())
}

/// Filter the 'updated' array based on type filters and field selections
fn filter_updated_array(
    updated: &mut Value,
    selections: &CascadeSelections,
) -> Result<(), String> {
    let Some(arr) = updated.as_array_mut() else {
        return Ok(());
    };

    // Apply type filtering (include/exclude)
    if let Some(ref include_types) = selections.updated_include {
        arr.retain(|item| {
            item.get("__typename")
                .and_then(|t| t.as_str())
                .map(|typename| include_types.iter().any(|t| t == typename))
                .unwrap_or(false)
        });
    } else if let Some(ref exclude_types) = selections.updated_exclude {
        arr.retain(|item| {
            item.get("__typename")
                .and_then(|t| t.as_str())
                .map(|typename| !exclude_types.iter().any(|t| t == typename))
                .unwrap_or(true)
        });
    }

    // Apply field selections to each entity
    for item in arr.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            filter_updated_item(obj, selections)?;
        }
    }

    Ok(())
}

/// Filter a single updated item
fn filter_updated_item(
    item: &mut Map<String, Value>,
    selections: &CascadeSelections,
) -> Result<(), String> {
    // Get typename for entity field selection
    let typename = item.get("__typename")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    // Filter entity object
    if let Some(entity) = item.get_mut("entity") {
        if let Some(entity_obj) = entity.as_object_mut() {
            // Get field selection for this typename
            if let Some(field_selection) = selections.entity_selections.get(typename) {
                entity_obj.retain(|key, _| field_selection.contains(key));
            }
        }
    }

    Ok(())
}

/// Filter a simple array (deleted, invalidations)
fn filter_simple_array(
    arr: &mut Value,
    field_selection: &Option<HashSet<String>>,
) -> Result<(), String> {
    let Some(arr_val) = arr.as_array_mut() else {
        return Ok(());
    };

    let Some(fields) = field_selection else {
        return Ok(());
    };

    for item in arr_val.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            obj.retain(|key, _| fields.contains(key));
        }
    }

    Ok(())
}

/// Filter metadata object
fn filter_metadata_object(
    metadata: &mut Value,
    field_selection: &Option<HashSet<String>>,
) -> Result<(), String> {
    let Some(obj) = metadata.as_object_mut() else {
        return Ok(());
    };

    let Some(fields) = field_selection else {
        return Ok(());
    };

    obj.retain(|key, _| fields.contains(key));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_cascade_no_selections() {
        let cascade = r#"{"updated": [], "deleted": []}"#;
        let result = filter_cascade_data(cascade, None).unwrap();
        assert_eq!(result, cascade);
    }

    #[test]
    fn test_filter_updated_by_typename() {
        let cascade = r#"{
            "updated": [
                {"__typename": "Post", "id": "1", "entity": {}},
                {"__typename": "User", "id": "2", "entity": {}},
                {"__typename": "Comment", "id": "3", "entity": {}}
            ]
        }"#;

        let selections = r#"{
            "fields": ["updated"],
            "updated": {
                "include": ["Post", "User"]
            }
        }"#;

        let result = filter_cascade_data(cascade, Some(selections)).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        let updated = v["updated"].as_array().unwrap();
        assert_eq!(updated.len(), 2);
    }

    #[test]
    fn test_filter_entity_fields() {
        let cascade = r#"{
            "updated": [{
                "__typename": "Post",
                "id": "1",
                "entity": {
                    "id": "1",
                    "title": "Hello",
                    "content": "World",
                    "authorId": "123"
                }
            }]
        }"#;

        let selections = r#"{
            "fields": ["updated"],
            "updated": {
                "entity_selections": {
                    "Post": ["id", "title"]
                }
            }
        }"#;

        let result = filter_cascade_data(cascade, Some(selections)).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        let entity = &v["updated"][0]["entity"];
        assert!(entity.get("id").is_some());
        assert!(entity.get("title").is_some());
        assert!(entity.get("content").is_none());
        assert!(entity.get("authorId").is_none());
    }
}
```

---

### 2. Python Integration: `cascade_selections.py`

Location: `src/fraiseql/mutations/cascade_selections.py`

```python
"""GraphQL field selection parser for cascade data.

Extracts cascade field selections from GraphQL query and converts them
to a format that Rust can efficiently process.
"""

from typing import Any, Optional, Dict, List, Set
from graphql import GraphQLResolveInfo, FieldNode


def extract_cascade_selections(info: GraphQLResolveInfo) -> Optional[str]:
    """Extract cascade field selections from GraphQL query.

    Parses the GraphQL field selections for the 'cascade' field and converts
    them to JSON format for Rust processing.

    Args:
        info: GraphQL resolve info containing field selections

    Returns:
        JSON string containing cascade selections, or None if no cascade field

    Example:
        >>> # For query: mutation { createPost { cascade { updated { id } } } }
        >>> selections = extract_cascade_selections(info)
        >>> # Returns: '{"fields": ["updated"], "updated": {"fields": ["id"]}}'
    """
    # Find the cascade field in current field selections
    cascade_field = _find_cascade_field(info.field_nodes)
    if not cascade_field:
        return None

    # Parse cascade selections
    selections = _parse_cascade_field(cascade_field, info)

    # Convert to JSON for Rust
    import json
    return json.dumps(selections, separators=(',', ':'))


def _find_cascade_field(field_nodes: List[FieldNode]) -> Optional[FieldNode]:
    """Find the 'cascade' field in field selections."""
    for field in field_nodes:
        if not field.selection_set:
            continue

        for selection in field.selection_set.selections:
            if hasattr(selection, 'name') and selection.name.value == 'cascade':
                return selection

    return None


def _parse_cascade_field(
    cascade_field: FieldNode,
    info: GraphQLResolveInfo
) -> Dict[str, Any]:
    """Parse cascade field selections into structured format."""
    selections: Dict[str, Any] = {
        'fields': []
    }

    if not cascade_field.selection_set:
        return selections

    for selection in cascade_field.selection_set.selections:
        if not hasattr(selection, 'name'):
            continue

        field_name = selection.name.value
        selections['fields'].append(field_name)

        # Parse field-specific selections
        if field_name == 'updated':
            selections['updated'] = _parse_updated_field(selection)
        elif field_name == 'deleted':
            selections['deleted'] = _parse_simple_field(selection)
        elif field_name == 'invalidations':
            selections['invalidations'] = _parse_simple_field(selection)
        elif field_name == 'metadata':
            selections['metadata'] = _parse_simple_field(selection)

    return selections


def _parse_updated_field(field_node: FieldNode) -> Dict[str, Any]:
    """Parse 'updated' field with type filters and entity selections."""
    result: Dict[str, Any] = {
        'fields': [],
        'include': None,
        'exclude': None,
        'entity_selections': {}
    }

    # Parse arguments (include/exclude)
    if field_node.arguments:
        for arg in field_node.arguments:
            if arg.name.value == 'include':
                result['include'] = _parse_string_list_arg(arg)
            elif arg.name.value == 'exclude':
                result['exclude'] = _parse_string_list_arg(arg)

    # Parse field selections
    if not field_node.selection_set:
        return result

    for selection in field_node.selection_set.selections:
        if not hasattr(selection, 'name'):
            continue

        field_name = selection.name.value
        result['fields'].append(field_name)

        # Parse entity field selections
        if field_name == 'entity':
            result['entity_selections'] = _parse_entity_selections(selection)

    return result


def _parse_entity_selections(field_node: FieldNode) -> Dict[str, List[str]]:
    """Parse entity field selections with inline fragments for different types."""
    entity_selections: Dict[str, List[str]] = {}

    if not field_node.selection_set:
        return entity_selections

    # Common fields (not in inline fragment)
    common_fields: List[str] = []

    for selection in field_node.selection_set.selections:
        # Regular field
        if hasattr(selection, 'name'):
            common_fields.append(selection.name.value)

        # Inline fragment (... on Post { ... })
        elif hasattr(selection, 'type_condition'):
            typename = selection.type_condition.name.value
            fields = _extract_field_names(selection)
            entity_selections[typename] = fields + common_fields

    # If no inline fragments, use common fields for all types
    if not entity_selections and common_fields:
        entity_selections['__default__'] = common_fields

    return entity_selections


def _parse_simple_field(field_node: FieldNode) -> Dict[str, Any]:
    """Parse simple field (deleted, invalidations, metadata)."""
    result: Dict[str, Any] = {
        'fields': []
    }

    if field_node.selection_set:
        for selection in field_node.selection_set.selections:
            if hasattr(selection, 'name'):
                result['fields'].append(selection.name.value)

    return result


def _parse_string_list_arg(arg) -> List[str]:
    """Parse a list of string arguments."""
    if not hasattr(arg.value, 'values'):
        return []

    return [
        value.value
        for value in arg.value.values
        if hasattr(value, 'value')
    ]


def _extract_field_names(selection_node) -> List[str]:
    """Extract field names from selection set."""
    if not selection_node.selection_set:
        return []

    return [
        selection.name.value
        for selection in selection_node.selection_set.selections
        if hasattr(selection, 'name')
    ]
```

---

### 3. Mutation Decorator Integration

Location: `src/fraiseql/mutations/mutation_decorator.py` (update existing)

```python
# Add at top of file
from fraiseql.mutations.cascade_selections import extract_cascade_selections

# Update create_resolver method (around line 159-165)
async def resolver(info: GraphQLResolveInfo, input: dict[str, Any]) -> Any:
    # ... existing code ...

    # Check for cascade data if enabled
    if self.enable_cascade:
        # Extract cascade field selections from GraphQL query
        cascade_selections = extract_cascade_selections(info)

        if "_cascade" in result:
            # Filter cascade data using Rust if selections present
            if cascade_selections:
                filtered_cascade = _filter_cascade_rust(
                    result["_cascade"],
                    cascade_selections
                )
                parsed_result.__cascade__ = filtered_cascade
            else:
                parsed_result.__cascade__ = result["_cascade"]
        elif parsed_result.extra_metadata and "_cascade" in parsed_result.extra_metadata:
            if cascade_selections:
                filtered_cascade = _filter_cascade_rust(
                    parsed_result.extra_metadata["_cascade"],
                    cascade_selections
                )
                parsed_result.__cascade__ = filtered_cascade
            else:
                parsed_result.__cascade__ = parsed_result.extra_metadata["_cascade"]

    return parsed_result


def _filter_cascade_rust(cascade_data: dict, selections_json: str) -> dict:
    """Filter cascade data using Rust implementation.

    Args:
        cascade_data: Raw cascade data from PostgreSQL
        selections_json: JSON string of field selections

    Returns:
        Filtered cascade data
    """
    try:
        from fraiseql import fraiseql_rs
        import json

        # Convert cascade data to JSON
        cascade_json = json.dumps(cascade_data, separators=(',', ':'))

        # Call Rust filter
        filtered_json = fraiseql_rs.filter_cascade_data(
            cascade_json,
            selections_json
        )

        # Parse back to dict
        return json.loads(filtered_json)

    except Exception as e:
        import logging
        logger = logging.getLogger(__name__)
        logger.warning(f"Cascade filtering failed, returning unfiltered: {e}")
        return cascade_data
```

---

### 4. GraphQL Schema Type Definition

Location: `src/fraiseql/mutations/types.py` (update existing)

```python
"""Mutation types with cascade support."""

from typing import Any, Dict, List, Optional
from fraiseql import fraise_type, fraise_enum

# Cascade operation enum
@fraise_enum
class CascadeOperation:
    """Type of operation performed on an entity."""
    CREATED = "CREATED"
    UPDATED = "UPDATED"
    DELETED = "DELETED"

# Cascade invalidation strategy
@fraise_enum
class InvalidationStrategy:
    """Cache invalidation strategy."""
    INVALIDATE = "INVALIDATE"
    REFETCH = "REFETCH"

# Cascade invalidation scope
@fraise_enum
class InvalidationScope:
    """Scope of cache invalidation."""
    PREFIX = "PREFIX"
    EXACT = "EXACT"
    ALL = "ALL"

# Cascade updated entity
@fraise_type
class CascadeUpdatedEntity:
    """Entity that was created or updated."""
    __typename: str
    id: str
    operation: CascadeOperation
    entity: Optional[Dict[str, Any]] = None  # Generic object for entity data

# Cascade deleted entity
@fraise_type
class CascadeDeletedEntity:
    """Entity that was deleted."""
    __typename: str
    id: str

# Cascade invalidation hint
@fraise_type
class CascadeInvalidation:
    """Query cache invalidation hint."""
    query_name: str
    strategy: InvalidationStrategy
    scope: Optional[InvalidationScope] = None

# Cascade metadata
@fraise_type
class CascadeMetadata:
    """Operation metadata."""
    timestamp: Optional[str] = None
    affected_count: Optional[int] = None

# Main cascade object
@fraise_type
class CascadeData:
    """GraphQL Cascade data for cache updates."""
    updated: Optional[List[CascadeUpdatedEntity]] = None
    deleted: Optional[List[CascadeDeletedEntity]] = None
    invalidations: Optional[List[CascadeInvalidation]] = None
    metadata: Optional[CascadeMetadata] = None


# Update MutationResultBase to include cascade
class MutationResultBase:
    """Base class for mutation results with cascade support."""

    # Cascade data (set by mutation resolver)
    __cascade__: Optional[Dict[str, Any]] = None

    # ... existing code ...
```

---

### 5. Rust PyO3 Export

Location: `fraiseql_rs/src/lib.rs` (update existing)

```rust
// Add cascade module
pub mod cascade;

// Export filter_cascade_data function
#[pyfunction]
pub fn filter_cascade_data(
    cascade_json: &str,
    selections_json: Option<&str>,
) -> PyResult<String> {
    cascade::filter_cascade_data(cascade_json, selections_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
}

#[pymodule]
fn _fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // ... existing exports ...

    // Add cascade filter
    m.add_function(wrap_pyfunction!(filter_cascade_data, m)?)?;

    // Update __all__
    m.add("__all__", vec![
        // ... existing exports ...
        "filter_cascade_data",
    ])?;

    Ok(())
}
```

---

## 🧪 Testing Strategy

### Unit Tests (Rust)

Location: `fraiseql_rs/src/cascade/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_selections_returns_original() {
        let cascade = r#"{"updated": [], "deleted": []}"#;
        let result = filter_cascade_data(cascade, None).unwrap();
        assert_eq!(result, cascade);
    }

    #[test]
    fn test_filter_by_typename_include() {
        // Test include filter
    }

    #[test]
    fn test_filter_by_typename_exclude() {
        // Test exclude filter
    }

    #[test]
    fn test_filter_entity_fields() {
        // Test entity field selection
    }

    #[test]
    fn test_filter_nested_fields() {
        // Test nested object filtering
    }

    #[test]
    fn test_invalid_cascade_json() {
        // Test error handling
    }

    #[test]
    fn test_invalid_selections_json() {
        // Test error handling
    }

    #[test]
    fn test_empty_selections() {
        // Test empty selections behavior
    }

    #[test]
    fn test_performance_large_cascade() {
        // Benchmark with 1000+ entities
    }
}
```

### Integration Tests (Python)

Location: `tests/integration/test_cascade_field_selection.py`

```python
"""Integration tests for cascade field selection."""

import pytest
from fraiseql import fraiseql_rs

def test_cascade_filter_basic():
    """Test basic cascade filtering."""
    cascade_json = '{"updated": [{"__typename": "Post", "id": "1"}]}'
    selections = '{"fields": ["updated"]}'

    result = fraiseql_rs.filter_cascade_data(cascade_json, selections)
    assert "updated" in result

def test_cascade_filter_typename():
    """Test typename filtering."""
    # Test with include/exclude
    pass

def test_cascade_filter_entity_fields():
    """Test entity field selection."""
    pass

def test_cascade_graphql_query():
    """Test full GraphQL query with cascade field selection."""
    # End-to-end test with GraphQL query
    pass
```

### Performance Benchmarks

Location: `fraiseql_rs/benches/cascade_benchmark.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_cascade_filter(c: &mut Criterion) {
    let cascade = generate_cascade_data(100); // 100 entities
    let selections = r#"{"fields": ["updated"], "updated": {"include": ["Post"]}}"#;

    c.bench_function("cascade_filter_100", |b| {
        b.iter(|| {
            filter_cascade_data(
                black_box(&cascade),
                black_box(Some(selections))
            )
        })
    });
}

criterion_group!(benches, benchmark_cascade_filter);
criterion_main!(benches);
```

---

## 📊 Performance Targets

### Rust Cascade Filtering
- **Small payload** (1-10 entities): < 0.1ms
- **Medium payload** (10-50 entities): < 0.3ms
- **Large payload** (50-100 entities): < 0.5ms
- **Extra large** (100-500 entities): < 2ms

### Memory Usage
- **Zero-copy**: Reuse serde_json::Value where possible
- **Peak memory**: < 2MB for typical payloads
- **No allocations**: For no-filtering case

---

## 🚀 Implementation Phases

### Phase 1: Rust Core (8 hours)
- [ ] Create `cascade_filter.rs` module
- [ ] Implement `CascadeSelections` struct
- [ ] Implement `filter_cascade_data` function
- [ ] Implement type filtering (include/exclude)
- [ ] Implement entity field filtering
- [ ] Write Rust unit tests
- [ ] Performance benchmarks

### Phase 2: Python Integration (4 hours)
- [ ] Create `cascade_selections.py` parser
- [ ] Implement GraphQL field selection extraction
- [ ] Update `mutation_decorator.py` integration
- [ ] Add Rust function call in resolver
- [ ] Error handling and fallback

### Phase 3: GraphQL Schema (3 hours)
- [ ] Define cascade GraphQL types
- [ ] Add `CascadeData` type
- [ ] Add cascade field to mutation success types
- [ ] Update schema generation
- [ ] Validate schema

### Phase 4: Testing (4 hours)
- [ ] Python integration tests
- [ ] End-to-end GraphQL tests
- [ ] Performance benchmarks
- [ ] Error handling tests
- [ ] Backward compatibility tests

### Phase 5: Documentation (3 hours)
- [ ] Update cascade feature docs
- [ ] Add field selection examples
- [ ] Migration guide for v1.6.0
- [ ] API reference
- [ ] Performance tuning guide

---

## 🔄 Backward Compatibility

### Ensuring Zero Breaking Changes

1. **Cascade field optional**: Mutations work without cascade field
2. **No selections = full data**: Default behavior unchanged
3. **Graceful degradation**: Errors fall back to unfiltered cascade
4. **Schema compatible**: New types don't break existing queries

### Migration Path

```python
# v1.5.0 (current) - works unchanged
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostFailure

# v1.6.0 - add cascade field to success type
@success
class CreatePostSuccess:
    post: Post
    message: str
    cascade: Optional[CascadeData] = None  # NEW - optional
```

---

## 📝 Documentation Requirements

### User-Facing Documentation

1. **Feature Guide**: `docs/features/graphql-cascade.md`
   - Add field selection section
   - Example queries with selections
   - Performance recommendations

2. **Migration Guide**: `docs/migration/v1.5-to-v1.6.md`
   - How to add cascade field to success types
   - Query examples
   - Performance tuning

3. **API Reference**: `docs/api/cascade-types.md`
   - `CascadeData` type reference
   - All cascade-related types
   - Field arguments (include/exclude)

### Developer Documentation

1. **Architecture**: `docs/architecture/cascade-filtering.md`
   - Rust implementation details
   - Python-Rust interface
   - Performance characteristics

2. **Testing**: `tests/integration/cascade_field_selection/README.md`
   - Test scenarios
   - Performance benchmarks
   - Debugging tips

---

## 🎯 Success Criteria

### Functional Requirements
- ✅ Clients can request specific cascade fields
- ✅ Type filtering works (include/exclude)
- ✅ Entity field selection works
- ✅ All cascade fields support selection
- ✅ Backward compatible (no breaking changes)

### Performance Requirements
- ✅ Rust filtering < 0.5ms for typical payloads
- ✅ Zero memory allocations for no-filter case
- ✅ No performance regression for existing code

### Quality Requirements
- ✅ 100% test coverage for Rust code
- ✅ Integration tests for all scenarios
- ✅ Performance benchmarks documented
- ✅ Complete user documentation
- ✅ No regressions in existing tests

---

## 🔍 Edge Cases & Error Handling

### Invalid Selections
- **Malformed JSON**: Return unfiltered cascade + log warning
- **Invalid field names**: Ignore unknown fields
- **Conflicting filters**: include takes precedence over exclude

### Empty Results
- **All entities filtered**: Return empty arrays (not null)
- **No cascade data**: Return null cascade field
- **No selections**: Return full cascade data

### Performance Degradation
- **Very large payloads** (1000+ entities): Consider pagination warning
- **Complex entity selections**: Optimize with zero-copy JSON manipulation
- **Fallback**: On Rust error, return unfiltered data

---

## 🚀 Release Plan

### v1.6.0 Release Checklist
- [ ] All phases completed
- [ ] Tests passing (100% coverage)
- [ ] Documentation complete
- [ ] Performance benchmarks met
- [ ] Backward compatibility verified
- [ ] CHANGELOG updated
- [ ] Migration guide written
- [ ] Examples updated

### Post-Release
- [ ] Monitor performance metrics
- [ ] Collect user feedback
- [ ] Document common patterns
- [ ] Consider future optimizations

---

## 📚 References

### GraphQL Cascade Specification
- Official spec (if public)
- Apollo Client cascade handling
- Relay cache updates

### FraiseQL Architecture
- `docs/architecture/rust-pipeline.md`
- `docs/features/graphql-cascade.md`
- Mutation decorator implementation

### Rust Resources
- `serde_json` documentation
- PyO3 Python-Rust interface
- Zero-copy JSON manipulation

---

**Status**: Planning Complete
**Next Step**: Begin Phase 1 - Rust Core Implementation
**Estimated Completion**: 22 hours development + 3 hours testing/docs
