# ID Policy Validation Implementation Plan

## Overview
Implement actual ID policy enforcement on both Python and Rust sides, replacing the current advisory-only configuration with real security validation.

## Current State
- ✅ **Rust side**: Complete validation implementation exists in `fraiseql_rs/src/validation/`
  - `id_policy.rs`: UUID format validation logic (189 lines, fully tested)
  - `input_processor.rs`: Variable processing and field validation (incomplete export to Python)
- ❌ **Python side**: Configuration-only, no actual validation
  - `config/schema_config.py`: Stores policy but never enforces it
  - `types/scalars/id_scalar.py`: Uses GraphQL's built-in ID (accepts any string)
  - No GraphQL scalar with serialize/parse_value validation

- ❌ **Integration**: Validation functions not exported to Python via PyO3

## Architecture Decision

**Approach**: Dual-layer validation with Python as primary, Rust as fallback/secondary
- Python validates immediately at scalar serialization (fast, early rejection)
- Rust validates at WHERE clause processing (defense-in-depth)
- Both read from same SchemaConfig.id_policy

## Implementation Steps

### Phase 1: Export Rust Validation to Python (5 files)

**1.1 Add PyO3 bindings for ID validation**
- File: `fraiseql_rs/src/lib.rs`
- Add these PyO3 functions:
  ```rust
  #[pyfunction]
  pub fn validate_id_policy(id: &str, policy_str: &str) -> PyResult<()>

  #[pyfunction]
  pub fn validate_ids_policy(ids: Vec<String>, policy_str: &str) -> PyResult<()>
  ```
- Reason: Makes Rust validation callable from Python

**1.2 Handle bidirectional IDPolicy enum**
- File: `fraiseql_rs/src/validation/id_policy.rs` (modify serde config)
- Ensure enum serialization uses lowercase: "uuid" / "opaque"
- Already done, just verify it's accessible

### Phase 2: Create Python ID Scalar with Policy Enforcement (1 file)

**2.1 Create PolicyEnforcedIDScalar in new module**
- File: `src/fraiseql/types/scalars/id_policy_scalar.py`
- Implementation pattern (based on UUID scalar):
  ```python
  def serialize_id(value: Any) -> str:
      """Output serialization with policy validation."""
      # 1. Convert value to string
      # 2. Get current policy from SchemaConfig.get_instance()
      # 3. If UUID policy: validate with Rust validate_id()
      # 4. Return string or raise GraphQLError

  def parse_id_value(value: Any) -> str:
      """Input parsing with policy validation."""
      # Same as serialize_id

  def parse_id_literal(ast, variables=None) -> str:
      """Query literal parsing."""
      # Same validation as parse_id_value

  # Create GraphQLScalarType with these handlers
  ```

- Why separate file?: Clean separation, can be swapped in/out
- Test: 20+ test cases covering:
  - Valid UUIDs with UUID policy
  - Invalid UUIDs with UUID policy
  - OPAQUE policy accepts anything
  - Error messages are clear

### Phase 3: Wire Scalar into Schema Generation (1 file)

**3.1 Conditional scalar mapping**
- File: `src/fraiseql/types/scalars/graphql_utils.py` (modify)
- Update `convert_scalar_to_graphql()`:
  ```python
  def convert_scalar_to_graphql(typ: type) -> GraphQLScalarType:
      # ... existing code ...

      # OLD:
      # ID: GraphQLID,  # Always use built-in

      # NEW:
      # ID: _get_id_scalar(),  # Policy-aware

  def _get_id_scalar() -> GraphQLScalarType:
      """Get ID scalar based on current policy."""
      from fraiseql.config.schema_config import SchemaConfig
      from .id_policy_scalar import PolicyEnforcedIDScalar

      config = SchemaConfig.get_instance()
      if config.id_policy.enforces_uuid():
          return PolicyEnforcedIDScalar
      else:
          return GraphQLID
  ```

- Why now?: Scalar selection must happen at schema generation time
- Impact: Changes which scalar is used for ID type

### Phase 4: WHERE Clause Integration (1 file)

**4.1 Validate WHERE clause ID values**
- File: `src/fraiseql/where_normalization.py` (modify normalize_dict_where)
- Add validation before WHERE clause generation:
  ```python
  def normalize_dict_where(...):
      # ... existing code ...

      # NEW: After detecting a field is ID type:
      from fraiseql.config.schema_config import SchemaConfig

      config = SchemaConfig.get_instance()
      if field_type == ID and config.id_policy.enforces_uuid():
          # Validate each ID value in the filter
          id_value = filter_value.get('eq') or filter_value.get('in_')
          if id_value:
              _validate_id_in_where(id_value, config.id_policy)
  ```

