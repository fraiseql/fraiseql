"""Regression test for FraiseQL v0.7.17 GraphQL validation bypass issue.

This test ensures that the v0.7.17 regression where GraphQL processing
bypassed FraiseQL input validation completely does not reoccur.

Critical Bug: The coerce_input() function used object.__new__() instead of
calling the class constructor, completely bypassing validation in GraphQL mutations.

Issue: GraphQL mutations accept invalid input (empty strings, etc.) that would
be rejected when creating FraiseQL input objects directly.

Fixed in: v0.7.18 by changing coerce_input() to use cls(**coerced_data)
instead of object.__new__() + manual attribute setting.
"""

from typing import Optional
from uuid import UUID

import pytest

import fraiseql
from fraiseql.types.coercion import coerce_input


@fraiseql.input
class ValidationInput:
    """Test input class with validation that should be enforced in GraphQL."""

    name: str  # Non-optional string that should reject empty values
    email: str
    test_id: Optional[UUID] = None


@fraiseql.input
class CreateUserInput:
    """Input for creating a user - mirrors the bug reproduction case."""

    name: str
    email: str
    password: str
    bio: Optional[str] = None


@fraiseql.success
class CreateUserSuccess:
    """Success response for user creation."""

    message: str = "User created successfully"
    name: str


@fraiseql.error
class CreateUserError:
    """Error response for user creation."""

    message: str
    code: str = "VALIDATION_ERROR"


@fraiseql.mutation(function="create_user")
class CreateUserMutation:
    """Test mutation to verify GraphQL validation."""

    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError


