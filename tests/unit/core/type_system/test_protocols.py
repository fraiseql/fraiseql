import pytest

"""Tests for FraiseQL type protocols."""

from typing import Any
from unittest.mock import MagicMock

import fraiseql
from fraiseql.types.definitions import FraiseQLTypeDefinition
from fraiseql.types.protocols import FraiseQLInputType, FraiseQLOutputType


@pytest.mark.unit
class TestFraiseQLProtocols:
    """Test FraiseQL type protocols."""

    def test_output_type_protocol_attributes(self) -> None:
        """Test that FraiseQLOutputType protocol defines required attributes."""

        # Create a mock class that implements the protocol
        class MockOutputType:
            __gql_typename__ = "User"
            __gql_table__ = "users"
            __gql_where_type__ = object()
            __gql_fields__ = {"id": "UUID", "name": "str"}
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        # This should not raise any type errors
        output_type: FraiseQLOutputType = MockOutputType()

        assert output_type.__gql_typename__ == "User"
        assert output_type.__gql_table__ == "users"
        assert isinstance(output_type.__gql_where_type__, object)
        assert output_type.__gql_fields__ == {"id": "UUID", "name": "str"}
        assert hasattr(output_type.__fraiseql_definition__, "_spec_class")

    def test_input_type_protocol_attributes(self) -> None:
        """Test that FraiseQLInputType protocol defines required attributes."""

        # Create a mock class that implements the protocol
        class MockInputType:
            __gql_typename__ = "CreateUserInput"
            __gql_fields__ = {"name": "str", "email": "str"}
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        # This should not raise any type errors
        input_type: FraiseQLInputType = MockInputType()

        assert input_type.__gql_typename__ == "CreateUserInput"
        assert input_type.__gql_fields__ == {"name": "str", "email": "str"}
        assert hasattr(input_type.__fraiseql_definition__, "_spec_class")

    def test_output_type_protocol_table_optional(self) -> None:
        """Test that __gql_table__ can be None for output types."""

        class MockOutputTypeNoTable:
            __gql_typename__ = "User"
            __gql_table__ = None  # This should be allowed
            __gql_where_type__ = object()
            __gql_fields__ = {"id": "UUID"}
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        output_type: FraiseQLOutputType = MockOutputTypeNoTable()
        assert output_type.__gql_table__ is None

    def test_protocol_fields_typing(self) -> None:
        """Test that protocol fields accept proper types."""

        class MockCompleteType:
            __gql_typename__ = "ComplexType"
            __gql_table__ = "complex_table"
            __gql_where_type__ = dict  # Any object type,
            __gql_fields__ = {
                "id": "UUID",
                "name": "str",
                "nested": {"type": "NestedType", "list": True},
            }
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        # Test as output type
        output_type: FraiseQLOutputType = MockCompleteType()
        assert isinstance(output_type.__gql_fields__, dict)
        assert "nested" in output_type.__gql_fields__

        # Test as input type (should work with subset of attributes)
        input_type: FraiseQLInputType = MockCompleteType()
        assert input_type.__gql_typename__ == "ComplexType"

    def test_protocol_inheritance_compatibility(self) -> None:
        """Test that real FraiseQL types can be used as protocol types."""
        from fraiseql.types.fraise_input import fraise_input

        # Create actual FraiseQL types
        @fraiseql.type
        class User:
            id: str
            name: str

        @fraise_input
        class CreateUserInput:
            name: str
            email: str

        # These should satisfy the protocols
        def process_output_type(output_type: FraiseQLOutputType) -> str:
            return output_type.__gql_typename__

        def process_input_type(input_type: FraiseQLInputType) -> str:
            return input_type.__gql_typename__

        # Should not raise type errors
        user_typename = process_output_type(User)
        input_typename = process_input_type(CreateUserInput)

        assert user_typename == "User"
        assert input_typename == "CreateUserInput"

    def test_protocol_field_access(self) -> None:
        """Test that protocol fields can be accessed properly."""

        class MockTypeWithComplexFields:
            __gql_typename__ = "TestType"
            __gql_table__ = "test_table"
            __gql_where_type__ = str
            __gql_fields__ = {
                "simple_field": "str",
                "complex_field": {
                    "type": "ComplexType",
                    "nullable": True,
                    "description": "A complex field",
                },
            }
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        test_type: FraiseQLOutputType = MockTypeWithComplexFields()

        # Test field access
        fields = test_type.__gql_fields__
        assert "simple_field" in fields
        assert "complex_field" in fields
        assert isinstance(fields["complex_field"], dict)
        assert fields["complex_field"]["nullable"] is True

    def test_protocol_with_minimal_implementation(self) -> None:
        """Test protocols with minimal required implementations."""

        # Minimal output type
        class MinimalOutput:
            __gql_typename__ = "Minimal"
            __gql_table__ = None
            __gql_where_type__ = object()
            __gql_fields__: dict[str, Any] = {}
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        # Minimal input type
        class MinimalInput:
            __gql_typename__ = "MinimalInput"
            __gql_fields__: dict[str, Any] = {}
            __fraiseql_definition__ = MagicMock(spec=FraiseQLTypeDefinition)

        # Should satisfy protocols
        output: FraiseQLOutputType = MinimalOutput()
        input: FraiseQLInputType = MinimalInput()

        assert output.__gql_typename__ == "Minimal"
        assert input.__gql_typename__ == "MinimalInput"
