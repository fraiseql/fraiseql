# Feature Request: Auto-Map sql_source Based on Type Name Convention

## Summary

Automatically infer `sql_source` from the type name using FraiseQL's naming conventions, eliminating the need to manually specify `sql_source="v_user"` for every type.

## Problem Statement

Currently, users must manually specify `sql_source` for every type, even when following FraiseQL's standard naming conventions:

```python
@fraiseql.type(sql_source="v_user")  # ← Redundant if following conventions
@dataclass
class User:
    id: UUID
    name: str

@fraiseql.type(sql_source="v_customer")  # ← Redundant
@dataclass
class Customer:
    id: UUID
    name: str

@fraiseql.type(sql_source="v_order")  # ← Redundant
@dataclass
class Order:
    id: UUID
    customer_id: UUID
```

**Problems:**
- ❌ Repetitive boilerplate - must specify for every type
- ❌ Easy to make mistakes (typos, forgetting singular/plural)
- ❌ Not DRY - type name already implies the source
- ❌ Maintenance burden - renaming a type requires updating `sql_source`

## Proposed Solution

### Auto-Map with Intelligent Source Detection

Automatically infer `sql_source` from the type name using FraiseQL's established conventions, with **intelligent fallback** based on what actually exists in the database:

```python
# Convention: Type name → sql_source mapping with detection
User      → Check: tv_user → mv_user → v_user (auto-detect and prefer tables)
Customer  → Check: tv_customer → mv_customer → v_customer
Order     → Check: tv_order → mv_order → v_order
Product   → Check: tv_product → mv_product → v_product
```

**Fallback Priority** (performance-optimized):
1. **Tables first** (`tv_*`) - Best performance, indexed, direct access
2. **Materialized views** (`mv_*`) - Pre-computed, fast queries
3. **Views** (`v_*`) - Flexible, but slower (computed on query)

**Rationale**: Tables provide the best performance and are the source of truth. Use views/materialized views when tables don't exist or when explicit flexibility is needed.

### Usage Example

```python
@fraiseql.type  # ← No sql_source needed!
@dataclass
class User:
    id: UUID
    name: str
    # Automatically uses: sql_source="v_user"

@fraiseql.type  # ← No sql_source needed!
@dataclass
class Customer:
    id: UUID
    name: str
    # Automatically uses: sql_source="v_customer"

@fraiseql.type  # ← No sql_source needed!
@dataclass
class Order:
    id: UUID
    customer_id: UUID
    # Automatically uses: sql_source="v_order"
```

### Override When Needed

Explicit `sql_source` still works (for non-standard cases):

```python
@fraiseql.type(sql_source="legacy_users_table")  # ← Explicit override
@dataclass
class User:
    id: UUID

@fraiseql.type(sql_source="mv_analytics_user")  # ← Materialized view
@dataclass
class AnalyticsUser:
    id: UUID
```

## Design Specification

### Intelligent Source Detection Algorithm

