"""Unit tests for FraiseQL mutation decorators.

This module tests mutation decorator functionality including:
- @success and @failure decorators
- @mutation decorator
- Input validation
- Result type generation
- Error handling patterns

Tests are isolated unit tests using mocks for external dependencies.
"""

from typing import Any, Dict, Optional

import pytest

import fraiseql
from fraiseql.mutations.decorators import failure, success
from fraiseql.mutations.mutation_decorator import MutationDefinition

# Import decorators from main fraiseql module
mutation = fraiseql.mutation
fraise_input = fraiseql.input


# Test input types
@fraise_input
class SampleInput:
    """Sample input for mutation testing."""

    name: str
    email: str
    age: Optional[int] = None


@fraise_input
class UpdateInput:
    """Sample update input."""

    name: Optional[str] = None
    email: Optional[str] = None
    is_active: Optional[bool] = None


# Test entity type
@fraiseql.type
class User:
    """Sample user type for testing."""

    id: str
    name: str
    email: str
    is_active: bool


# Test result types
@success
class SampleSuccess:
    """Sample success result."""

    message: str
    user: User
    metadata: Optional[Dict[str, Any]] = None


@success
class UpdateSuccess:
    """Sample update success result."""

    user: User
    message: str = "User updated successfully"


@failure
class SampleError:
    """Sample error result."""

    message: str
    code: str = "ERROR"
    details: Optional[Dict[str, Any]] = None


@failure
class ValidationError:
    """Validation error result."""

    message: str
    code: str = "VALIDATION_ERROR"
    field_errors: Optional[Dict[str, str]] = None


@failure
class NotFoundError:
    """Not found error result."""

    message: str = "Entity not found"
    code: str = "NOT_FOUND"
    entity_id: Optional[str] = None


class TestSuccessDecorator:
    """Test @success decorator functionality."""

    def test_success_decorator_basic(self, clear_schema_registry):
        """Test basic success decorator usage."""

        @success
        class TestSuccess:
            message: str
            data: Dict[str, Any]

        # Should be marked as success type
        assert hasattr(TestSuccess, "__fraiseql_definition__")

        # Should be instantiable
        instance = TestSuccess(message="test", data={})
        assert instance.message == "test"

    def test_success_decorator_with_defaults(self, clear_schema_registry):
        """Test success decorator with default field values."""

        @success
        class DefaultSuccess:
            message: str = "Operation successful"
            status: str = "OK"
            data: Optional[Dict[str, Any]] = None

        # Should create instance with defaults
        instance = DefaultSuccess()
        assert instance.message == "Operation successful"
        assert instance.status == "OK"
        assert instance.data is None

    def test_success_decorator_inheritance(self, clear_schema_registry):
        """Test success decorator with inheritance."""

        @success
        class BaseSuccess:
            message: str
            timestamp: str

        @success
        class ExtendedSuccess(BaseSuccess):
            extra_data: Dict[str, Any]

        # Both should be marked as success types
        assert hasattr(BaseSuccess, "__fraiseql_definition__")
        assert hasattr(ExtendedSuccess, "__fraiseql_definition__")

        # Extended should be instantiable with inherited and own fields
        instance = ExtendedSuccess(
            message="test", timestamp="2023-01-01", extra_data={"key": "value"}
        )
        assert instance.message == "test"
        assert instance.timestamp == "2023-01-01"
        assert instance.extra_data == {"key": "value"}


class TestFailureDecorator:
    """Test @failure decorator functionality."""

    def test_failure_decorator_basic(self, clear_schema_registry):
        """Test basic failure decorator usage."""

        @failure
        class TestError:
            message: str
            code: str

        # Should be marked as failure type
        assert hasattr(TestError, "__fraiseql_definition__")

        # Should be instantiable
        instance = TestError(message="test error", code="ERROR")
        assert instance.message == "test error"

    def test_failure_decorator_with_defaults(self, clear_schema_registry):
        """Test failure decorator with default values."""

        @failure
        class DefaultError:
            message: str
            code: str = "GENERIC_ERROR"
            severity: str = "ERROR"

        # Should create instance with defaults
        instance = DefaultError(message="Test error")
        assert instance.message == "Test error"
        assert instance.code == "GENERIC_ERROR"
        assert instance.severity == "ERROR"

    def test_failure_decorator_inheritance(self, clear_schema_registry):
        """Test failure decorator with inheritance."""

        @failure
        class BaseError:
            message: str
            code: str

        @failure
        class ValidationError(BaseError):
            field_errors: Dict[str, str]
            code: str = "VALIDATION_ERROR"  # Override default

        # Both should be marked as failure types
        assert hasattr(BaseError, "__fraiseql_definition__")
        assert hasattr(ValidationError, "__fraiseql_definition__")

        # Validation error should have specific default
        instance = ValidationError(message="Validation failed", field_errors={})
        assert instance.code == "VALIDATION_ERROR"


