# Release Notes - FraiseQL v0.10.2

## ✨ Mutation Input Transformation and Empty String Handling

### Release Date: 2025-10-06
### Type: Feature Enhancement

## Summary

This release adds powerful input transformation capabilities to mutations and improves frontend compatibility with automatic empty string handling. Two complementary features enable clean separation of frontend and backend data formats while maintaining type safety.

## 🎯 New Features

### 1. `prepare_input` Hook for Mutations (Fixes #75)

Adds an optional `prepare_input` static method to mutation classes that allows transforming input data **after GraphQL validation** but **before the PostgreSQL function call**.

#### Use Cases
- Multi-field transformations (IP + subnet mask → CIDR notation)
- Empty string normalization (custom logic)
- Date format conversions
- Coordinate transformations (lat/lng → PostGIS point)
- Unit conversions (imperial → metric)
- Multi-field combinations (street + city + zip → full address)

#### Example: IP Address + Subnet Mask → CIDR

**Frontend sends:**
```graphql
mutation {
  createNetworkConfiguration(input: {
    ipAddress: "192.168.1.1"
    subnetMask: "255.255.255.0"
  })
}
```

**Backend transformation:**
```python
@mutation
class CreateNetworkConfig:
    input: NetworkConfigInput
    success: NetworkConfigSuccess
    error: NetworkConfigError

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        """Transform IP + subnet mask to CIDR notation."""
        ip = input_data.get("ip_address")
        mask = input_data.get("subnet_mask")

        if ip and mask:
            # Convert subnet mask to CIDR prefix
            cidr_prefix = {
                "255.255.255.0": 24,
                "255.255.0.0": 16,
                "255.0.0.0": 8,
            }.get(mask, 32)

            return {
                "ip_address": f"{ip}/{cidr_prefix}",
                # subnet_mask field is removed
            }
        return input_data
```

**Database receives:**
```sql
INSERT INTO network_config (ip_address)
VALUES ('192.168.1.1/24'::inet);
```

#### How It Works

1. **GraphQL validation** - Input validated against GraphQL schema
2. **Input to dict** - Input object converted to dictionary
3. **✨ prepare_input called** - Your transformation logic executes
4. **PostgreSQL function** - Transformed data sent to database

#### Non-Breaking Change

The `prepare_input` hook is **completely optional**:
- Existing mutations without the hook work unchanged
- No changes to mutation decorator API
- Hook only runs if defined on mutation class

### 2. Automatic Empty String to NULL Conversion

Frontends commonly send empty strings (`""`) when users clear text fields. FraiseQL now automatically converts empty strings to `None` for optional fields while maintaining data quality validation for required fields.

#### Problem Solved

**Before v0.10.2:**
```python
# Frontend sends empty string when user clears notes field
{ id: "note-123", notes: "" }

# Backend rejects with validation error ❌
ValueError: Field 'notes' cannot be empty
```

**After v0.10.2:**
```python
# Frontend sends empty string (standard behavior)
{ id: "note-123", notes: "" }

# Backend accepts and converts to None ✅
{ id: "note-123", notes: null }

# Database stores NULL (proper semantics)
UPDATE notes SET notes = NULL WHERE id = 'note-123';
```

#### Behavior

| Field Type | Empty String `""` | None | Validation |
|-----------|------------------|------|------------|
| **Required** (`name: str`) | ❌ Rejected | ❌ Rejected | Strict data quality |
| **Optional** (`notes: str \| None`) | ✅ Accepted → `None` | ✅ Accepted | Frontend-friendly |

#### Example: Note Update Mutation

```python
@fraise_input
class UpdateNoteInput:
    id: UUID
    notes: str | None = None  # Optional field

@mutation
class UpdateNote:
    input: UpdateNoteInput
    success: UpdateNoteSuccess
    error: UpdateNoteError

# Frontend clears notes field
input_obj = UpdateNoteInput(id="...", notes="")

# Input validation: ✅ Accepted (optional field)
# Serialization: "" → None automatically
# Database: NULL stored correctly
```

#### Implementation Details

**Where Conversion Happens:**
1. `_serialize_field_value()` in `types/constructor.py` - During `to_dict()` serialization
2. `_to_dict()` in `mutations/mutation_decorator.py` - For non-FraiseQL input objects

**Validation Changes:**
- `_validate_input_string_value()` in `utils/fraiseql_builder.py` - Only rejects empty strings for **required** fields
- Optional fields can accept empty strings (will be converted to `None` during serialization)

## Impact

### Who Benefits?

1. **Frontend Developers** - Standard form behavior, no need to send `null` explicitly
2. **Backend Developers** - Clean data transformations without custom resolvers
3. **Full-Stack Applications** - Proper NULL semantics in database
4. **API Users** - More intuitive mutation APIs

### Performance

- **Zero overhead** - Transformations only run when mutations execute
- **No extra queries** - Same number of database calls
- **Efficient** - Simple string checks and dict operations

## Technical Details

### Files Changed

#### 1. `src/fraiseql/mutations/mutation_decorator.py`
- Added `prepare_input` hook support (lines 113-115)
- Enhanced `_to_dict()` with empty string conversion (lines 640-642)
- Added comprehensive documentation with examples

