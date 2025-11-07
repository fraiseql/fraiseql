# Implementation Plan: Issue #121 - Auto-Generate WhereInput and OrderBy Types

**Issue**: https://github.com/fraiseql/fraiseql/issues/121
**Release**: FraiseQL 1.4.0 (Future Release)
**Type**: Feature Enhancement

## Executive Summary

Automatically generate WhereInput and OrderBy types when types are registered with `@fraise_type`, eliminating the need for manual boilerplate code. This will reduce 100+ lines of repetitive code to zero and prevent errors from forgotten companion types.

## Problem Statement

### Current Workflow (Manual)

Users must manually create WhereInput and OrderBy types for every single type:

```python
# filters.py - Must manually create for EVERY type
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.sql import create_graphql_order_by_input

# Manual creation (50+ types = 100+ lines of boilerplate)
OrganizationWhereInput = create_graphql_where_input(Organization)
AllocationWhereInput = create_graphql_where_input(Allocation)
MachineWhereInput = create_graphql_where_input(Machine)
LocationWhereInput = create_graphql_where_input(Location)
ContractWhereInput = create_graphql_where_input(Contract)
# ... repeat 50+ times

OrganizationOrderByInput = create_graphql_order_by_input(Organization)
AllocationOrderByInput = create_graphql_order_by_input(Allocation)
# ... repeat 50+ times
```

**Problems:**
- ❌ High maintenance burden - must remember to add for every new type
- ❌ ~100+ lines of boilerplate code
- ❌ Easy to forget when adding new types
- ❌ No type safety - typos in names won't be caught
- ❌ Duplication - same pattern repeated dozens of times

### Desired Workflow (Auto-Generated)

Types should automatically have WhereInput and OrderBy available:

```python
@fraiseql.type
class Machine:
    id: UUID
    identifier: str
    contract_id: UUID | None
    contract: Contract | None

# Automatically available:
# - Machine.WhereInput (lazy-loaded)
# - Machine.OrderBy (lazy-loaded)

@fraiseql.query
async def machines(
    where: Machine.WhereInput | None = None,  # ✅ Auto-available
    order_by: Machine.OrderBy | None = None,   # ✅ Auto-available
) -> list[Machine]:
    return await db.find("tv_machine", where=where, order_by=order_by)
```

**Benefits:**
- ✅ Zero boilerplate
- ✅ Can't forget to create companion types
- ✅ Clean, intuitive API
- ✅ Type-safe (IDE autocomplete)

## Solution Architecture

### Approach: Lazy Property Injection

Add lazy-loaded properties to types decorated with `@fraise_type`:

```python
class TypeMeta:
    """Metaclass or property descriptor for lazy WhereInput/OrderBy generation."""

    @property
    def WhereInput(cls):
        """Lazy-generate and cache WhereInput type."""
        cache_key = f"{cls.__module__}.{cls.__name__}_WhereInput"
        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_where_generator import create_graphql_where_input
            _auto_generated_cache[cache_key] = create_graphql_where_input(cls)
        return _auto_generated_cache[cache_key]

    @property
    def OrderBy(cls):
        """Lazy-generate and cache OrderBy type."""
        cache_key = f"{cls.__module__}.{cls.__name__}_OrderBy"
        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_order_by_generator import create_graphql_order_by_input
            _auto_generated_cache[cache_key] = create_graphql_order_by_input(cls)
        return _auto_generated_cache[cache_key]
```

### Why Lazy Loading?

**Benefits:**
- ✅ Only generates when actually used
- ✅ Avoids circular dependency issues
- ✅ No startup performance penalty
- ✅ Clean property access: `Machine.WhereInput`
- ✅ Caching prevents redundant generation

**Alternative Considered (Eager Generation):**
- ❌ Could cause circular dependencies with nested types
- ❌ Generates types that might never be used
- ❌ Increases startup time
- ❌ More complex to handle forward references

## Implementation Phases

### Phase 1: Core Infrastructure (2-3 hours)

**Objective**: Add lazy property mechanism to `@fraise_type` decorator

#### Task 1.1: Create Lazy Property Descriptors

**File**: `src/fraiseql/types/lazy_properties.py` (NEW)