- Reason: Early validation before SQL generation prevents bad SQL

### Phase 5: Comprehensive Testing (NEW file)

**5.1 Test file: test_id_policy_enforcement.py**
- Location: `tests/config/test_id_policy_enforcement.py`
- ~200 lines covering:
  - Scalar validation on query execution
  - WHERE clause validation
  - Error message clarity
  - Both policies working correctly
  - Integration with schema generation

### Phase 6: Documentation Updates (1 file)

**6.1 Update ID scalar documentation**
- File: `src/fraiseql/types/scalars/id_scalar.py`
- Change from: "UUID validation happens at the input validation layer"
- Change to: "UUID validation is enforced at the GraphQL scalar layer via SchemaConfig.id_policy"
- Add example showing both policies

## File Changes Summary

| File | Type | Changes | Lines |
|------|------|---------|-------|
| `fraiseql_rs/src/lib.rs` | Modify | Add PyO3 exports for validate_id | +20 |
| `src/fraiseql/types/scalars/id_policy_scalar.py` | Create | New scalar with policy validation | ~120 |
| `src/fraiseql/types/scalars/graphql_utils.py` | Modify | Conditional scalar selection | +10 |
| `src/fraiseql/types/scalars/id_scalar.py` | Modify | Update documentation | +5 |
| `src/fraiseql/where_normalization.py` | Modify | WHERE clause ID validation | +15 |
| `tests/config/test_id_policy_enforcement.py` | Create | Comprehensive validation tests | ~200 |
| `tests/config/test_id_policy.py` | Modify | Add behavior tests (not just config) | +30 |

**Total LOC**: ~400 lines (mostly tests, documentation)

## Testing Strategy

### Unit Tests
- ✅ UUID format validation (Rust module)
- NEW: Python scalar serialization/deserialization
- NEW: Policy-aware scalar selection
- NEW: WHERE clause ID value validation

### Integration Tests
- NEW: Full GraphQL query with policy enforcement
- NEW: Schema generation with different policies
- NEW: WHERE clause with ID filters + policy

### Edge Cases
- Empty string IDs (OPAQUE accepts, UUID rejects)
- Mixed case UUIDs (should accept)
- Nil UUID (should accept)
- Max UUID (should accept)
- SQL injection attempts in UUID policy (should reject)
- Path traversal attempts (should reject)

## Verification Commands

```bash
# Run new tests
make test-one TEST=tests/config/test_id_policy_enforcement.py

# Full test suite (should still pass)
make test

# Build Rust (required for PyO3 exports)
maturin develop

# Verify PyO3 exports available
python3 -c "from fraiseql._fraiseql_rs import validate_id_policy; print('✓ Exports working')"
```

## Rollout Plan

1. **Phase 1-2**: Export Rust functions, create Python scalar (no schema changes)
2. **Phase 3**: Wire scalar into schema generation (non-breaking, only affects ID type)
3. **Phase 4**: Add WHERE clause validation (purely defensive)
4. **Phase 5-6**: Tests + documentation

**Risk**: Existing code using OPAQUE policy should be unaffected
- Queries with valid UUIDs: pass with either policy
- Queries with non-UUID IDs: only work if explicitly set to OPAQUE

## Security Model After Implementation

```
Client Request (ID = "not-a-uuid")
    ↓
GraphQL Scalar Parsing [Python]
    ├─ Policy = UUID? → validate_id() → REJECT ✓
    └─ Policy = OPAQUE? → ACCEPT ✓
    ↓
WHERE Clause Normalization [Python]
    ├─ ID field with UUID policy? → validate again → REJECT ✓
    ↓
Rust Pipeline [Second layer defense]
    └─ WHERE clause would still fail if somehow bypassed
    ↓
PostgreSQL
    ├─ Parameterized query → safe from injection
```

## Success Criteria

- ✅ Invalid UUIDs rejected with UUID policy
- ✅ Any string accepted with OPAQUE policy
- ✅ Errors are clear and actionable
- ✅ No performance regression
- ✅ All 5991+ existing tests still pass
- ✅ 50+ new validation tests pass
- ✅ Committed with clear message: `feat(security): enforce ID Policy validation on Python + Rust layers`
