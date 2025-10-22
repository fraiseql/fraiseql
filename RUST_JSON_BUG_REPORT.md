# Rust JSON Bug Report: Incorrect __typename for Nested Objects

**Date**: 2025-10-22
**Priority**: CRITICAL
**Status**: ROOT CAUSE IDENTIFIED

---

## 🐛 Bug Description

The Rust pipeline (`fraiseql-rs`) incorrectly assigns the parent object's `__typename` to nested objects, causing the nested object to have the wrong GraphQL type name.

---

## 📊 Impact

- **5 skipped tests** due to this bug
- **Blocks nested object filtering** - critical feature
- **Invalid GraphQL responses** - breaks type system
- **Production blocker** - affects any query with nested objects

---

## 🔍 Root Cause

When the Rust pipeline generates JSON for queries with nested objects, it copies the parent type's `__typename` to all nested objects instead of using the correct type for each nested field.

### Example

**Schema**:
```python
@fraiseql.type
class Machine:
    id: UUID
    name: str

@fraiseql.type
class RouterConfig:
    id: UUID
    machine_id: UUID
    config_name: str
    is_current: bool
    machine: Optional[Machine]  # Nested object
```

**Query**:
```python
where = {"machine": {"id": {"eq": machine_id}}}
results = await repo.find("test_router_config_view", where=where)
```

**Actual JSON Output (WRONG)**:
```json
{
  "data": {
    "test_router_config_view": [{
      "__typename": "RouterConfig",
      "id": "6fb1c7a6-3fa8-4c37-acc0-7986262a51c1",
      "machine": {
        "__typename": "RouterConfig",  # ❌ WRONG! Should be "Machine"
        "id": "005e2378-e240-46d2-9e86-a9ec52cf9589",
        "name": "router-01"
      },
      "configName": "config-v1"
    }]
  }
}
```

**Expected JSON Output (CORRECT)**:
```json
{
  "data": {
    "test_router_config_view": [{
      "__typename": "RouterConfig",
      "id": "6fb1c7a6-3fa8-4c37-acc0-7986262a51c1",
      "machine": {
        "__typename": "Machine",  # ✅ CORRECT!
        "id": "005e2378-e240-46d2-9e86-a9ec52cf9589",
        "name": "router-01"
      },
      "configName": "config-v1"
    }]
  }
}
```

---

## 🔬 Reproduction Steps

1. **Setup test tables with nested objects**:
```python
# Parent table
CREATE TABLE test_machines (
    id UUID PRIMARY KEY,
    name TEXT
);

# Child table with foreign key
CREATE TABLE test_router_configs (
    id UUID PRIMARY KEY,
    machine_id UUID REFERENCES test_machines(id),
    config_name TEXT,
    is_current BOOLEAN
);

# View with JOIN
CREATE VIEW test_router_config_view AS
SELECT
    rc.id,
    rc.machine_id,
    rc.config_name,
    rc.is_current,
    jsonb_build_object(
        'id', m.id,
        'name', m.name
    ) as machine
FROM test_router_configs rc
JOIN test_machines m ON rc.machine_id = m.id;
```

2. **Query with nested object filter**:
```python
repo = FraiseQLRepository(db_pool)
results = await repo.find(
    "test_router_config_view",
    where={"machine": {"id": {"eq": machine_id}}}
)
```

3. **Observe malformed JSON**:
- Nested `machine` object has wrong `__typename`
- Should be `"Machine"` but is `"RouterConfig"`

---

## 🧩 Where the Bug Is

This is a **Rust crate bug** in `fraiseql-rs`, specifically in the JSON generation/type name injection logic.

### Possible Locations in fraiseql-rs:

1. **Type name injection during JSON building**:
   ```rust
   // Somewhere in fraiseql-rs
   // When building nested objects, it's not looking up the correct type
   fn inject_typename(obj: &mut JsonValue, parent_typename: &str) {
       // Bug: Using parent_typename for nested objects
       obj["__typename"] = parent_typename;  // ❌ WRONG!
   }
   ```

2. **Field type resolution**:
   ```rust
   // Should resolve field type from schema
   fn get_field_typename(parent_type: &str, field_name: &str) -> String {
       // Bug: Returning parent type instead of field type
       parent_type.to_string()  // ❌ WRONG!
   }
   ```

---

## 🛠️ Solutions

### Option A: Fix in Rust Crate (PROPER FIX)

**Pros**:
- ✅ Fixes root cause
- ✅ Clean solution
- ✅ Benefits all users

**Cons**:
- ⏰ Requires Rust expertise
- ⏰ Need to wait for release
- ⏰ 1-2 weeks timeline

**Steps**:
1. Clone fraiseql-rs repository
2. Find type name injection logic
3. Fix to use correct field type
4. Add tests for nested objects
5. Submit PR
6. Wait for review/merge/release
7. Update fraiseql dependency