```python
"""Lazy property descriptors for auto-generating WhereInput and OrderBy types."""

from typing import Any, TypeVar

T = TypeVar("T")

# Global cache for auto-generated types
_auto_generated_cache: dict[str, type] = {}


class LazyWhereInputProperty:
    """Descriptor for lazy WhereInput type generation."""

    def __get__(self, obj: Any, objtype: type[T]) -> type:
        """Generate and cache WhereInput type on first access."""
        if obj is not None:
            # Called on instance, not class - return bound descriptor
            return self

        # Called on class - generate WhereInput
        cache_key = f"{objtype.__module__}.{objtype.__name__}_WhereInput"

        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_where_generator import create_graphql_where_input

            try:
                _auto_generated_cache[cache_key] = create_graphql_where_input(objtype)
            except Exception as e:
                msg = f"Failed to auto-generate WhereInput for {objtype.__name__}: {e}"
                raise RuntimeError(msg) from e

        return _auto_generated_cache[cache_key]


class LazyOrderByProperty:
    """Descriptor for lazy OrderBy type generation."""

    def __get__(self, obj: Any, objtype: type[T]) -> type:
        """Generate and cache OrderBy type on first access."""
        if obj is not None:
            return self

        cache_key = f"{objtype.__module__}.{objtype.__name__}_OrderBy"

        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_order_by_generator import create_graphql_order_by_input

            try:
                _auto_generated_cache[cache_key] = create_graphql_order_by_input(objtype)
            except Exception as e:
                msg = f"Failed to auto-generate OrderBy for {objtype.__name__}: {e}"
                raise RuntimeError(msg) from e

        return _auto_generated_cache[cache_key]


def clear_auto_generated_cache() -> None:
    """Clear the auto-generated type cache (useful for testing)."""
    _auto_generated_cache.clear()
```

**Tests**: `tests/unit/types/test_lazy_properties.py` (NEW)

```python
"""Tests for lazy property auto-generation."""

import pytest
from fraiseql.types.lazy_properties import (
    LazyWhereInputProperty,
    LazyOrderByProperty,
    clear_auto_generated_cache,
)


def test_lazy_where_input_property_caching():
    """Test that WhereInput is cached after first access."""
    from dataclasses import dataclass
    import fraiseql

    clear_auto_generated_cache()

    @fraiseql.type
    @dataclass
    class TestType:
        id: int
        name: str

    # First access should generate
    where_input_1 = TestType.WhereInput
    assert where_input_1 is not None
    assert "WhereInput" in where_input_1.__name__

    # Second access should return cached version
    where_input_2 = TestType.WhereInput
    assert where_input_1 is where_input_2  # Same object (cached)


def test_lazy_order_by_property_caching():
    """Test that OrderBy is cached after first access."""
    from dataclasses import dataclass
    import fraiseql

    clear_auto_generated_cache()

    @fraiseql.type
    @dataclass
    class TestType:
        id: int
        name: str

    # First access should generate
    order_by_1 = TestType.OrderBy
    assert order_by_1 is not None

    # Second access should return cached version
    order_by_2 = TestType.OrderBy
    assert order_by_1 is order_by_2


def test_multiple_types_independent_caching():
    """Test that different types have independent caches."""
    from dataclasses import dataclass
    import fraiseql

    clear_auto_generated_cache()

    @fraiseql.type
    @dataclass
    class TypeA:
        id: int

    @fraiseql.type
    @dataclass
    class TypeB:
        id: int

    where_a = TypeA.WhereInput
    where_b = TypeB.WhereInput

    assert where_a is not where_b
    assert "TypeAWhereInput" in where_a.__name__
    assert "TypeBWhereInput" in where_b.__name__
```

#### Task 1.2: Integrate into `@fraise_type` Decorator

**File**: `src/fraiseql/types/fraise_type.py`

**Modification** (lines ~168-178):

```python
def wrapper(cls: T) -> T:
    from fraiseql.utils.fields import patch_missing_field_types

    logger.debug("Decorating class %s at %s", cls.__name__, id(cls))

    # Patch types *before* definition is frozen
    patch_missing_field_types(cls)

    # Infer kind: treat no SQL source as a pure type
    inferred_kind = "type" if sql_source is None else "output"
    cls = define_fraiseql_type(cls, kind=inferred_kind)

    if sql_source:
        cls.__gql_table__ = sql_source
        cls.__fraiseql_definition__.sql_source = sql_source
        # ... existing code ...
        cls.__gql_where_type__ = safe_create_where_type(cls)

        # NEW: Add lazy properties for auto-generation
        from fraiseql.types.lazy_properties import (
            LazyWhereInputProperty,
            LazyOrderByProperty,
        )

        # Only add for output types with SQL sources
        if not hasattr(cls, 'WhereInput'):
            cls.WhereInput = LazyWhereInputProperty()
        if not hasattr(cls, 'OrderBy'):
            cls.OrderBy = LazyOrderByProperty()

    # ... rest of existing code ...

    return cls
```

**Tests**: `tests/unit/types/test_fraise_type_auto_generation.py` (NEW)

