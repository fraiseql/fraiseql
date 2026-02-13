# Phase 3: REFACTOR - Code Cleanup and Improvement

**Objective**: Improve code quality, structure, and maintainability without changing behavior.

**Status**: 🔵 REFACTOR (All tests still passing)

---

## Context

Phase 2 implemented a minimal fix. Now we refactor to:

- Improve code organization
- Add proper error handling
- Enhance type safety
- Improve performance
- Follow FraiseQL conventions

**Golden Rule**: Run tests after EACH refactoring step to ensure no behavior change.

---

## Files to Refactor

1. `fraiseql_rs/src/mutation/cascade_filter.rs` - Improve filtering logic
2. `fraiseql_rs/src/mutation/response_builder.rs` - Clean up CASCADE handling
3. `fraiseql/mutations/executor.py` - Clean up selection extraction
4. `fraiseql/mutations/cascade_selections.py` - Optimize parsing (if needed)

---

## Refactoring Steps

### Step 1: Improve Rust CASCADE Filter Structure

**File**: `fraiseql_rs/src/mutation/cascade_filter.rs`

**Improvements**:

1. Add comprehensive error handling
2. Improve camelCase conversion
3. Add type safety for field filtering
4. Add inline documentation

```rust
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
pub struct CascadeSelections {
    pub fields: Vec<String>,
    #[serde(default)]
    pub updated: Option<FieldSelections>,
    #[serde(default)]
    pub deleted: Option<FieldSelections>,
    #[serde(default)]
    pub invalidations: Option<FieldSelections>,
    #[serde(default)]
    pub metadata: Option<FieldSelections>,
}

#[derive(Debug, Deserialize)]
pub struct FieldSelections {
    pub fields: Vec<String>,
    #[serde(default)]
    pub entity_selections: Option<EntitySelections>,
}

#[derive(Debug, Deserialize)]
pub struct EntitySelections {
    #[serde(flatten)]
    pub type_selections: std::collections::HashMap<String, Vec<String>>,
}

pub fn filter_cascade_by_selections(
    cascade: &Value,
    selections: &CascadeSelections,
    auto_camel_case: bool,
) -> Result<Value, String> {
    let cascade_obj = match cascade {
        Value::Object(obj) => obj,
        _ => return Err("CASCADE must be an object".to_string()),
    };

    let mut filtered = Map::new();

    for field_name in &selections.fields {
        let key = convert_field_name(field_name, auto_camel_case);

        if let Some(value) = cascade_obj.get(&key) {
            let filtered_value = match field_name.as_str() {
                "updated" => filter_updated_field(value, selections.updated.as_ref())?,
                "deleted" => filter_simple_field(value, selections.deleted.as_ref())?,
                "invalidations" => filter_simple_field(value, selections.invalidations.as_ref())?,
                "metadata" => filter_simple_field(value, selections.metadata.as_ref())?,
                _ => value.clone(),
            };

            filtered.insert(key, filtered_value);
        }
    }

    Ok(Value::Object(filtered))
}

fn filter_updated_field(
    value: &Value,
    field_selections: Option<&FieldSelections>,
) -> Result<Value, String> {
    let Some(selections) = field_selections else {
        return Ok(value.clone());
    };

    if let Value::Array(entities) = value {
        let filtered_entities: Vec<Value> = entities
            .iter()
            .map(|entity| filter_entity_fields(entity, &selections.fields))
            .collect::<Result<_, _>>()?;

        Ok(Value::Array(filtered_entities))
    } else {
        Ok(value.clone())
    }
}

fn filter_simple_field(
    value: &Value,
    field_selections: Option<&FieldSelections>,
) -> Result<Value, String> {
    let Some(selections) = field_selections else {
        return Ok(value.clone());
    };

    if let Value::Array(items) = value {
        let filtered_items: Vec<Value> = items
            .iter()
            .map(|item| filter_object_fields(item, &selections.fields))
            .collect::<Result<_, _>>()?;

        Ok(Value::Array(filtered_items))
    } else if let Value::Object(_) = value {
        filter_object_fields(value, &selections.fields)
    } else {
        Ok(value.clone())
    }
}

fn filter_entity_fields(entity: &Value, fields: &[String]) -> Result<Value, String> {
    let entity_obj = match entity {
        Value::Object(obj) => obj,
        _ => return Ok(entity.clone()),
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = entity_obj.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    if !filtered.contains_key("__typename") {
        if let Some(typename) = entity_obj.get("__typename") {
            filtered.insert("__typename".to_string(), typename.clone());
        }
    }

    Ok(Value::Object(filtered))
}

fn filter_object_fields(obj: &Value, fields: &[String]) -> Result<Value, String> {
    let obj_map = match obj {
        Value::Object(map) => map,
        _ => return Ok(obj.clone()),
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = obj_map.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    Ok(Value::Object(filtered))
}

fn convert_field_name(field_name: &str, auto_camel_case: bool) -> String {
    if !auto_camel_case {
        return field_name.to_string();
    }

    let mut result = String::new();
    let mut capitalize_next = false;

    for (i, ch) in field_name.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else if i == 0 {
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_field_name_camel_case() {
        assert_eq!(convert_field_name("updated", true), "updated");
        assert_eq!(convert_field_name("affected_count", true), "affectedCount");
        assert_eq!(convert_field_name("query_name", true), "queryName");
    }

    #[test]
    fn test_convert_field_name_no_camel_case() {
        assert_eq!(convert_field_name("updated", false), "updated");
        assert_eq!(convert_field_name("affected_count", false), "affected_count");
    }

    #[test]
    fn test_filter_cascade_empty_selections() {
        let cascade = serde_json::json!({
            "updated": [],
            "deleted": [],
        });

        let selections = CascadeSelections {
            fields: vec![],
            updated: None,
            deleted: None,
            invalidations: None,
            metadata: None,
        };

        let result = filter_cascade_by_selections(&cascade, &selections, false).unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[test]
    fn test_filter_cascade_single_field() {
        let cascade = serde_json::json!({
            "updated": [{"id": "1"}],
            "deleted": [{"id": "2"}],
            "metadata": {"affectedCount": 2}
        });

        let selections = CascadeSelections {
            fields: vec!["metadata".to_string()],
            updated: None,
            deleted: None,
            invalidations: None,
            metadata: Some(FieldSelections {
                fields: vec!["affectedCount".to_string()],
                entity_selections: None,
            }),
        };

        let result = filter_cascade_by_selections(&cascade, &selections, false).unwrap();
        let result_obj = result.as_object().unwrap();

        assert!(result_obj.contains_key("metadata"));
        assert!(!result_obj.contains_key("updated"));
        assert!(!result_obj.contains_key("deleted"));
    }
}
```