```python
async def detect_and_infer_sql_source(
    type_name: str,
    db_pool,
    schema: str | None = None,
    disable_detection: bool = False,
) -> str:
    """Detect available database source and infer sql_source with fallback.

    Detection order (performance-optimized):
    1. Tables (tv_*) - Direct access, best performance
    2. Materialized views (mv_*) - Pre-computed, fast
    3. Views (v_*) - Computed on query, flexible

    Args:
        type_name: Python class name (e.g., "User")
        db_pool: Database connection pool for introspection
        schema: Optional schema (default: public)
        disable_detection: Skip detection, use default prefix

    Returns:
        Detected or inferred sql_source

    Examples:
        # Database has tv_user (table)
        User → "tv_user" (detected)

        # Database has only v_user (view)
        User → "v_user" (detected)

        # Database has mv_customer and v_customer
        Customer → "mv_customer" (prefers materialized view)

        # Database has none, detection disabled
        Order → "v_order" (default fallback)
    """
    snake_name = camel_to_snake(type_name)
    schema_prefix = f"{schema}." if schema else ""

    # Skip detection if disabled (for performance or testing)
    if disable_detection:
        return f"{schema_prefix}v_{snake_name}"

    # Try detection with fallback priority
    candidates = [
        f"{schema_prefix}tv_{snake_name}",   # Tables (preferred)
        f"{schema_prefix}mv_{snake_name}",   # Materialized views
        f"{schema_prefix}v_{snake_name}",    # Views
    ]

    async with db_pool.connection() as conn:
        for candidate in candidates:
            if await source_exists(conn, candidate, schema):
                return candidate

    # No source found - return default (v_*)
    # Will cause error at query time if source doesn't exist
    return f"{schema_prefix}v_{snake_name}"


async def source_exists(conn, source_name: str, schema: str | None = None) -> bool:
    """Check if database source (table/view/materialized view) exists.

    Uses information_schema for efficient lookup with caching.
    """
    # Parse schema-qualified name
    if "." in source_name:
        schema_part, table_part = source_name.split(".", 1)
    else:
        schema_part = schema or "public"
        table_part = source_name

    # Query information_schema (cached by PostgreSQL)
    result = await conn.execute(
        """
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = %s
              AND table_name = %s
        )
        """,
        (schema_part, table_part),
    )
    row = await result.fetchone()
    return row[0] if row else False


def camel_to_snake(name: str) -> str:
    """Convert PascalCase/camelCase to snake_case.

    Examples:
        User          → user
        UserProfile   → user_profile
        APIKey        → api_key
        OrderItem     → order_item
    """
    import re

    # Insert underscore before uppercase letters
    s1 = re.sub('(.)([A-Z][a-z]+)', r'\1_\2', name)
    # Insert underscore before uppercase in sequences
    s2 = re.sub('([a-z0-9])([A-Z])', r'\1_\2', s1)

    return s2.lower()
```

### Detection Caching Strategy

To avoid repeated database introspection on every type decoration:

```python
# Global cache for detected sources
_detected_sources_cache: dict[tuple[str, str | None], str] = {}


async def detect_and_infer_sql_source_cached(
    type_name: str,
    db_pool,
    schema: str | None = None,
) -> str:
    """Cached version of source detection.

    Cache key: (type_name, schema)
    Cache lifetime: Application lifetime (cleared on restart)
    """
    cache_key = (type_name, schema)

    if cache_key in _detected_sources_cache:
        return _detected_sources_cache[cache_key]

    # Perform detection
    source = await detect_and_infer_sql_source(type_name, db_pool, schema)

    # Cache result
    _detected_sources_cache[cache_key] = source

    return source
```

**Cache Benefits:**
- ✅ Database queried once per type (not on every request)
- ✅ Startup time: ~100ms for 50 types (with detection)
- ✅ Runtime: 0ms (cached)
- ✅ Cache can be warmed up at application startup

### Prefix Options

Support different database object types via configuration:

```python
# Default behavior (views)
@fraiseql.type  # → v_user
class User: ...

# Global configuration
fraiseql.config.default_source_prefix = "tv"  # Tables
fraiseql.config.default_source_prefix = "mv"  # Materialized views
fraiseql.config.default_source_prefix = "v"   # Views (default)

# Per-type override via prefix parameter
@fraiseql.type(source_prefix="tv")  # → tv_user (table)
class User: ...

@fraiseql.type(source_prefix="mv")  # → mv_analytics (materialized view)
class Analytics: ...

@fraiseql.type(source_prefix="v")   # → v_customer (view, explicit)
class Customer: ...
```

### Edge Cases and Special Handling

#### 1. Acronyms and Abbreviations

```python
class APIKey: ...       # → v_api_key (not v_a_p_i_key)
class HTTPRequest: ...  # → v_http_request
class URLShortener: ... # → v_url_shortener
```