```python
"""Tests for auto-generation integration in @fraise_type."""

import pytest
from dataclasses import dataclass
import fraiseql


def test_fraise_type_has_where_input_property():
    """Test that @fraise_type adds WhereInput property."""
    @fraiseql.type(sql_source="test_table")
    @dataclass
    class TestType:
        id: int
        name: str

    assert hasattr(TestType, 'WhereInput')
    assert hasattr(TestType, 'OrderBy')


def test_where_input_is_lazy():
    """Test that WhereInput is not generated until accessed."""
    from fraiseql.types.lazy_properties import clear_auto_generated_cache

    clear_auto_generated_cache()

    @fraiseql.type(sql_source="test_table")
    @dataclass
    class TestType:
        id: int
        name: str

    # Cache should be empty (not generated yet)
    from fraiseql.types.lazy_properties import _auto_generated_cache
    cache_key = f"{TestType.__module__}.{TestType.__name__}_WhereInput"
    assert cache_key not in _auto_generated_cache

    # Access WhereInput - now it should be generated
    where_input = TestType.WhereInput
    assert cache_key in _auto_generated_cache
    assert where_input is not None


def test_types_without_sql_source_no_auto_generation():
    """Test that pure types (no sql_source) don't get auto-generation."""
    @fraiseql.type
    @dataclass
    class PureType:
        id: int
        name: str

    # Pure types shouldn't have WhereInput/OrderBy
    # (they're not queryable, so filters don't make sense)
    assert not hasattr(PureType, 'WhereInput')
    assert not hasattr(PureType, 'OrderBy')
```

---

### Phase 2: Nested Type Handling (2-3 hours)

**Objective**: Handle circular dependencies and nested type references

#### Task 2.1: Add Deferred Loading for Nested Types

**Challenge**: When `Order` has `customer: Customer`, generating `OrderWhereInput` needs `CustomerWhereInput`, which might not exist yet.

**Solution**: Lazy properties naturally handle this - when `OrderWhereInput` is generated, it can access `Customer.WhereInput` which triggers generation.

**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Modification** (lines ~560-620 - nested field handling):

```python
# When processing nested fields in create_graphql_where_input()
for field_name, field_type in type_hints.items():
    # ... existing field processing ...

    # Handle nested FraiseQL types
    if hasattr(field_type, "__fraiseql_definition__"):
        # Check if type has auto-generated WhereInput property
        if hasattr(field_type, 'WhereInput'):
            # Use lazy property - it will generate on access
            try:
                nested_where_input = field_type.WhereInput
                field_definitions.append((field_name, Optional[nested_where_input], None))
                field_defaults[field_name] = None
                continue
            except Exception as e:
                logger.debug(
                    f"Failed to use auto-generated WhereInput for {field_name}: {e}. "
                    f"Will defer to avoid circular dependency."
                )
                # Fall through to existing deferred logic

        # Existing deferred field logic as fallback
        deferred_fields[field_name] = field_type
        # ... existing code ...
```

**Tests**: `tests/unit/sql/test_nested_where_input_auto_generation.py` (NEW)

```python
"""Tests for nested type auto-generation."""

import pytest
from dataclasses import dataclass
import fraiseql
from uuid import UUID


def test_nested_where_input_auto_generation():
    """Test that nested types automatically generate WhereInput."""
    @fraiseql.type(sql_source="v_customer")
    @dataclass
    class Customer:
        id: UUID
        name: str

    @fraiseql.type(sql_source="v_order")
    @dataclass
    class Order:
        id: UUID
        customer_id: UUID
        customer: Customer | None

    # Access should trigger nested generation
    order_where = Order.WhereInput

    # Should have nested customer field with CustomerWhereInput
    assert hasattr(order_where, '__annotations__')
    annotations = order_where.__annotations__

    # Check that customer field exists and uses CustomerWhereInput
    assert 'customer' in annotations
    # The type should reference CustomerWhereInput
    customer_type_str = str(annotations['customer'])
    assert 'CustomerWhereInput' in customer_type_str


def test_circular_reference_handling():
    """Test that circular references don't cause infinite loops."""
    @fraiseql.type(sql_source="v_user")
    @dataclass
    class User:
        id: UUID
        name: str
        manager_id: UUID | None
        manager: 'User | None' = None  # Self-reference

    # Should handle circular reference gracefully
    user_where = User.WhereInput
    assert user_where is not None

    # Manager field should use deferred type or same WhereInput class
    assert 'manager' in user_where.__annotations__
```

#### Task 2.2: Handle Forward References

**File**: `src/fraiseql/types/lazy_properties.py`

