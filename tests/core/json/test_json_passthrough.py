"""Tests for JSON passthrough optimization."""

from typing import List, Optional
from uuid import UUID

import pytest

import fraiseql
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.json_passthrough import JSONPassthrough, is_json_passthrough, wrap_in_passthrough



@pytest.mark.unit
@fraiseql.type
class Address:
    """Test address type."""

    street: str
    city: str
    postal_code: Optional[str] = None


@fraiseql.type
class Organization:
    """Test organization type."""

    id: UUID
    name: str
    address: Optional[Address] = None


@fraiseql.type
class User:
    """Test user type."""

    id: UUID
    name: str
    email: str
    organization: Optional[Organization] = None
    tags: List[str] = []


class TestJSONPassthrough:
    """Test JSON passthrough wrapper functionality."""

    def test_basic_passthrough(self):
        """Test basic attribute access through wrapper."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "email": "john@example.com",
        }

        wrapped = JSONPassthrough(data, "User", User)

        # Test attribute access
        assert wrapped.id == "550e8400-e29b-41d4-a716-446655440000"
        assert wrapped.name == "John Doe"
        assert wrapped.email == "john@example.com"

        # Test __typename injection
        assert wrapped.__typename == "User"
        assert data["__typename"] == "User"

    def test_camel_case_conversion(self):
        """Test automatic snake_case to camelCase conversion."""
        # Enable camelCase in config
        config = SchemaConfig.get_instance()
        original_setting = config.camel_case_fields
        config.camel_case_fields = True

        try:
            data = {"firstName": "John", "lastName": "Doe", "emailAddress": "john@example.com"}

            wrapped = JSONPassthrough(data, "User")

            # Should access camelCase fields with snake_case names
            assert wrapped.first_name == "John"
            assert wrapped.last_name == "Doe"
            assert wrapped.email_address == "john@example.com"

            # Should also work with original camelCase
            assert wrapped.firstName == "John"
            assert wrapped.lastName == "Doe"

        finally:
            config.camel_case_fields = original_setting

    def test_nested_object_wrapping(self):
        """Test lazy wrapping of nested objects."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "email": "john@example.com",
            "organization": {
                "id": "660e8400-e29b-41d4-a716-446655440001",
                "name": "Acme Corp",
                "address": {"street": "123 Main St", "city": "Anytown", "postalCode": "12345"},
            },
        }

        wrapped = JSONPassthrough(data, "User", User)

        # Access nested object - should be wrapped automatically
        org = wrapped.organization
        assert is_json_passthrough(org)
        assert org.name == "Acme Corp"

        # Access deeply nested object
        address = org.address
        assert is_json_passthrough(address)
        assert address.street == "123 Main St"
        assert address.city == "Anytown"

        # Test caching - should return same instance
        org2 = wrapped.organization
        assert org is org2

    def test_list_handling(self):
        """Test handling of lists, including lists of objects."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "tags": ["python", "graphql", "postgresql"],
            "addresses": [
                {"street": "123 Main St", "city": "City A"},
                {"street": "456 Oak Ave", "city": "City B"},
            ],
        }

        wrapped = JSONPassthrough(data, "User")

        # List of scalars should be returned directly
        assert wrapped.tags == ["python", "graphql", "postgresql"]
        assert not is_json_passthrough(wrapped.tags)

        # List of objects should be wrapped
        addresses = wrapped.addresses
        assert isinstance(addresses, list)
        assert len(addresses) == 2
        assert all(is_json_passthrough(addr) for addr in addresses)
        assert addresses[0].street == "123 Main St"
        assert addresses[1].city == "City B"

    def test_null_handling(self):
        """Test handling of null/None values."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "email": None,
            "organization": None,
        }

        wrapped = JSONPassthrough(data, "User", User)

        assert wrapped.email is None
        assert wrapped.organization is None

    def test_attribute_error(self):
        """Test helpful error messages for missing attributes."""
        data = {"id": "550e8400-e29b-41d4-a716-446655440000", "name": "John Doe"}

        wrapped = JSONPassthrough(data, "User")

        with pytest.raises(AttributeError) as exc_info:
            _ = wrapped.email

        error_msg = str(exc_info.value)
        assert "'User' object has no attribute 'email'" in error_msg
        assert "Available fields: ['id', 'name']" in error_msg

    def test_dict_like_methods(self):
        """Test dict-like methods for compatibility."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "email": "john@example.com",
        }

        wrapped = JSONPassthrough(data, "User")

        # Test get method
        assert wrapped.get("name") == "John Doe"
        assert wrapped.get("missing", "default") == "default"

        # Test contains
        assert "name" in wrapped
        assert "email" in wrapped
        assert "missing" not in wrapped

        # Test __dict__ property
        assert wrapped.__dict__ is data

    def test_wrap_in_passthrough_function(self):
        """Test the wrap_in_passthrough utility function."""
        # Test dict wrapping
        dict_data = {"id": "123", "name": "Test"}
        wrapped = wrap_in_passthrough(dict_data, User)
        assert is_json_passthrough(wrapped)
        assert wrapped._type_hint is User

        # Test list wrapping
        list_data = [{"id": "123", "name": "Test 1"}, {"id": "456", "name": "Test 2"}]
        wrapped_list = wrap_in_passthrough(list_data, List[User])
        assert isinstance(wrapped_list, list)
        assert all(is_json_passthrough(item) for item in wrapped_list)

        # Test scalar passthrough
        scalar = "test"
        assert wrap_in_passthrough(scalar) == "test"

    def test_type_name_extraction(self):
        """Test extraction of type names from various sources."""
        # Test with __typename in data
        data1 = {"__typename": "CustomType", "field": "value"}
        wrapped1 = JSONPassthrough(data1)
        assert wrapped1._type_name == "CustomType"

        # Test with type hint
        data2 = {"field": "value"}
        wrapped2 = JSONPassthrough(data2, type_hint=User)
        assert wrapped2._type_name == "User"

        # Test with explicit type name
        data3 = {"field": "value"}
        wrapped3 = JSONPassthrough(data3, "ExplicitType")
        assert wrapped3._type_name == "ExplicitType"

        # Test with __typename override
        data4 = {"__typename": "Override", "field": "value"}
        wrapped4 = JSONPassthrough(data4, "Original")
        assert wrapped4._type_name == "Override"

    def test_repr_and_str(self):
        """Test string representations for debugging."""
        data = {"id": "123", "name": "Test"}
        wrapped = JSONPassthrough(data, "User")

        assert repr(wrapped) == "JSONPassthrough(User, fields=['id', 'name'])"
        assert str(wrapped) == "User (passthrough)"


class TestJSONPassthroughPerformance:
    """Test performance characteristics of JSON passthrough."""

    def test_no_instantiation(self):
        """Verify that JSONPassthrough doesn't instantiate objects."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "organization": {"id": "660e8400-e29b-41d4-a716-446655440001", "name": "Acme Corp"},
        }

        # Track instantiation attempts
        original_new = User.__new__
        instantiation_count = 0

        def tracked_new(cls):
            nonlocal instantiation_count
            instantiation_count += 1
            return original_new(cls)

        User.__new__ = tracked_new

        try:
            # Create wrapper and access fields
            wrapped = JSONPassthrough(data, "User", User)
            _ = wrapped.name
            _ = wrapped.organization
            _ = wrapped.organization.name

            # Verify no User objects were instantiated
            assert instantiation_count == 0

        finally:
            User.__new__ = original_new

    def test_lazy_evaluation(self):
        """Test that nested objects are only wrapped when accessed."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "John Doe",
            "organization": {"id": "660e8400-e29b-41d4-a716-446655440001", "name": "Acme Corp"},
        }

        wrapped = JSONPassthrough(data, "User", User)

        # Cache should be empty initially
        assert len(wrapped._wrapped_cache) == 0

        # Access name - no caching needed for scalars
        _ = wrapped.name
        assert len(wrapped._wrapped_cache) == 0

        # Access organization - should create and cache wrapper
        org = wrapped.organization
        assert len(wrapped._wrapped_cache) == 1
        assert "organization" in wrapped._wrapped_cache

        # Second access should use cache
        org2 = wrapped.organization
        assert org is org2  # Same instance