class TestV0717GraphQLValidationBypassRegression:
    """Test suite for the v0.7.17 GraphQL validation bypass regression fix."""

    def test_direct_input_validation_still_works(self) -> None:
        """Test that direct FraiseQL input validation still works correctly.

        This verifies that our fix doesn't break the expected validation behavior
        when creating input objects directly (not through GraphQL).
        """
        # Valid input should work
        valid_input = ValidationInput(name="John Doe", email="john@example.com")
        assert valid_input.name == "John Doe"
        assert valid_input.email == "john@example.com"

        # Empty string should be rejected
        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            ValidationInput(name="", email="john@example.com")  # Empty string should fail

        # Whitespace-only string should be rejected
        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            ValidationInput(name="   ", email="john@example.com")  # Whitespace-only should fail

    def test_coerce_input_function_calls_constructor(self) -> None:
        """Test that coerce_input() now calls the class constructor for validation.

        This is the core fix - coerce_input() must call cls(**data) instead of
        using object.__new__() to bypass validation.
        """
        # Valid data should work
        valid_data = {"name": "John Doe", "email": "john@example.com"}
        result = coerce_input(ValidationInput, valid_data)
        assert isinstance(result, ValidationInput)
        assert result.name == "John Doe"
        assert result.email == "john@example.com"

        # Invalid data should now raise validation errors (this was the bug)
        invalid_data = {"name": "", "email": "john@example.com"}  # Empty string should be rejected
        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            coerce_input(ValidationInput, invalid_data)

        # Whitespace-only data should also be rejected
        whitespace_data = {
            "name": "   ",  # Whitespace-only should be rejected
            "email": "john@example.com",
        }
        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            coerce_input(ValidationInput, whitespace_data)

        # None data should also be rejected for required fields (v0.7.18 specific regression)
        none_data = {
            "name": None,  # None should be rejected for required string field
            "email": "john@example.com",
        }
        with pytest.raises(ValueError, match="Field 'name' is required and cannot be None"):
            coerce_input(ValidationInput, none_data)

    def test_coerce_input_with_missing_required_fields(self) -> None:
        """Test that coerce_input() properly handles missing required fields."""
        # Missing required field should raise proper error
        incomplete_data = {
            "name": "John Doe"
            # Missing required 'email' field
        }
        with pytest.raises(ValueError, match="Missing required field 'email'"):
            coerce_input(ValidationInput, incomplete_data)

    def test_coerce_input_with_optional_fields(self) -> None:
        """Test that coerce_input() properly handles optional fields."""
        # Optional fields should work when omitted
        data_without_optional = {
            "name": "John Doe",
            "email": "john@example.com",
            # test_id is optional and omitted
        }
        result = coerce_input(ValidationInput, data_without_optional)
        assert result.name == "John Doe"
        assert result.email == "john@example.com"
        assert result.test_id is None

        # Optional fields should work when provided
        data_with_optional = {
            "name": "Jane Doe",
            "email": "jane@example.com",
            "test_id": "12345678-1234-1234-1234-123456789012",
        }
        result = coerce_input(ValidationInput, data_with_optional)
        assert result.name == "Jane Doe"
        assert result.test_id is not None

    def test_nested_input_coercion_validation(self) -> None:
        """Test that nested input objects also get proper validation through coercion."""

        @fraiseql.input
        class NestedInput:
            nested_name: str

        @fraiseql.input
        class ParentInput:
            parent_name: str
            nested: NestedInput

        # Valid nested data should work
        valid_nested_data = {"parent_name": "Parent", "nested": {"nested_name": "Child"}}
        result = coerce_input(ParentInput, valid_nested_data)
        assert result.parent_name == "Parent"
        assert result.nested.nested_name == "Child"

        # Invalid nested data should be rejected
        invalid_nested_data = {
            "parent_name": "Parent",
            "nested": {"nested_name": ""},  # Empty string in nested object should fail
        }
        with pytest.raises(ValueError, match="Field 'nested_name' cannot be empty"):
            coerce_input(ParentInput, invalid_nested_data)

    def test_coerce_input_arguments_validation_integration(self) -> None:
        """Integration test for coerce_input_arguments function with validation.

        This test verifies that the GraphQL argument coercion process now
        properly validates input through the corrected coerce_input() function.
        This is the key integration point where the bug manifested.
        """
        from fraiseql.types.coercion import coerce_input_arguments

        # Mock resolver function signature that matches GraphQL mutation resolvers
        async def mock_create_user_resolver(info, input: CreateUserInput) -> None:
            """Mock resolver that would normally be called by GraphQL."""
            return CreateUserSuccess(name=input.name, message=f"Created {input.name}")

        # Test 1: Valid GraphQL arguments should work
        valid_raw_args = {
            "input": {"name": "John Doe", "email": "john@example.com", "password": "secretpass"}
        }

        coerced_args = coerce_input_arguments(mock_create_user_resolver, valid_raw_args)

        # Should successfully coerce and validate
        assert "input" in coerced_args
        assert isinstance(coerced_args["input"], CreateUserInput)
        assert coerced_args["input"].name == "John Doe"
        assert coerced_args["input"].email == "john@example.com"

        # Test 2: Invalid arguments (empty string) should now raise validation errors
        invalid_raw_args = {
            "input": {
                "name": "",  # Empty string should be rejected
                "email": "john@example.com",
                "password": "secretpass",
            }
        }

        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            coerce_input_arguments(mock_create_user_resolver, invalid_raw_args)

        # Test 3: Whitespace-only arguments should also be rejected
        whitespace_raw_args = {
            "input": {
                "name": "   ",  # Whitespace-only should be rejected
                "email": "john@example.com",
                "password": "secretpass",
            }
        }

        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            coerce_input_arguments(mock_create_user_resolver, whitespace_raw_args)

        # Test 4: None arguments should also be rejected for required fields (v0.7.18 regression)
        none_raw_args = {
            "input": {
                "name": None,  # None should be rejected for required field
                "email": "john@example.com",
                "password": "secretpass",
            }
        }

        with pytest.raises(ValueError, match="Field 'name' is required and cannot be None"):
            coerce_input_arguments(mock_create_user_resolver, none_raw_args)

    def test_regression_case_from_bug_report(self) -> None:
        """Test the specific case from the bug report that was failing.

        This reproduces the exact scenario described in the bug report
        where empty strings were making it through GraphQL validation.
        """
        # This reproduces the exact failing coerce_input call from the bug
        bug_reproduction_data = {"name": "", "email": "test@example.com", "password": "secretpass"}

        # Before the fix, this would succeed and create an object with empty name
        # After the fix, this should raise a validation error
        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            coerce_input(CreateUserInput, bug_reproduction_data)

        # Verify valid data still works
        valid_data = {"name": "Valid Name", "email": "test@example.com", "password": "secretpass"}
        result = coerce_input(CreateUserInput, valid_data)
        assert result.name == "Valid Name"
        assert result.email == "test@example.com"
        assert result.password == "secretpass"

    def test_fix_preserves_existing_functionality(self) -> None:
        """Test that the fix doesn't break any existing coercion functionality."""

        # Test default values work
        @fraiseql.input
        class InputWithDefaults:
            required_field: str
            optional_with_default: str = "default_value"

        data_without_optional = {"required_field": "test"}
        result = coerce_input(InputWithDefaults, data_without_optional)
        assert result.required_field == "test"
        assert result.optional_with_default == "default_value"

        # Test optional fields work
        @fraiseql.input
        class InputWithOptional:
            required_field: str
            optional_field: Optional[str] = None

        data_with_optional = {"required_field": "test", "optional_field": "optional"}
        result = coerce_input(InputWithOptional, data_with_optional)
        assert result.required_field == "test"
        assert result.optional_field == "optional"

        data_without_optional = {"required_field": "test"}
        result = coerce_input(InputWithOptional, data_without_optional)
        assert result.required_field == "test"
        assert result.optional_field is None