**Enhancement**:

```python
class LazyWhereInputProperty:
    """Descriptor for lazy WhereInput type generation."""

    def __get__(self, obj: Any, objtype: type[T]) -> type:
        """Generate and cache WhereInput type on first access."""
        if obj is not None:
            return self

        cache_key = f"{objtype.__module__}.{objtype.__name__}_WhereInput"

        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_where_generator import create_graphql_where_input

            try:
                # Resolve forward references before generation
                from typing import get_type_hints
                try:
                    # This resolves forward references like 'User' -> User class
                    get_type_hints(objtype)
                except Exception:
                    pass  # Forward refs might not be resolvable yet

                _auto_generated_cache[cache_key] = create_graphql_where_input(objtype)
            except Exception as e:
                msg = f"Failed to auto-generate WhereInput for {objtype.__name__}: {e}"
                raise RuntimeError(msg) from e

        return _auto_generated_cache[cache_key]
```

---

### Phase 3: Integration Testing (2-3 hours)

**Objective**: Ensure auto-generation works in real-world scenarios

#### Task 3.1: Real-World Integration Tests

**File**: `tests/integration/core/test_auto_generation_integration.py` (NEW)

```python
"""Integration tests for auto-generation with database operations."""

import pytest
from dataclasses import dataclass
import fraiseql
from uuid import UUID
from fraiseql.db import FraiseQLRepository


@pytest.mark.integration
class TestAutoGenerationIntegration:
    """Test auto-generation works with actual database queries."""

    async def test_auto_generated_where_input_in_query(self, db_pool):
        """Test that auto-generated WhereInput works in db.find()."""
        @fraiseql.type(sql_source="tv_customers_test")
        @dataclass
        class CustomerTest:
            id: UUID
            name: str
            email: str

        # Setup test data
        async with db_pool.connection() as conn:
            await conn.execute("""
                DROP TABLE IF EXISTS tv_customers_test;
                CREATE TABLE tv_customers_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name TEXT NOT NULL,
                    email TEXT,
                    data JSONB
                );
                INSERT INTO tv_customers_test (name, email, data)
                VALUES ('Alice', 'alice@test.com', '{"name": "Alice", "email": "alice@test.com"}');
            """)
            await conn.commit()

        db = FraiseQLRepository(db_pool)

        # Use auto-generated WhereInput
        WhereInput = CustomerTest.WhereInput
        where = WhereInput(name={"eq": "Alice"})

        # Should work with db.find()
        results = await db.find("tv_customers_test", where=where)
        assert results is not None

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP TABLE IF EXISTS tv_customers_test")
            await conn.commit()

    async def test_auto_generated_order_by_in_query(self, db_pool):
        """Test that auto-generated OrderBy works in db.find()."""
        @fraiseql.type(sql_source="tv_products_test")
        @dataclass
        class ProductTest:
            id: UUID
            name: str
            price: float

        # Setup test data
        async with db_pool.connection() as conn:
            await conn.execute("""
                DROP TABLE IF EXISTS tv_products_test;
                CREATE TABLE tv_products_test (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name TEXT NOT NULL,
                    price DECIMAL(10,2),
                    data JSONB
                );
                INSERT INTO tv_products_test (name, price, data)
                VALUES
                    ('Widget', 9.99, '{"name": "Widget", "price": 9.99}'),
                    ('Gadget', 19.99, '{"name": "Gadget", "price": 19.99}');
            """)
            await conn.commit()

        db = FraiseQLRepository(db_pool)

        # Use auto-generated OrderBy
        OrderBy = ProductTest.OrderBy
        order_by = [OrderBy(price="ASC")]

        # Should work with db.find()
        results = await db.find("tv_products_test", order_by=order_by)
        assert results is not None

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP TABLE IF EXISTS tv_products_test")
            await conn.commit()


@pytest.mark.integration
async def test_nested_auto_generation_with_fk_detection(db_pool):
    """Test that nested auto-generated WhereInput works with FK detection."""
    @fraiseql.type(sql_source="tv_customers_nested")
    @dataclass
    class CustomerNested:
        id: UUID
        name: str

    @fraiseql.type(sql_source="tv_orders_nested")
    @dataclass
    class OrderNested:
        id: UUID
        customer_id: UUID
        customer: CustomerNested | None

    # Setup test data
    async with db_pool.connection() as conn:
        await conn.execute("""
            DROP TABLE IF EXISTS tv_orders_nested CASCADE;
            DROP TABLE IF EXISTS tv_customers_nested CASCADE;

            CREATE TABLE tv_customers_nested (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                data JSONB
            );

            CREATE TABLE tv_orders_nested (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                customer_id UUID REFERENCES tv_customers_nested(id),
                order_number TEXT,
                data JSONB
            );

            INSERT INTO tv_customers_nested (name, data)
            VALUES ('Customer A', '{"name": "Customer A"}');
        """)

        result = await conn.execute("SELECT id FROM tv_customers_nested")
        customer_id = (await result.fetchone())[0]

        await conn.execute(
            "INSERT INTO tv_orders_nested (customer_id, data) VALUES (%s, %s)",
            (customer_id, '{"orderNumber": "ORD-001"}')
        )
        await conn.commit()

    db = FraiseQLRepository(db_pool)

    # Register FK metadata
    from fraiseql.db import register_type_for_view
    register_type_for_view(
        "tv_orders_nested",
        OrderNested,
        table_columns={"id", "customer_id", "order_number", "data"},
        has_jsonb_data=True
    )

    # Use auto-generated nested WhereInput
    OrderWhere = OrderNested.WhereInput
    where = OrderWhere(customer={"id": {"eq": str(customer_id)}})

    # Should use FK detection
    results = await db.find("tv_orders_nested", where=where)
    assert results is not None

    # Cleanup
    async with db_pool.connection() as conn:
        await conn.execute("DROP TABLE IF EXISTS tv_orders_nested CASCADE")
        await conn.execute("DROP TABLE IF EXISTS tv_customers_nested CASCADE")
        await conn.commit()
```