**Run tests after this step**:

```bash
cd fraiseql-rs && cargo test
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs
```

---

### Step 2: Refactor Response Builder

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Improvements**:

1. Extract CASCADE handling to dedicated function
2. Improve error messages
3. Simplify conditional logic

```rust
// FIND: CASCADE handling code (around line 159-170)
// REPLACE WITH:

fn add_cascade_if_selected(
    obj: &mut Map<String, Value>,
    result: &MutationResult,
    cascade_selections: Option<&str>,
    auto_camel_case: bool,
) -> Result<(), String> {
    let Some(cascade) = &result.cascade else {
        return Ok(());
    };

    let Some(selections_json) = cascade_selections else {
        return Ok(());
    };

    let selections: CascadeSelections = serde_json::from_str(selections_json)
        .map_err(|e| format!("Invalid CASCADE selections JSON: {}", e))?;

    let filtered_cascade = filter_cascade_by_selections(
        cascade,
        &selections,
        auto_camel_case
    )?;

    obj.insert("cascade".to_string(), filtered_cascade);

    Ok(())
}

// Then in build_success_response, replace inline CASCADE code with:
add_cascade_if_selected(&mut obj, result, cascade_selections, auto_camel_case)?;
```

**Run tests**:

```bash
cd fraiseql-rs && cargo build
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs
```

---

### Step 3: Refactor Python Executor

**File**: `fraiseql/mutations/executor.py`

**Improvements**:

1. Extract CASCADE selection logic to helper method
2. Add type hints
3. Improve variable names

```python
def _get_cascade_selections(self, info: GraphQLResolveInfo | None) -> str | None:
    if not self.enable_cascade or not info:
        return None

    from fraiseql.mutations.cascade_selections import extract_cascade_selections

    return extract_cascade_selections(info)

# Then in the main execution method:
cascade_selections = self._get_cascade_selections(info)

result_bytes = build_mutation_response(
    json.dumps(result_data),
    success_type_name,
    entity_field_name,
    cascade_selections,
    auto_camel_case,
    success_type_fields,
)
```