### Option B: Python Workaround (QUICK FIX)

**Pros**:
- ✅ Can fix immediately
- ✅ Unblocks users
- ✅ Under our control

**Cons**:
- ⚠️ Technical debt
- ⚠️ Performance overhead
- ⚠️ Needs cleanup later

**Implementation**:

```python
# src/fraiseql/core/rust_pipeline.py

def fix_nested_object_typenames(json_data: dict, schema_info: dict) -> dict:
    """
    Workaround for Rust bug: Fix incorrect __typename in nested objects.

    Args:
        json_data: The malformed JSON from Rust pipeline
        schema_info: Schema information to look up correct types

    Returns:
        Fixed JSON with correct __typename values
    """
    if "data" not in json_data:
        return json_data

    for field_name, field_data in json_data["data"].items():
        if isinstance(field_data, list):
            for item in field_data:
                fix_item_nested_typenames(item, field_name, schema_info)
        elif isinstance(field_data, dict):
            fix_item_nested_typenames(field_data, field_name, schema_info)

    return json_data


def fix_item_nested_typenames(item: dict, parent_type: str, schema_info: dict):
    """Fix __typename in nested objects recursively."""
    if not isinstance(item, dict):
        return

    # Get schema for parent type
    parent_schema = schema_info.get(parent_type, {})

    for field_name, field_value in item.items():
        if isinstance(field_value, dict) and "__typename" in field_value:
            # This is a nested object with __typename
            # Look up correct type from schema
            correct_typename = parent_schema.get("fields", {}).get(field_name, {}).get("type")

            if correct_typename and field_value["__typename"] != correct_typename:
                # Fix the wrong typename
                logger.warning(
                    f"Rust bug detected: Fixed __typename for {parent_type}.{field_name} "
                    f"from '{field_value['__typename']}' to '{correct_typename}'"
                )
                field_value["__typename"] = correct_typename

            # Recurse into nested object
            fix_item_nested_typenames(field_value, correct_typename, schema_info)
```

**Usage**:
```python
# In RustResponseBytes parsing
class RustResponseBytes:
    def __bytes__(self):
        raw_json = self._raw_bytes
        json_data = json.loads(raw_json)

        # Apply workaround
        if ENABLE_RUST_BUG_WORKAROUND:  # Config flag
            json_data = fix_nested_object_typenames(json_data, schema_info)

        return json.dumps(json_data).encode()
```

---

## 📝 Affected Tests

All in `tests/integration/database/repository/test_dict_where_mixed_filters_bug.py`:

1. ✅ `test_dict_where_with_nested_filter_only` - Isolated nested filter
2. ✅ `test_dict_where_with_direct_filter_only` - Isolated direct filter
3. ✅ `test_dict_where_with_mixed_nested_and_direct_filters_BUG` - Mixed filters
4. ✅ `test_dict_where_with_multiple_direct_filters_after_nested` - Edge case
5. ✅ `test_dict_where_with_direct_filter_before_nested` - Order test

---

## 🎯 Recommended Action

**Immediate (Today)**: Implement Python Workaround (Option B)
- Fixes all 5 tests today
- Unblocks nested object filtering
- Users can use the feature

**Long-term (This Month)**: Submit PR to fraiseql-rs (Option A)
- File issue in fraiseql-rs repo
- Submit fix with tests
- Remove workaround when fixed

---

## 🧪 Verification

After fix, all these should work:

```python
# 1. Nested object filter only
where = {"machine": {"id": {"eq": machine_id}}}

# 2. Direct field filter only
where = {"is_current": {"eq": True}}

# 3. Mixed filters
where = {
    "machine": {"id": {"eq": machine_id}},
    "is_current": {"eq": True}
}

# 4. Multiple filters
where = {
    "machine": {"id": {"eq": machine_id}},
    "is_current": {"eq": True},
    "config_name": {"eq": "config-v2"}
}
```

---

## 📊 Timeline

### With Python Workaround:
- **Today**: Implement workaround (4-6 hours)
- **Today**: All 5 tests passing
- **This week**: File issue + PR to fraiseql-rs
- **Next month**: Remove workaround when fixed upstream

### Without Workaround (waiting for Rust fix):
- **This week**: File issue in fraiseql-rs
- **1-2 weeks**: PR review/merge
- **2-3 weeks**: New release
- **3-4 weeks**: Update dependency, tests pass

---

## 🚀 Next Steps

1. **Decide**: Python workaround OR wait for Rust fix?
2. **Implement**: Based on decision
3. **Test**: Verify all 5 tests pass
4. **Document**: Add known issue if using workaround
5. **Follow up**: File Rust issue regardless

---

**Recommendation**: Implement Python workaround NOW, fix Rust crate in parallel.

---

*Bug identified: 2025-10-22*
*Next action: Implement Python workaround*