#### Task 3.2: GraphQL Schema Generation Tests

**File**: `tests/integration/graphql/test_auto_generated_schema.py` (NEW)

```python
"""Test that auto-generated types work in GraphQL schema."""

import pytest
from dataclasses import dataclass
import fraiseql
from uuid import UUID


@pytest.mark.integration
def test_auto_generated_types_in_schema():
    """Test that auto-generated WhereInput types appear in GraphQL schema."""
    @fraiseql.type(sql_source="v_product")
    @dataclass
    class Product:
        id: UUID
        name: str
        price: float

    @fraiseql.query
    async def products(
        where: Product.WhereInput | None = None,
        order_by: Product.OrderBy | None = None
    ) -> list[Product]:
        # Dummy implementation
        return []

    # Build schema
    schema = fraiseql.build_fraiseql_schema()

    # Check that ProductWhereInput exists in schema
    assert 'ProductWhereInput' in schema.type_map
    product_where = schema.type_map['ProductWhereInput']

    # Check fields exist
    assert 'name' in product_where.fields
    assert 'price' in product_where.fields
    assert 'id' in product_where.fields

    # Check logical operators exist
    assert 'OR' in product_where.fields
    assert 'AND' in product_where.fields
    assert 'NOT' in product_where.fields
```

---

### Phase 4: Documentation and Examples (1-2 hours)

**Objective**: Update documentation and provide examples

#### Task 4.1: Update User Documentation

**File**: `docs/reference/auto-generation.md` (NEW)

```markdown
# Auto-Generated Filter Types

FraiseQL automatically generates WhereInput and OrderBy types for all types decorated with `@fraise_type(sql_source="...")`.

## Basic Usage

```python
import fraiseql
from dataclasses import dataclass
from uuid import UUID

@fraiseql.type(sql_source="v_user")
@dataclass
class User:
    id: UUID
    name: str
    email: str

# WhereInput and OrderBy are automatically available
@fraiseql.query
async def users(
    info,
    where: User.WhereInput | None = None,  # ✅ Auto-generated
    order_by: User.OrderBy | None = None,  # ✅ Auto-generated
) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user", where=where, order_by=order_by)
```

## How It Works

### Lazy Generation

WhereInput and OrderBy types are generated **lazily** - only when first accessed:

```python
@fraiseql.type(sql_source="v_product")
@dataclass
class Product:
    id: UUID
    name: str

# Not generated yet - zero overhead
print("Type defined")

# Generated on first access
WhereInput = Product.WhereInput  # ← Generation happens here
print(f"Generated: {WhereInput.__name__}")
# Output: Generated: ProductWhereInput

# Subsequent access uses cached version (fast)
WhereInput2 = Product.WhereInput  # ← Uses cache
assert WhereInput is WhereInput2  # Same object
```

### Caching

Generated types are cached globally:
- First access: generates and caches
- Subsequent access: returns cached type
- No memory bloat: only generates what you use

### Nested Types

Nested types automatically generate nested WhereInput:

```python
@fraiseql.type(sql_source="v_customer")
@dataclass
class Customer:
    id: UUID
    name: str

@fraiseql.type(sql_source="v_order")
@dataclass
class Order:
    id: UUID
    customer_id: UUID
    customer: Customer | None  # ← Nested type