**Solution**: Use smart acronym detection in `camel_to_snake()`:
- Consecutive uppercase letters are treated as acronym
- Last letter of acronym stays with following word
- Examples: `APIKey` → `api_key`, `HTTPSConnection` → `https_connection`

#### 2. Multi-Word Names

```python
class UserProfile: ...      # → v_user_profile ✅
class OrderLineItem: ...    # → v_order_line_item ✅
class PaymentMethod: ...    # → v_payment_method ✅
```

**Solution**: Standard PascalCase to snake_case conversion handles this.

#### 3. Schema-Qualified Names

```python
@fraiseql.type(schema="analytics")  # → analytics.v_user
class User: ...

@fraiseql.type(schema="public")     # → public.v_customer
class Customer: ...
```

**Solution**: Add optional `schema` parameter that prefixes the inferred source.

#### 4. Plural vs Singular

**Convention**: FraiseQL uses **singular** names for types and sources:
- Type: `User` (singular)
- Source: `v_user` (singular)

**Not supported**: Automatic pluralization (`User` → `v_users`)
- Reason: Ambiguous (User → Users? Useres? Useri?)
- Reason: FraiseQL convention is singular
- Solution: Users wanting plural must specify explicitly

```python
# If you really want plural (non-standard)
@fraiseql.type(sql_source="v_users")  # ← Explicit override
class User: ...
```

## Implementation Plan

### Phase 1: Core Auto-Mapping (2 hours)

**File**: `src/fraiseql/utils/naming.py` (NEW or update existing)

```python
"""Naming convention utilities for FraiseQL."""

import re


def camel_to_snake(name: str) -> str:
    """Convert PascalCase/camelCase to snake_case.

    Handles acronyms intelligently:
    - APIKey → api_key
    - HTTPRequest → http_request
    - UserHTTPProfile → user_http_profile

    Args:
        name: PascalCase or camelCase string

    Returns:
        snake_case string
    """
    # Insert underscore before uppercase letters (not at start)
    s1 = re.sub('(.)([A-Z][a-z]+)', r'\1_\2', name)
    # Insert underscore before uppercase in sequences
    s2 = re.sub('([a-z0-9])([A-Z])', r'\1_\2', s1)

    return s2.lower()


def infer_sql_source(
    type_name: str,
    prefix: str = "v",
    schema: str | None = None,
) -> str:
    """Infer sql_source from type name.

    Args:
        type_name: Python class name (e.g., "User")
        prefix: Database object prefix ("v", "tv", "mv")
        schema: Optional schema name ("public", "analytics", etc.)

    Returns:
        Inferred sql_source

    Examples:
        ("User", "v") → "v_user"
        ("UserProfile", "tv") → "tv_user_profile"
        ("Analytics", "mv", "reporting") → "reporting.mv_analytics"
    """
    snake_name = camel_to_snake(type_name)
    source = f"{prefix}_{snake_name}"

    if schema:
        source = f"{schema}.{source}"

    return source
```

**Tests**: `tests/unit/utils/test_naming.py`

```python
"""Tests for naming convention utilities."""

import pytest
from fraiseql.utils.naming import camel_to_snake, infer_sql_source


class TestCamelToSnake:
    """Test PascalCase to snake_case conversion."""

    def test_simple_names(self):
        assert camel_to_snake("User") == "user"
        assert camel_to_snake("Customer") == "customer"
        assert camel_to_snake("Order") == "order"

    def test_multi_word_names(self):
        assert camel_to_snake("UserProfile") == "user_profile"
        assert camel_to_snake("OrderLineItem") == "order_line_item"
        assert camel_to_snake("PaymentMethod") == "payment_method"

    def test_acronyms(self):
        assert camel_to_snake("APIKey") == "api_key"
        assert camel_to_snake("HTTPRequest") == "http_request"
        assert camel_to_snake("URLShortener") == "url_shortener"

    def test_mixed_acronyms(self):
        assert camel_to_snake("UserAPIKey") == "user_api_key"
        assert camel_to_snake("HTTPSConnection") == "https_connection"


class TestInferSQLSource:
    """Test sql_source inference."""

    def test_default_view_prefix(self):
        assert infer_sql_source("User") == "v_user"
        assert infer_sql_source("Customer") == "v_customer"
        assert infer_sql_source("Order") == "v_order"

    def test_custom_prefix(self):
        assert infer_sql_source("User", prefix="tv") == "tv_user"
        assert infer_sql_source("Analytics", prefix="mv") == "mv_analytics"

    def test_with_schema(self):
        assert infer_sql_source("User", schema="public") == "public.v_user"
        assert infer_sql_source("Analytics", prefix="mv", schema="reporting") == "reporting.mv_analytics"

    def test_complex_names(self):
        assert infer_sql_source("UserProfile") == "v_user_profile"
        assert infer_sql_source("APIKey") == "v_api_key"
```

