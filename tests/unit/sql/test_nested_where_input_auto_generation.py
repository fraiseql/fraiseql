"""Tests for nested type auto-generation."""

from dataclasses import dataclass
from uuid import UUID

import pytest

import fraiseql
from fraiseql.types.lazy_properties import clear_auto_generated_cache


def test_nested_where_input_auto_generation() -> None:
    """Test that nested types automatically generate WhereInput."""
    clear_auto_generated_cache()

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
    assert hasattr(order_where, "__annotations__")
    annotations = order_where.__annotations__

    # Check that customer field exists
    assert "customer" in annotations

    # The customer filter type should be CustomerWhereInput
    customer_annotation = annotations["customer"]
    # Extract the inner type from Optional[...]
    import typing

    if hasattr(typing, "get_args"):
        args = typing.get_args(customer_annotation)
        if args:
            customer_type = args[0]
            assert "CustomerWhereInput" in str(customer_type)


def test_circular_reference_handling() -> None:
    """Test that circular references (self-referential types) work correctly."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="v_category")
    @dataclass
    class Category:
        id: UUID
        name: str
        parent_id: UUID | None = None
        parent: "Category | None" = None  # Self-reference (string annotation)

    # This should NOT cause infinite recursion
    category_where = Category.WhereInput

    # Should have all fields including self-reference
    assert hasattr(category_where, "__annotations__")
    annotations = category_where.__annotations__

    # Basic fields
    assert "id" in annotations
    assert "name" in annotations
    assert "parent_id" in annotations

    # Self-referential field should be present
    assert "parent" in annotations

    # Should be instantiable
    filter_obj = category_where(name={"eq": "Electronics"})
    assert filter_obj is not None


def test_nested_type_uses_lazy_property() -> None:
    """Test that nested type detection uses lazy properties when available."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="v_department")
    @dataclass
    class Department:
        id: UUID
        name: str

    @fraiseql.type(sql_source="v_employee")
    @dataclass
    class Employee:
        id: UUID
        name: str
        department_id: UUID
        department: Department | None

    # Generate EmployeeWhereInput
    employee_where = Employee.WhereInput

    # This should have used Department.WhereInput via lazy property
    # Verify by checking that department field exists
    assert "department" in employee_where.__annotations__

    # Both should be accessible
    department_where = Department.WhereInput
    assert department_where is not None
    assert employee_where is not None


def test_deeply_nested_types() -> None:
    """Test that deeply nested types work correctly."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="v_country")
    @dataclass
    class Country:
        id: UUID
        name: str

    @fraiseql.type(sql_source="v_city")
    @dataclass
    class City:
        id: UUID
        name: str
        country_id: UUID
        country: Country | None

    @fraiseql.type(sql_source="v_address")
    @dataclass
    class Address:
        id: UUID
        street: str
        city_id: UUID
        city: City | None

    # Generate AddressWhereInput
    address_where = Address.WhereInput

    # Should have city field
    assert "city" in address_where.__annotations__

    # Generate CityWhereInput to verify it has country
    city_where = City.WhereInput
    assert "country" in city_where.__annotations__


def test_multiple_nested_types_in_same_class() -> None:
    """Test a class with multiple nested type references."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="v_customer")
    @dataclass
    class Customer:
        id: UUID
        name: str

    @fraiseql.type(sql_source="v_product")
    @dataclass
    class Product:
        id: UUID
        name: str
        price: float

    @fraiseql.type(sql_source="v_order")
    @dataclass
    class Order:
        id: UUID
        customer_id: UUID
        product_id: UUID
        customer: Customer | None
        product: Product | None

    # Generate OrderWhereInput
    order_where = Order.WhereInput

    # Should have both nested fields
    assert "customer" in order_where.__annotations__
    assert "product" in order_where.__annotations__


def test_nested_type_with_forward_reference() -> None:
    """Test that forward references to undefined types raise helpful errors."""
    clear_auto_generated_cache()

    # Test that forward references raise a helpful TypeError
    # (not a cryptic NameError)
    # Use string annotation to defer evaluation to decorator time
    with pytest.raises(TypeError) as exc_info:

        @fraiseql.type(sql_source="v_bad")
        @dataclass
        class BadType:
            id: UUID
            undefined_ref: "NonExistentType | None" = None  # noqa: F821

    # Error message should be helpful
    error_msg = str(exc_info.value)
    assert "NonExistentType" in error_msg
    assert "cannot be resolved" in error_msg or "not defined" in error_msg


def test_nested_where_input_can_be_instantiated() -> None:
    """Test that generated nested WhereInput can actually be used."""
    clear_auto_generated_cache()

    @fraiseql.type(sql_source="v_customer")
    @dataclass
    class Customer:
        id: UUID
        name: str
        email: str

    @fraiseql.type(sql_source="v_order")
    @dataclass
    class Order:
        id: UUID
        order_number: str
        customer_id: UUID
        customer: Customer | None

    # Get the WhereInput types
    OrderWhere = Order.WhereInput
    CustomerWhere = Customer.WhereInput

    # Should be able to instantiate them
    customer_filter = CustomerWhere(name={"eq": "John"})
    assert customer_filter is not None

    # Should be able to use nested filter
    # Note: The exact structure might vary based on implementation
    order_filter = OrderWhere(order_number={"eq": "ORD-001"})
    assert order_filter is not None