# OrderWhereInput includes CustomerWhereInput
OrderWhere = Order.WhereInput

# Can filter by nested customer
where = OrderWhere(customer={"name": {"contains": "Acme"}})
```

## Migration from Manual Generation

### Before (Manual)

```python
# Old approach - manual boilerplate
from fraiseql.sql.graphql_where_generator import create_graphql_where_input

UserWhereInput = create_graphql_where_input(User)
ProductWhereInput = create_graphql_where_input(Product)
OrderWhereInput = create_graphql_where_input(Order)
# ... repeat for 50+ types
```

### After (Auto-Generated)

```python
# New approach - zero boilerplate
# Just use Type.WhereInput directly

@fraiseql.query
async def users(where: User.WhereInput | None = None):
    # WhereInput auto-generated
    pass
```

### Backward Compatibility

Manual generation still works:

```python
# You can still manually create if needed
CustomWhereInput = create_graphql_where_input(User, name="CustomUserFilter")

@fraiseql.query
async def users(where: CustomWhereInput | None = None):
    pass
```

## Advanced Usage

### Type Annotations

Use `Type.WhereInput` in type hints for IDE autocomplete:

```python
from typing import Annotated

async def find_users(filters: User.WhereInput) -> list[User]:
    """Type checker understands User.WhereInput is a type."""
    db = get_db()
    return await db.find("v_user", where=filters)
```

### Conditional Filters

Build filters dynamically:

```python
@fraiseql.query
async def users(
    info,
    name: str | None = None,
    email: str | None = None
) -> list[User]:
    db = info.context["db"]

    # Build where clause dynamically
    WhereInput = User.WhereInput
    where_dict = {}

    if name:
        where_dict["name"] = {"contains": name}
    if email:
        where_dict["email"] = {"eq": email}

    where = WhereInput(**where_dict) if where_dict else None

    return await db.find("v_user", where=where)
```

## Types Without Auto-Generation

Pure types (no `sql_source`) don't get auto-generation:

```python
@fraiseql.type  # ← No sql_source
@dataclass
class Address:
    street: str
    city: str

# These don't exist (Address is not queryable):
# Address.WhereInput  # ← AttributeError
# Address.OrderBy     # ← AttributeError
```

This is intentional - only queryable types need filters.

## Performance

### Lazy Loading Benefits

- **Zero startup cost**: Types generated only when used
- **No circular dependency issues**: Lazy loading breaks cycles
- **Memory efficient**: Only caches what you access

### Generation Performance

- **Fast**: ~1-2ms per type (one-time cost)
- **Cached**: Subsequent access is instant
- **Scales well**: 100+ types = no performance impact

## Troubleshooting

### "Failed to auto-generate WhereInput"

**Cause**: Type has forward references or circular dependencies

**Solution**: Ensure all referenced types are defined before access:

```python
# Define types in correct order
@fraiseql.type(sql_source="v_customer")
class Customer: ...

@fraiseql.type(sql_source="v_order")
class Order:
    customer: Customer  # ← Customer must be defined first

# Now safe to access
Order.WhereInput  # ✅ Works
```

### "AttributeError: WhereInput"

**Cause**: Type doesn't have `sql_source`

**Solution**: Only types with `sql_source` get auto-generation:

```python
@fraiseql.type(sql_source="v_user")  # ← Add sql_source
class User: ...
```

## See Also

- [Filtering Documentation](./filtering.md)
- [WhereInput Types](./where-input.md)
- [OrderBy Types](./order-by.md)
```

#### Task 4.2: Update Examples

**File**: `examples/auto_generation_example.py` (NEW)