class TestMutationDecorator:
    """Test @mutation decorator functionality."""

    def test_mutation_decorator_basic(self, clear_schema_registry):
        """Test basic mutation decorator usage."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        # Should have mutation definition
        assert hasattr(CreateUser, "__fraiseql_mutation__")
        assert isinstance(CreateUser.__fraiseql_mutation__, MutationDefinition)

        definition = CreateUser.__fraiseql_mutation__
        assert definition.input_type == SampleInput
        assert definition.success_type == SampleSuccess
        assert definition.error_type == SampleError

    def test_mutation_decorator_with_function_name(self, clear_schema_registry):
        """Test mutation decorator with custom function name."""

        @mutation(function="create_user_v2")
        class CustomMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = CustomMutation.__fraiseql_mutation__
        assert hasattr(definition, "function_name")  # Check that function name can be set

    def test_mutation_decorator_with_schema_param(self, clear_schema_registry):
        """Test mutation with schema parameter."""

        @mutation(schema="app")
        class SchemaBasedMutation:
            input: UpdateInput
            success: UpdateSuccess
            failure: SampleError

        definition = SchemaBasedMutation.__fraiseql_mutation__

        # Should identify success and error types
        assert definition.success_type == UpdateSuccess
        assert definition.error_type == SampleError

    def test_mutation_decorator_missing_success_raises_error(self, clear_schema_registry):
        """Test that mutation requires success type."""
        with pytest.raises(TypeError, match="must define 'success'"):

            @mutation
            class InvalidMutation:
                input: SampleInput
                failure: SampleError
                # Missing success type

    def test_mutation_decorator_missing_input_raises_error(self, clear_schema_registry):
        """Test that mutation requires input type."""
        with pytest.raises(TypeError, match="must define 'input'"):

            @mutation
            class InvalidMutation:
                # Missing input type
                success: SampleSuccess
                failure: SampleError


class TestMutationDefinition:
    """Test MutationDefinition class functionality."""

    def test_create_definition_with_all_types(self, clear_schema_registry):
        """Test creating mutation definition with all required types."""

        @mutation
        class CreateUser:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = CreateUser.__fraiseql_mutation__

        assert isinstance(definition, MutationDefinition)
        assert definition.name == "CreateUser"
        assert definition.input_type == SampleInput
        assert definition.success_type == SampleSuccess
        assert definition.error_type == SampleError

    def test_definition_get_result_types(self, clear_schema_registry):
        """Test getting all result types from definition."""

        @mutation
        class SimpleMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = SimpleMutation.__fraiseql_mutation__

        # Test basic properties
        assert definition.success_type == SampleSuccess
        assert definition.error_type == SampleError
        assert definition.input_type == SampleInput

    def test_definition_has_proper_attributes(self, clear_schema_registry):
        """Test definition has proper attributes."""

        @mutation
        class AttributesMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = AttributesMutation.__fraiseql_mutation__

        # Should have all expected attributes
        assert hasattr(definition, "name")
        assert hasattr(definition, "input_type")
        assert hasattr(definition, "success_type")
        assert hasattr(definition, "error_type")
        assert hasattr(definition, "mutation_class")
        assert definition.name == "AttributesMutation"

    def test_definition_with_context_params(self, clear_schema_registry):
        """Test mutation definition with context parameters."""

        @mutation(context_params={"user_id": "current_user"})
        class ContextMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = ContextMutation.__fraiseql_mutation__
        assert definition.context_params == {"user_id": "current_user"}


class TestMutationValidation:
    """Test mutation input validation functionality."""

    def test_validate_input_type_structure(self, clear_schema_registry):
        """Test validation of input type structure."""

        # Valid input type
        @fraise_input
        class ValidInput:
            required_field: str
            optional_field: Optional[int] = None

        @mutation
        class ValidMutation:
            input: ValidInput
            success: SampleSuccess
            error: SampleError

        # Should not raise error
        definition = ValidMutation.__fraiseql_mutation__
        assert definition.input_type == ValidInput

    def test_validate_result_type_structure(self, clear_schema_registry):
        """Test validation of result type structure."""

        # Success type must be marked with @success
        @success
        class ValidSuccess:
            message: str
            data: Any

        # Error type must be marked with @failure
        @failure
        class ValidError:
            message: str
            code: str

        @mutation
        class ValidMutation:
            input: SampleInput
            success: ValidSuccess
            error: ValidError

        definition = ValidMutation.__fraiseql_mutation__
        assert definition.success_type == ValidSuccess
        assert definition.error_type == ValidError


class TestMutationSchemaIntegration:
    """Test mutation schema integration."""

    def test_mutation_with_function_parameter(self, clear_schema_registry):
        """Test mutation with custom function name."""

        @mutation(function="create_user_custom")
        class CreateUserMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = CreateUserMutation.__fraiseql_mutation__

        # Should have function name set
        assert hasattr(definition, "function_name")

    def test_mutation_with_schema_parameter(self, clear_schema_registry):
        """Test mutation with schema parameter."""

        @mutation(schema="custom_schema")
        class SchemaMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = SchemaMutation.__fraiseql_mutation__

        # Should have schema set
        assert hasattr(definition, "schema")

    def test_mutation_creates_resolver(self, clear_schema_registry):
        """Test that mutation definition can create resolver."""

        @mutation
        class ResolverMutation:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        definition = ResolverMutation.__fraiseql_mutation__

        # Should be able to create resolver
        assert hasattr(definition, "create_resolver")
        resolver = definition.create_resolver()
        assert callable(resolver)


class TestMutationDecoratorEdgeCases:
    """Test edge cases and error conditions."""

    def test_mutation_with_no_annotations_raises_error(self, clear_schema_registry):
        """Test that mutation without annotations raises error."""
        with pytest.raises(TypeError, match="must define 'input'"):

            @mutation
            class NoAnnotationsMutation:
                pass  # No type annotations

    def test_mutation_allows_non_decorated_types(self, clear_schema_registry):
        """Test mutation works with non-decorated types."""

        class NotASuccessType:
            message: str

        # FraiseQL appears to be more permissive than expected
        @mutation
        class FlexibleMutation:
            input: SampleInput
            success: NotASuccessType  # Not marked with @success
            failure: SampleError

        definition = FlexibleMutation.__fraiseql_mutation__
        assert definition.success_type == NotASuccessType

    def test_mutation_with_regular_error_type(self, clear_schema_registry):
        """Test mutation with non-decorated error type."""

        class NotAnErrorType:
            message: str

        @mutation
        class FlexibleErrorMutation:
            input: SampleInput
            success: SampleSuccess
            failure: NotAnErrorType  # Not marked with @failure

        definition = FlexibleErrorMutation.__fraiseql_mutation__
        assert definition.error_type == NotAnErrorType

    def test_mutation_redefinition(self, clear_schema_registry):
        """Test behavior when redefining mutation."""

        @mutation
        class TestMutationFirst:
            input: SampleInput
            success: SampleSuccess
            failure: SampleError

        first_definition = TestMutationFirst.__fraiseql_mutation__

        # Create different mutation with same base structure
        @mutation
        class TestMutationSecond:  # Different name
            input: UpdateInput  # Different input
            success: UpdateSuccess  # Different success
            failure: SampleError

        second_definition = TestMutationSecond.__fraiseql_mutation__

        # Should be different definitions
        assert first_definition != second_definition
        assert first_definition.input_type == SampleInput
        assert second_definition.input_type == UpdateInput
        assert second_definition.success_type == UpdateSuccess
