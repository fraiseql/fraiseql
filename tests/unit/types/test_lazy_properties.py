"""Tests for lazy property auto-generation."""

from dataclasses import dataclass
from uuid import UUID

import fraiseql
from fraiseql.types.lazy_properties import (
    clear_auto_generated_cache,
)


def test_lazy_where_input_property_caching() -> None:
    """Test that WhereInput is cached after first access."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="test_table")
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


def test_lazy_order_by_property_caching() -> None:
    """Test that OrderBy is cached after first access."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="test_table")
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


def test_multiple_types_independent_caching() -> None:
    """Test that different types have independent caches."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="table_a")
    @dataclass
    class TypeA:
        id: int

    @fraiseql.type(sql_source="table_b")
    @dataclass
    class TypeB:
        id: int

    where_a = TypeA.WhereInput
    where_b = TypeB.WhereInput

    assert where_a is not where_b
    assert "TypeAWhereInput" in where_a.__name__
    assert "TypeBWhereInput" in where_b.__name__


def test_fraise_type_has_where_input_property() -> None:
    """Test that @fraise_type adds WhereInput property."""

    @fraiseql.type(sql_source="test_table")
    @dataclass
    class TestType:
        id: int
        name: str

    assert hasattr(TestType, "WhereInput")
    assert hasattr(TestType, "OrderBy")


def test_where_input_is_lazy() -> None:
    """Test that WhereInput is not generated until accessed."""
    from fraiseql.types.lazy_properties import _auto_generated_cache

    clear_auto_generated_cache()

    @fraiseql.type(sql_source="test_table")
    @dataclass
    class TestType:
        id: int
        name: str

    # Cache should be empty (not generated yet)
    cache_key = f"{TestType.__module__}.{TestType.__name__}_WhereInput"
    assert cache_key not in _auto_generated_cache

    # Access WhereInput - now it should be generated
    where_input = TestType.WhereInput
    assert cache_key in _auto_generated_cache
    assert where_input is not None


def test_types_without_sql_source_no_auto_generation() -> None:
    """Test that pure types (no sql_source) don't get auto-generation."""

    @fraiseql.type
    @dataclass
    class PureType:
        id: int
        name: str

    # Pure types shouldn't have WhereInput/OrderBy
    # (they're not queryable, so filters don't make sense)
    assert not hasattr(PureType, "WhereInput")
    assert not hasattr(PureType, "OrderBy")


def test_generated_where_input_has_expected_fields() -> None:
    """Test that generated WhereInput has fields from the original type."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="users")
    @dataclass
    class User:
        id: UUID
        name: str
        email: str
        age: int

    WhereInput = User.WhereInput

    # Check that the WhereInput has annotations for our fields
    assert hasattr(WhereInput, "__annotations__")
    annotations = WhereInput.__annotations__

    # Should have our fields
    assert "id" in annotations
    assert "name" in annotations
    assert "email" in annotations
    assert "age" in annotations

    # Should have logical operators
    assert "OR" in annotations
    assert "AND" in annotations
    assert "NOT" in annotations


def test_generated_order_by_has_expected_structure() -> None:
    """Test that generated OrderBy has expected structure."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="products")
    @dataclass
    class Product:
        id: UUID
        name: str
        price: float

    OrderBy = Product.OrderBy

    # Check that the OrderBy type was created
    assert OrderBy is not None
    assert hasattr(OrderBy, "__annotations__")