```python
"""Example demonstrating auto-generated WhereInput and OrderBy types."""

import asyncio
from dataclasses import dataclass
from uuid import UUID

import fraiseql
from fraiseql.db import FraiseQLRepository


@fraiseql.type(sql_source="tv_customers")
@dataclass
class Customer:
    """Customer type with auto-generated filters."""

    id: UUID
    name: str
    email: str


@fraiseql.type(sql_source="tv_orders")
@dataclass
class Order:
    """Order type with nested customer and auto-generated filters."""

    id: UUID
    order_number: str
    customer_id: UUID
    customer: Customer | None


# WhereInput types are automatically available - no manual creation needed!


@fraiseql.query
async def customers(
    info,
    where: Customer.WhereInput | None = None,  # ✅ Auto-generated
    order_by: Customer.OrderBy | None = None,  # ✅ Auto-generated
    limit: int = 10,
) -> list[Customer]:
    """Query customers with auto-generated filters."""
    db: FraiseQLRepository = info.context["db"]
    return await db.find("tv_customers", where=where, order_by=order_by, limit=limit)


@fraiseql.query
async def orders(
    info,
    where: Order.WhereInput | None = None,  # ✅ Auto-generated with nested Customer
    order_by: Order.OrderBy | None = None,  # ✅ Auto-generated
    limit: int = 10,
) -> list[Order]:
    """Query orders with auto-generated filters and nested customer filtering."""
    db: FraiseQLRepository = info.context["db"]
    return await db.find("tv_orders", where=where, order_by=order_by, limit=limit)


async def main():
    """Example usage."""
    # Build schema
    schema = fraiseql.build_fraiseql_schema()

    # Example query with auto-generated WhereInput
    query = """
        query {
            customers(where: { name: { contains: "Acme" } }) {
                id
                name
                email
            }
        }
    """

    print("Auto-generation example:")
    print(f"- Customer.WhereInput: {Customer.WhereInput}")
    print(f"- Customer.OrderBy: {Customer.OrderBy}")
    print(f"- Order.WhereInput: {Order.WhereInput}")
    print("\nNo manual type generation needed! ✨")


if __name__ == "__main__":
    asyncio.run(main())
```

#### Task 4.3: Update CHANGELOG for Future Release

**File**: `CHANGELOG.md`

**Add section** (to be uncommented when implemented):

```markdown
## [1.4.0] - TBD

### ✨ New Features

**Issue #121: Auto-Generate WhereInput and OrderBy Types**
- **Feature**: Automatic generation of filter types via lazy properties
- **Motivation**:
  - Users previously needed 100+ lines of boilerplate for manual type generation
  - Easy to forget when adding new types
  - No type safety for manual approach
- **Solution**: Added lazy `WhereInput` and `OrderBy` properties to all types with `sql_source`

#### Usage

Before (manual generation):
```python
UserWhereInput = create_graphql_where_input(User)
ProductWhereInput = create_graphql_where_input(Product)
# ... repeat 50+ times
```

After (auto-generation):
```python
@fraiseql.query
async def users(where: User.WhereInput | None = None):  # ✅ Auto-available
    pass
```

#### Key Features

- **Zero boilerplate**: No manual type generation needed
- **Lazy loading**: Types generated only when accessed (no startup penalty)
- **Caching**: Generated types cached globally for performance
- **Nested types**: Automatically handles nested WhereInput
- **Backward compatible**: Manual generation still works

### 📦 Changes

**Python Layer**
- `src/fraiseql/types/lazy_properties.py`: NEW lazy property descriptors
- `src/fraiseql/types/fraise_type.py`: Added WhereInput/OrderBy properties to decorator
- `src/fraiseql/sql/graphql_where_generator.py`: Enhanced for lazy property compatibility

**Tests**
- `tests/unit/types/test_lazy_properties.py`: NEW comprehensive lazy loading tests
- `tests/unit/types/test_fraise_type_auto_generation.py`: NEW decorator integration tests
- `tests/integration/core/test_auto_generation_integration.py`: NEW database integration tests
- `tests/integration/graphql/test_auto_generated_schema.py`: NEW schema generation tests

**Documentation**
- `docs/reference/auto-generation.md`: NEW comprehensive auto-generation guide
- `examples/auto_generation_example.py`: NEW usage examples

### 🔗 Related Issues
- Implements #121 - Auto-generate WhereInput and OrderBy types

### 🔄 Migration Notes
- **No breaking changes** - this is an additive feature
- Existing manual generation continues to work
- Recommended: Switch to auto-generation for cleaner code
```

---

### Phase 5: Opt-Out Mechanism (1 hour)

**Objective**: Allow users to disable auto-generation if needed

#### Task 5.1: Add `auto_generate` Parameter

**File**: `src/fraiseql/types/fraise_type.py`

**Modification**:

```python
@dataclass_transform(field_specifiers=(fraise_field, field, Field))
@overload
def fraise_type(
    _cls: None = None,
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = ...,
    implements: list[type] | None = None,
    resolve_nested: bool = False,
    auto_generate: bool = True,  # NEW parameter
) -> Callable[[T], T]: ...


def fraise_type(
    _cls: T | None = None,
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = ...,
    implements: list[type] | None = None,
    resolve_nested: bool = False,
    auto_generate: bool = True,  # NEW parameter
) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL GraphQL output type.

    Args:
        auto_generate: If True (default), automatically add WhereInput and OrderBy
            properties. Set to False to disable auto-generation for specific types.
    """

    def wrapper(cls: T) -> T:
        # ... existing code ...

        if sql_source:
            cls.__gql_table__ = sql_source
            # ... existing code ...

            # Only add auto-generation if enabled
            if auto_generate:
                from fraiseql.types.lazy_properties import (
                    LazyWhereInputProperty,
                    LazyOrderByProperty,
                )

                if not hasattr(cls, 'WhereInput'):
                    cls.WhereInput = LazyWhereInputProperty()
                if not hasattr(cls, 'OrderBy'):
                    cls.OrderBy = LazyOrderByProperty()

        return cls

    return wrapper if _cls is None else wrapper(_cls)
```

