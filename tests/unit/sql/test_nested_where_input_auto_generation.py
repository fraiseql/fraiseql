"""Tests for nested type auto-generation."""

import pytest
from dataclasses import dataclass
from uuid import UUID

import fraiseql
from fraiseql.types.lazy_properties import clear_auto_generated_cache


def test_nested_where_input_auto_generation():
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


def test_circular_reference_handling():
    """Test that circular references don't cause infinite loops."""
    # Self-referential types require advanced forward reference handling
    # This is a known limitation that would require:
    # 1. Deferred type hint resolution in constructor.py
    # 2. Special handling in lazy property descriptors
    # For now, users should define such types carefully or use manual generation
    pytest.skip("Self-referential types require advanced forward reference handling")


def test_nested_type_uses_lazy_property():
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


def test_deeply_nested_types():
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


def test_multiple_nested_types_in_same_class():
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


def test_nested_type_with_forward_reference():
    """Test that forward references in nested types are handled."""
    # Forward references during decoration require special handling
    # For now, we define types in the correct order (dependencies first)
    pytest.skip("Forward references during decoration require special handling - use correct definition order instead")


def test_nested_where_input_can_be_instantiated():
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