#### 2. `src/fraiseql/types/constructor.py`
- Modified `_serialize_field_value()` to convert empty strings to `None` (lines 44-47)

#### 3. `src/fraiseql/utils/fraiseql_builder.py`
- Updated `_validate_input_string_value()` to only reject empty strings for required fields (line 60)
- Added documentation about optional field behavior

### Test Coverage

#### New Tests

**prepare_input Hook Tests:**
```python
# tests/unit/decorators/test_mutation_decorator.py
- test_prepare_input_transforms_data_before_database_call
- test_mutation_without_prepare_input_works_normally
- test_prepare_input_can_convert_empty_strings_to_null
```

**Empty String Conversion Tests:**
```python
# tests/unit/decorators/test_empty_string_to_null.py (NEW FILE)
- test_optional_field_accepts_empty_string_in_input_type
- test_to_dict_converts_empty_string_to_none_for_optional_fields
- test_to_dict_preserves_non_empty_strings
- test_to_dict_preserves_explicit_none
- test_required_string_still_rejects_empty_string
- test_optional_field_with_default_accepts_empty_string
```

**Updated Test:**
```python
# tests/unit/core/type_system/test_empty_string_validation.py
- test_optional_string_accepts_empty_when_provided (updated for new behavior)
```

#### Test Results
✅ 3 new `prepare_input` hook tests pass
✅ 6 new empty string conversion tests pass
✅ All 3,295 existing tests pass (no regressions)
✅ 100% backward compatible

## Migration Guide

### No Action Required ✅

This release is **completely backward compatible**:

1. **Automatic benefits** - Empty string handling works immediately
2. **Optional hook** - Only use `prepare_input` if you need transformations
3. **No schema changes** - Existing mutations continue working
4. **No configuration changes** - Framework handles conversions automatically

### Upgrade

```bash
pip install fraiseql==0.10.2
```

or with uv:

```bash
uv add fraiseql==0.10.2
```

### Using the prepare_input Hook

If you want to use input transformations, simply add the `prepare_input` static method:

```python
@mutation
class YourMutation:
    input: YourInput
    success: YourSuccess
    error: YourError

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        """Transform input data before database call."""
        # Your transformation logic here
        return transformed_data
```

### Verification

After upgrading, verify empty string handling:

```python
# Test with optional string field
@fraise_input
class TestInput:
    notes: str | None = None

# Frontend sends empty string
input_obj = TestInput(notes="")
assert input_obj.notes == ""  # Stored as empty string in object

# Serialization converts to None
result = input_obj.to_dict()
assert result["notes"] is None  # ✅ Converted to None for database
```

## Benefits Summary

### Developer Experience
✅ **Clean separation** of frontend and backend data formats
✅ **No custom resolvers** needed for common transformations
✅ **Standard frontend behavior** supported out of the box
✅ **Type safety maintained** with GraphQL schema validation

### Data Quality
✅ **Required fields protected** - Empty strings still rejected
✅ **Optional fields flexible** - Accept empty strings, convert to NULL
✅ **Proper NULL semantics** - Database stores NULL, not empty strings
✅ **Validation preserved** - GraphQL schema validation runs first

### Reusability
✅ **Transformation patterns** can be shared across mutations
✅ **Consistent behavior** for similar field types
✅ **No duplication** in PostgreSQL functions
✅ **Middleware-free** - No global hooks affecting all mutations

### Production Ready
✅ **Non-breaking changes** - Existing code works unchanged
✅ **Comprehensive tests** - All 3,295+ tests pass
✅ **Performance neutral** - No overhead for existing mutations
✅ **Well documented** - Examples in mutation decorator docstring

## Related Use Cases

### Coordinate Transformations
```python
@staticmethod
def prepare_input(input_data: dict) -> dict:
    """Convert lat/lng to PostGIS point."""
    lat = input_data.get("latitude")
    lng = input_data.get("longitude")

    if lat is not None and lng is not None:
        return {
            "location": f"POINT({lng} {lat})",
            # latitude and longitude removed
        }
    return input_data
```

### Date Format Conversions
```python
@staticmethod
def prepare_input(input_data: dict) -> dict:
    """Convert frontend date format to ISO."""
    date_str = input_data.get("date")

    if date_str:
        # Convert "MM/DD/YYYY" → "YYYY-MM-DD"
        from datetime import datetime
        date_obj = datetime.strptime(date_str, "%m/%d/%Y")
        input_data["date"] = date_obj.strftime("%Y-%m-%d")

    return input_data
```

### Unit Conversions
```python
@staticmethod
def prepare_input(input_data: dict) -> dict:
    """Convert miles to kilometers."""
    distance_miles = input_data.get("distance_miles")

    if distance_miles is not None:
        return {
            "distance_km": distance_miles * 1.60934,
            # distance_miles removed
        }
    return input_data
```

## Related Issues

Fixes #75 - Add input_transformer/prepare_input hook support to mutation decorator

## Acknowledgments

Thank you to the community for feedback on frontend/backend data format mismatches and the need for input transformation capabilities in mutations.

---

**Note:** This release makes FraiseQL mutations more frontend-friendly while maintaining strict data quality validation for required fields.