**Usage**:

```python
# Opt out of auto-generation for specific type
@fraiseql.type(sql_source="v_legacy", auto_generate=False)
@dataclass
class LegacyType:
    id: int

# Won't have WhereInput/OrderBy properties
# Must manually create if needed
```

---

## Testing Strategy

### Unit Tests (Phase 1-2)
- ✅ Lazy property descriptor behavior
- ✅ Caching mechanism
- ✅ Integration with `@fraise_type`
- ✅ Nested type handling
- ✅ Forward reference resolution

### Integration Tests (Phase 3)
- ✅ Database query operations
- ✅ GraphQL schema generation
- ✅ Nested filtering with FK detection
- ✅ OrderBy with actual queries

### Regression Tests
- ✅ Manual generation still works
- ✅ Existing code unaffected
- ✅ Performance benchmarks

## Risk Assessment

### Low Risk ✅
- **Backward compatible**: Existing code continues to work
- **Opt-in behavior**: Only affects types with `sql_source`
- **Lazy loading**: No startup performance impact
- **Well-tested patterns**: Descriptors and lazy loading are proven Python patterns

### Medium Risk ⚠️
- **Circular dependencies**: Handled via lazy loading
- **Caching complexity**: Global cache needs proper lifecycle management
- **Type hints**: IDE autocomplete might be confused (but type checkers work)

### Mitigation Strategies
1. Comprehensive test coverage (100+ tests)
2. Clear documentation with troubleshooting guide
3. Opt-out mechanism for edge cases
4. Gradual rollout (mark as beta in first release)

## Success Criteria

### Functional Requirements
- ✅ `Type.WhereInput` accessible on all types with `sql_source`
- ✅ `Type.OrderBy` accessible on all types with `sql_source`
- ✅ Lazy loading works correctly (no premature generation)
- ✅ Caching prevents redundant generation
- ✅ Nested types handled correctly
- ✅ Works with database queries
- ✅ Works in GraphQL schema generation

### Non-Functional Requirements
- ✅ Zero startup performance penalty
- ✅ < 2ms generation time per type
- ✅ No memory leaks from caching
- ✅ Backward compatible (100% existing tests pass)

### Documentation Requirements
- ✅ Comprehensive user guide
- ✅ Migration guide from manual approach
- ✅ Troubleshooting section
- ✅ Working examples

## Timeline

**Total Estimated Time**: 8-12 hours

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| Phase 1 | 2-3 hours | Lazy properties + decorator integration |
| Phase 2 | 2-3 hours | Nested types + forward references |
| Phase 3 | 2-3 hours | Integration tests |
| Phase 4 | 1-2 hours | Documentation + examples |
| Phase 5 | 1 hour | Opt-out mechanism |
| **Total** | **8-12 hours** | **Complete auto-generation feature** |

## Follow-up Work (Future Enhancements)

These are **out of scope** for initial implementation:

1. **Type stub generation** for better IDE support
   - Generate `.pyi` files with `WhereInput` properties
   - Improves autocomplete in IDEs

2. **Custom naming** for generated types
   - Allow `@fraise_type(where_input_name="CustomFilter")`

3. **Selective field inclusion/exclusion**
   - Allow `@fraise_type(where_input_exclude=["internal_field"])`

4. **Performance monitoring**
   - Add telemetry for generation times
   - Cache hit/miss metrics

## Conclusion

This phased plan provides a clear, disciplined approach to implementing auto-generation:

- **Phase 1**: Core infrastructure (lazy properties)
- **Phase 2**: Nested type handling
- **Phase 3**: Integration testing
- **Phase 4**: Documentation
- **Phase 5**: Opt-out mechanism

**Benefits**:
- ✅ Eliminates 100+ lines of boilerplate
- ✅ Prevents forgotten companion types
- ✅ Clean, intuitive API (`Type.WhereInput`)
- ✅ Zero startup performance penalty
- ✅ Backward compatible

**Risks**: Low - well-tested patterns, comprehensive testing, opt-out mechanism

Ready for implementation in FraiseQL 1.4.0! 🚀