### Phase 2: Integrate with @fraise_type (1 hour)

**File**: `src/fraiseql/types/fraise_type.py`

**Modification**:

```python
def fraise_type(
    _cls: T | None = None,
    *,
    sql_source: str | None = None,
    source_prefix: str = "v",  # NEW: prefix for auto-mapping
    schema: str | None = None,  # NEW: optional schema
    jsonb_column: str | None = ...,
    implements: list[type] | None = None,
    resolve_nested: bool = False,
    auto_generate: bool = True,
) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL GraphQL output type.

    Args:
        sql_source: Explicit SQL source. If None, auto-infers from type name.
        source_prefix: Prefix for auto-mapped source ("v", "tv", "mv").
            Default: "v" (views). Only used if sql_source is None.
        schema: Optional schema qualifier for auto-mapped source.
            Only used if sql_source is None.

    Examples:
        # Auto-mapped (default to v_user)
        @fraise_type
        class User: ...

        # Auto-mapped with table prefix (tv_user)
        @fraise_type(source_prefix="tv")
        class User: ...

        # Auto-mapped with schema (analytics.mv_user)
        @fraise_type(source_prefix="mv", schema="analytics")
        class User: ...

        # Explicit override (no auto-mapping)
        @fraise_type(sql_source="legacy_users")
        class User: ...
    """

    def wrapper(cls: T) -> T:
        from fraiseql.utils.fields import patch_missing_field_types
        from fraiseql.utils.naming import infer_sql_source

        logger.debug("Decorating class %s at %s", cls.__name__, id(cls))

        # Patch types *before* definition is frozen
        patch_missing_field_types(cls)

        # Determine sql_source (auto-map if not provided)
        actual_sql_source = sql_source
        if actual_sql_source is None:
            # Auto-map from type name
            actual_sql_source = infer_sql_source(
                cls.__name__,
                prefix=source_prefix,
                schema=schema,
            )
            logger.debug(
                "Auto-mapped sql_source for %s: %s (prefix=%s, schema=%s)",
                cls.__name__,
                actual_sql_source,
                source_prefix,
                schema,
            )

        # Infer kind: treat types without any SQL source as pure types
        inferred_kind = "type" if actual_sql_source is None else "output"
        cls = define_fraiseql_type(cls, kind=inferred_kind)

        if actual_sql_source:
            cls.__gql_table__ = actual_sql_source
            cls.__fraiseql_definition__.sql_source = actual_sql_source
            # ... rest of existing code ...

        return cls

    return wrapper if _cls is None else wrapper(_cls)
```

**Tests**: `tests/unit/types/test_fraise_type_auto_mapping.py` (NEW)