**Run tests**:

```bash
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs
uv run pytest tests/integration/test_graphql_cascade.py -xvs
```

---

### Step 4: Add Type Safety to Cascade Selections

**File**: `fraiseql/mutations/cascade_selections.py`

**Improvements**:

1. Add comprehensive type hints
2. Improve function documentation
3. Add edge case handling

```python
from typing import Any, Optional
from graphql import FieldNode, GraphQLResolveInfo, InlineFragmentNode

def extract_cascade_selections(info: GraphQLResolveInfo) -> Optional[str]:
    """Extract cascade field selections from GraphQL query.

    Parses the GraphQL selection set to determine which CASCADE fields
    were requested by the client. Returns JSON for Rust consumption.

    Args:
        info: GraphQL resolve info containing field selections

    Returns:
        JSON string with requested CASCADE fields, or None if CASCADE not selected

    Example returned JSON:
        {"fields": ["updated", "metadata"], "updated": {"fields": ["__typename", "id"]}}
    """
    if not info or not info.field_nodes:
        return None

    for field_node in info.field_nodes:
        if not field_node.selection_set:
            continue

        for selection in field_node.selection_set.selections:
            if isinstance(selection, InlineFragmentNode):
                cascade_field = _find_cascade_in_fragment(selection)
                if cascade_field:
                    return _parse_cascade_to_json(cascade_field)
            elif hasattr(selection, "name") and selection.name.value == "cascade":
                return _parse_cascade_to_json(selection)

    return None


def _find_cascade_in_fragment(fragment: InlineFragmentNode) -> Optional[FieldNode]:
    if not fragment.selection_set:
        return None

    for selection in fragment.selection_set.selections:
        if hasattr(selection, "name") and selection.name.value == "cascade":
            return selection

    return None
```

**Run tests**:

```bash
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs
```

---

### Step 5: Performance Optimization

**File**: `fraiseql_rs/src/mutation/cascade_filter.rs`

**Optimizations**:

1. Avoid unnecessary clones
2. Use efficient data structures
3. Short-circuit when possible

```rust
pub fn filter_cascade_by_selections(
    cascade: &Value,
    selections: &CascadeSelections,
    auto_camel_case: bool,
) -> Result<Value, String> {
    if selections.fields.is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let cascade_obj = match cascade {
        Value::Object(obj) => obj,
        _ => return Err("CASCADE must be an object".to_string()),
    };

    let mut filtered = Map::with_capacity(selections.fields.len());

    // ... rest of implementation (same as before but with capacity hint)
}
```

**Run benchmarks** (if available):

```bash
cd fraiseql-rs && cargo bench
```

---

## Verification Commands

After EACH refactoring step:

```bash
# 1. Run new tests
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs

# 2. Run existing CASCADE tests
uv run pytest tests/integration/test_graphql_cascade.py -xvs

# 3. Run full test suite
uv run pytest tests/integration/ -x

# 4. Check Rust compilation
cd fraiseql-rs && cargo build --release

# 5. Run Rust tests
cd fraiseql-rs && cargo test
```

---

## Acceptance Criteria

- ✅ All tests still pass (no behavior change)
- ✅ Code is better organized and more readable
- ✅ Proper error handling added
- ✅ Type safety improved
- ✅ Performance optimized (no unnecessary allocations)
- ✅ Functions have clear, single responsibilities
- ✅ No code duplication

---

## DO NOT

- ❌ Change test behavior
- ❌ Add new features
- ❌ Fix unrelated bugs
- ❌ Add explanatory comments about the bug fix
- ❌ Change public APIs unnecessarily

---

## Refactoring Checklist

- [ ] Extract complex logic to helper functions
- [ ] Add proper error handling with descriptive messages
- [ ] Remove code duplication
- [ ] Improve variable and function names
- [ ] Add type hints (Python) and proper types (Rust)
- [ ] Optimize performance (avoid clones, allocations)
- [ ] Add unit tests for new helper functions
- [ ] Run all tests after each change

---

## Next Phase

After this phase completes:
→ **Phase 4: QA** - Test edge cases, update existing tests, validate against spec
