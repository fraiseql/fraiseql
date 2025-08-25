"""Tests for the @mutation decorator."""

from unittest.mock import AsyncMock, Mock

import pytest

import fraiseql
from fraiseql.mutations.decorators import failure, success
from fraiseql.mutations.mutation_decorator import MutationDefinition, mutation
from fraiseql.types.fraise_input import fraise_input


@fraise_input
class SampleInput:
    name: str
    email: str


@fraiseql.type
class User:
    id: str
    name: str
    email: str


@success
class SampleSuccess:
    message: str
    user: User


@failure
class SampleError:
    message: str
    code: str = "ERROR"


class TestMutationDefinition:
    """Test MutationDefinition class."""

    def test_create_definition_with_all_types(self) -> None:
        """Test creating a mutation definition with all required types."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        definition = CreateUser.__fraiseql_mutation__

        assert isinstance(definition, MutationDefinition)
        assert definition.name == "CreateUser"
        assert definition.function_name == "create_user"
        assert definition.schema == "public"
        assert definition.input_type == SampleInput
        assert definition.success_type == SampleSuccess
        assert definition.error_type == SampleError

    def test_custom_function_name(self) -> None:
        """Test mutation with custom function name."""

        @mutation(function="custom_create_user")
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        definition = CreateUser.__fraiseql_mutation__
        assert definition.function_name == "custom_create_user"

    def test_custom_schema(self) -> None:
        """Test mutation with custom schema."""

        @mutation(schema="mutations")
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        definition = CreateUser.__fraiseql_mutation__
        assert definition.schema == "mutations"

    def test_missing_input_type_raises_error(self) -> None:
        """Test that missing input type raises TypeError."""
        with pytest.raises(TypeError, match="must define 'input' type"):

            @mutation
            class BadMutation:
                success: SampleSuccess
                error: SampleError

    def test_missing_success_type_raises_error(self) -> None:
        """Test that missing success type raises TypeError."""
        with pytest.raises(TypeError, match="must define 'success' type"):

            @mutation
            class BadMutation:
                input: SampleInput
                error: SampleError

    def test_missing_error_type_raises_error(self) -> None:
        """Test that missing error type raises TypeError."""
        with pytest.raises(TypeError, match="must define 'failure' type"):

            @mutation
            class BadMutation:
                input: SampleInput
                success: SampleSuccess

    def test_camel_to_snake_conversion(self) -> None:
        """Test CamelCase to snake_case conversion."""
        test_cases = [
            ("CreateUser", "create_user"),
            ("UpdateUserProfile", "update_user_profile"),
            ("DeletePost", "delete_post"),
            ("BulkUpdateOrders", "bulk_update_orders"),
            ("APIKeyGeneration", "api_key_generation"),
        ]

        for camel, expected_snake in test_cases:

            @mutation
            class TestMutation:
                input: SampleInput
                success: SampleSuccess
                error: SampleError

            # Temporarily change the name
            TestMutation.__name__ = camel
            definition = MutationDefinition(TestMutation)
            assert definition.function_name == expected_snake


class TestMutationResolver:
    """Test the generated resolver function."""

    @pytest.mark.asyncio
    async def test_resolver_calls_database_function(self) -> None:
        """Test that resolver calls the correct database function."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        resolver = CreateUser.__fraiseql_resolver__

        # Mock the context and database
        mock_db = AsyncMock()
        mock_db.execute_function.return_value = {
            "status": "success",
            "message": "User created",
            "object_data": {"id": "123", "name": "John Doe", "email": "john@example.com"},
        }

        info = Mock()
        info.context = {"db": mock_db}

        # Mock input
        input_obj = Mock()
        input_obj.name = "John Doe"
        input_obj.email = "john@example.com"

        # Mock to_dict method
        def mock_to_dict():
            return {"name": "John Doe", "email": "john@example.com"}

        input_obj.to_dict = mock_to_dict

        # Call resolver
        result = await resolver(info, input_obj)

        # Verify database function was called
        mock_db.execute_function.assert_called_once_with(
            """public.create_user""", {"name": "John Doe", "email": "john@example.com"}
        )

        # Verify result type
        assert isinstance(result, SampleSuccess)
        assert result.message == "User created"
        assert isinstance(result.user, User)
        assert result.user.id == "123"

    @pytest.mark.asyncio
    async def test_resolver_handles_error_result(self) -> None:
        """Test that resolver handles error results."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        resolver = CreateUser.__fraiseql_resolver__

        # Mock error response
        mock_db = AsyncMock()
        mock_db.execute_function.return_value = {
            "status": "validation_error",
            "message": "Email already exists",
        }

        info = Mock()
        info.context = {"db": mock_db}

        input_obj = Mock()
        input_obj.to_dict = lambda: {"name": "John", "email": "existing@example.com"}

        # Call resolver
        result = await resolver(info, input_obj)

        # Verify result is error type
        assert isinstance(result, SampleError)
        assert result.message == "Email already exists"
        assert result.code == "validation_error"

    @pytest.mark.asyncio
    async def test_resolver_missing_database_raises_error(self) -> None:
        """Test that missing database in context raises RuntimeError."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            error: SampleError

        resolver = CreateUser.__fraiseql_resolver__

        # No database in context
        info = Mock()
        info.context = {}

        input_obj = Mock()

        with pytest.raises(RuntimeError, match="No database connection in context"):
            await resolver(info, input_obj)

    def test_resolver_metadata(self) -> None:
        """Test that resolver has proper metadata."""

        @mutation
        class CreateUser:
            """Create a new user account."""

            input: SampleInput
            success: SampleSuccess
            error: SampleError

        resolver = CreateUser.__fraiseql_resolver__

        assert resolver.__name__ == "create_user"
        assert "Create a new user account" in resolver.__doc__
        assert hasattr(resolver, "__fraiseql_mutation__")


class TestInputConversion:
    """Test input object to dict conversion."""

    def test_convert_object_with_to_dict(self) -> None:
        """Test converting object with to_dict method."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        obj = Mock()
        obj.to_dict.return_value = {"name": "test", "value": 42}

        result = _to_dict(obj)
        assert result == {"name": "test", "value": 42}

    def test_convert_object_with_dict_attr(self) -> None:
        """Test converting object with __dict__ attribute."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        class TestObj:
            def __init__(self) -> None:
                self.name = "test"
                self.value = 42
                self._private = "hidden"

        result = _to_dict(TestObj())
        assert result == {"name": "test", "value": 42}
        assert "_private" not in result

    def test_convert_dict_object(self) -> None:
        """Test converting dict object."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        data = {"name": "test", "value": 42}
        result = _to_dict(data)
        assert result == data

    def test_convert_unsupported_type_raises_error(self) -> None:
        """Test that unsupported types raise TypeError."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        with pytest.raises(TypeError, match="Cannot convert.*to dictionary"):
            _to_dict("string")