```python
"""Tests for sql_source auto-mapping in @fraise_type."""

import pytest
from dataclasses import dataclass
import fraiseql


def test_auto_map_default_view_prefix():
    """Test that types auto-map to v_* by default."""
    @fraiseql.type
    @dataclass
    class User:
        id: int

    assert User.__gql_table__ == "v_user"


def test_auto_map_table_prefix():
    """Test auto-mapping with table prefix."""
    @fraiseql.type(source_prefix="tv")
    @dataclass
    class Customer:
        id: int

    assert Customer.__gql_table__ == "tv_customer"


def test_auto_map_materialized_view():
    """Test auto-mapping with materialized view prefix."""
    @fraiseql.type(source_prefix="mv")
    @dataclass
    class Analytics:
        id: int

    assert Analytics.__gql_table__ == "mv_analytics"


def test_auto_map_with_schema():
    """Test auto-mapping with schema qualifier."""
    @fraiseql.type(schema="analytics")
    @dataclass
    class Report:
        id: int

    assert Report.__gql_table__ == "analytics.v_report"


def test_auto_map_complex_names():
    """Test auto-mapping handles multi-word names."""
    @fraiseql.type
    @dataclass
    class UserProfile:
        id: int

    assert UserProfile.__gql_table__ == "v_user_profile"


def test_explicit_sql_source_overrides():
    """Test that explicit sql_source overrides auto-mapping."""
    @fraiseql.type(sql_source="legacy_users_table")
    @dataclass
    class User:
        id: int

    assert User.__gql_table__ == "legacy_users_table"


def test_no_sql_source_remains_pure_type():
    """Test that types without sql_source remain pure types."""
    # This requires explicit way to disable auto-mapping
    # Option 1: Use a sentinel value
    @fraiseql.type(sql_source=False)  # False = disable auto-mapping
    @dataclass
    class PureType:
        id: int

    assert not hasattr(PureType, '__gql_table__')
```

### Phase 3: Configuration and Global Settings (1 hour)

**File**: `src/fraiseql/config/schema_config.py`

**Add configuration options**:

```python
@dataclass
class SchemaConfig:
    # ... existing fields ...

    # Auto-mapping configuration
    default_source_prefix: str = "v"  # Default: views
    auto_map_sql_source: bool = True  # Enable/disable globally

    @classmethod
    def set_default_source_prefix(cls, prefix: str) -> None:
        """Set global default prefix for sql_source auto-mapping.

        Args:
            prefix: "v" (views), "tv" (tables), "mv" (materialized views)

        Example:
            fraiseql.config.set_default_source_prefix("tv")
        """
        instance = cls.get_instance()
        instance.default_source_prefix = prefix

    @classmethod
    def disable_auto_mapping(cls) -> None:
        """Disable sql_source auto-mapping globally."""
        instance = cls.get_instance()
        instance.auto_map_sql_source = False
```

**Usage**:

```python
import fraiseql

# Configure globally for entire application
fraiseql.config.set_default_source_prefix("tv")  # Use tables by default

@fraiseql.type  # → tv_user (uses global config)
class User: ...

@fraiseql.type(source_prefix="v")  # → v_customer (override global)
class Customer: ...
```

### Phase 4: Documentation (1 hour)

**File**: `docs/features/auto-mapping.md` (NEW)

Complete documentation with:
- Auto-mapping rules and examples
- Configuration options
- Override patterns
- Edge cases and troubleshooting
- Migration guide from explicit `sql_source`

## Benefits

### Developer Experience

**Before** (explicit):
```python
@fraiseql.type(sql_source="v_user")
@fraiseql.type(sql_source="v_customer")
@fraiseql.type(sql_source="v_order")
@fraiseql.type(sql_source="v_product")
@fraiseql.type(sql_source="v_payment_method")
# ... repeat for 50+ types
```

**After** (auto-mapped):
```python
@fraiseql.type
class User: ...

@fraiseql.type
class Customer: ...

@fraiseql.type
class Order: ...

@fraiseql.type
class Product: ...

@fraiseql.type
class PaymentMethod: ...  # → v_payment_method
```

**Savings**: 50+ types × 25 characters = **1,250+ characters of boilerplate eliminated**

### Consistency

- ✅ Enforces naming conventions automatically
- ✅ Prevents typos in `sql_source` specification
- ✅ Type name and source name always match
- ✅ Easier to refactor (rename type → source updates automatically)

### Flexibility

- ✅ Works with views, tables, materialized views
- ✅ Supports schema-qualified names
- ✅ Explicit override always available
- ✅ Global configuration for project-wide defaults

## Backward Compatibility

**Fully backward compatible**:
- Explicit `sql_source` still works (takes precedence)
- Existing code continues to function
- Opt-in via omitting `sql_source` parameter
- Can be disabled globally if needed

**Migration path**:
```python
# Phase 1: Keep explicit (no change)
@fraiseql.type(sql_source="v_user")
class User: ...

# Phase 2: Remove explicit sql_source (auto-map)
@fraiseql.type  # Now auto-maps to v_user
class User: ...
```

## Alternatives Considered

### Alternative 1: Always Auto-Map (No Override)

**Rejected**: Too restrictive - users need ability to use non-standard names

### Alternative 2: Convention-Based Registry

```python
fraiseql.register_naming_convention({
    "User": "legacy_users",
    "Customer": "crm_customers",
})
```

**Rejected**: More complex than simple parameter override

### Alternative 3: Decorator Chaining

```python
@fraiseql.type
@fraiseql.source_prefix("tv")
class User: ...
```

**Rejected**: More verbose than single parameter

## Risks and Mitigation

### Risk: Unexpected Source Names

**Risk**: User doesn't realize auto-mapping is happening, expects different source

**Mitigation**:
- Clear documentation
- Debug logging shows inferred source
- Explicit override always available

### Risk: Breaking Changes if Algorithm Changes

**Risk**: Updating `camel_to_snake()` could change inferred names

**Mitigation**:
- Algorithm should be stable (standard pattern)
- Any changes would be major version
- Explicit `sql_source` immune to algorithm changes

### Risk: Confusion About Singular vs Plural

**Risk**: Users might expect `User` → `v_users` (plural)

**Mitigation**:
- Clear documentation stating singular convention
- Error message if plural table doesn't exist
- Easy to override if needed

## Success Criteria

### Functional Requirements
- ✅ Auto-maps type name to sql_source using convention
- ✅ Supports view (`v_`), table (`tv_`), materialized view (`mv_`) prefixes
- ✅ Supports schema-qualified names
- ✅ Explicit `sql_source` overrides auto-mapping
- ✅ Global configuration for default prefix
- ✅ Handles multi-word names (PascalCase → snake_case)
- ✅ Handles acronyms intelligently

### Non-Functional Requirements
- ✅ Zero performance impact (inference is simple string operation)
- ✅ Backward compatible (100% existing tests pass)
- ✅ Clear error messages when inferred source doesn't exist

### Documentation Requirements
- ✅ Complete user guide with examples
- ✅ Configuration documentation
- ✅ Migration guide
- ✅ Troubleshooting section

## Timeline

**Total Estimated Time**: 5 hours

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| Phase 1 | 2 hours | Core naming utilities + tests |
| Phase 2 | 1 hour | Integration with @fraise_type |
| Phase 3 | 1 hour | Configuration and global settings |
| Phase 4 | 1 hour | Documentation |
| **Total** | **5 hours** | **Complete auto-mapping feature** |

## Related Issues

- Related to #121 (auto-generate WhereInput/OrderBy)
- Both features reduce boilerplate and enforce conventions
- Complementary: auto-mapping + auto-generation = minimal boilerplate

## Conclusion

Auto-mapping `sql_source` from type names:
- ✅ Eliminates significant boilerplate
- ✅ Enforces FraiseQL naming conventions
- ✅ Reduces errors from typos
- ✅ Simplifies refactoring
- ✅ Fully backward compatible
- ✅ Easy to implement (5 hours)

**Recommended**: Implement in FraiseQL 1.4.0 alongside auto-generation (#121)

Together, these features will dramatically improve developer experience! 🚀
