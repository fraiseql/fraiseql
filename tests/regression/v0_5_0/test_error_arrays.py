"""Error Arrays Regression Tests - v0.5.0 Feature

This test suite validates the error arrays functionality introduced in v0.5.0,
which allows mutations to return structured arrays of validation errors with
comprehensive error information.

Key Features Tested:
- Multiple validation errors returned as arrays
- Structured error objects with code, identifier, message, details
- Field-level validation error grouping
- Security validation error patterns
- Business rule violation error patterns
- Comprehensive error metadata and debugging information
"""

import uuid
from typing import Optional, Union

import pytest

import fraiseql
from fraiseql.types import fraise_input, fraise_type


# Mock Author type for testing
@fraise_type
class Author:
    id: uuid.UUID
    identifier: str
    name: str
    email: str
    bio: Optional[str] = None


@fraise_input
class CreateAuthorInput:
    identifier: str
    name: str
    email: str
    bio: Optional[str] = None


# Error types with arrays support
@fraise_type
class ValidationError:
    code: int
    identifier: str
    message: str
    details: Optional[dict] = None


@fraiseql.success
class CreateAuthorSuccess:
    author: Author
    message: str = "Author created successfully"
    errors: list[ValidationError] = []


@fraiseql.error
class CreateAuthorError:
    message: str
    errors: list[ValidationError]
    validation_summary: Optional[dict] = None
    conflict_author: Optional[Author] = None


def validate_author_input(input_data: dict) -> list[dict]:
    """Mock validation function that returns error array."""
    errors = []

    # Check required fields
    required_fields = ["identifier", "name", "email"]
    for field in required_fields:
        if not input_data.get(field):
            errors.append(
                {
                    "code": 422,
                    "identifier": "missing_required_field",
                    "message": f"Missing required field: {field}",
                    "details": {"field": field, "constraint": "required"},
                }
            )

    # Validate field lengths
    if input_data.get("name") and len(input_data["name"]) > 100:
        errors.append(
            {
                "code": 422,
                "identifier": "name_too_long",
                "message": "Name exceeds maximum length of 100 characters",
                "details": {
                    "field": "name",
                    "constraint": "length",
                    "max_length": 100,
                    "current_length": len(input_data["name"]),
                },
            }
        )

    # Validate email format (simple check)
    email = input_data.get("email", "")
    if email and "@" not in email:
        errors.append(
            {
                "code": 422,
                "identifier": "invalid_email_format",
                "message": "Email format is invalid",
                "details": {"field": "email", "constraint": "format"},
            }
        )

    return errors


def create_author_mutation_logic(input_data: dict) -> Union[CreateAuthorSuccess, CreateAuthorError]:
    """Pure logic for create author mutation with error arrays."""
    # Validate input and collect errors
    validation_errors = validate_author_input(input_data)

    if validation_errors:
        return CreateAuthorError(
            message=f"Author creation failed with {len(validation_errors)} validation errors",
            errors=[ValidationError(**error) for error in validation_errors],
            validation_summary={
                "total_errors": len(validation_errors),
                "has_validation_errors": any(e["code"] == 422 for e in validation_errors),
                "has_conflicts": any(e["code"] == 409 for e in validation_errors),
            },
        )

    # Create successful author
    author_id = uuid.uuid4()
    author = Author(
        id=author_id,
        identifier=input_data["identifier"],
        name=input_data["name"],
        email=input_data["email"],
        bio=input_data.get("bio"),
    )

    return CreateAuthorSuccess(author=author, message="Author created successfully", errors=[])


@pytest.mark.regression
@pytest.mark.unit
class TestErrorArraysV050:
    """Test error arrays functionality introduced in v0.5.0."""

    def test_multiple_validation_errors_returned_as_array(self) -> None:
        """Test that multiple validation errors are returned as structured array."""
        # Input with multiple validation errors
        input_data = {
            "identifier": "",  # Missing required field
            "name": "A" * 150,  # Too long
            "email": "invalid-email",  # Invalid format
        }

        result = create_author_mutation_logic(input_data)

        assert isinstance(result, CreateAuthorError)
        assert len(result.errors) >= 3  # Should have multiple validation errors

        # Check error structure
        for error in result.errors:
            assert hasattr(error, "code")
            assert hasattr(error, "identifier")
            assert hasattr(error, "message")
            assert isinstance(error.code, int)
            assert isinstance(error.identifier, str)
            assert isinstance(error.message, str)

        # Check specific error types
        error_identifiers = [e.identifier for e in result.errors]
        assert "missing_required_field" in error_identifiers
        assert "name_too_long" in error_identifiers
        assert "invalid_email_format" in error_identifiers

    def test_successful_creation_returns_empty_errors_array(self) -> None:
        """Test that successful operations return empty errors array."""
        input_data = {
            "identifier": "success-author",
            "name": "Success Author",
            "email": "success@example.com",
        }

        result = create_author_mutation_logic(input_data)

        assert isinstance(result, CreateAuthorSuccess)

        # Success should have empty errors array (not null)
        errors = result.errors
        assert isinstance(errors, list)
        assert len(errors) == 0

        # Should have actual data
        assert result.author is not None
        assert result.author.identifier == "success-author"

    def test_error_array_structure_consistency(self) -> None:
        """Test that all errors in array follow consistent structure."""
        # Input that will generate multiple different error types
        input_data = {"identifier": "", "name": "", "email": "bad-email"}

        result = create_author_mutation_logic(input_data)
        assert isinstance(result, CreateAuthorError)

        errors = result.errors
        assert len(errors) >= 2

        # Every error must follow consistent structure
        for i, error in enumerate(errors):
            # Required fields
            assert hasattr(error, "code"), f"Error {i} missing 'code' field"
            assert hasattr(error, "identifier"), f"Error {i} missing 'identifier' field"
            assert hasattr(error, "message"), f"Error {i} missing 'message' field"

            # Field types
            assert isinstance(error.code, int), f"Error {i} 'code' should be integer"
            assert isinstance(error.identifier, str), f"Error {i} 'identifier' should be string"
            assert isinstance(error.message, str), f"Error {i} 'message' should be string"

            # Code should be valid HTTP status code
            assert error.code in [
                400,
                401,
                403,
                404,
                409,
                422,
                500,
            ], f"Error {i} has invalid code: {error.code}"

            # Message should be non-empty
            assert len(error.message) > 0, f"Error {i} has empty message"

            # Details should be dict if present
            if error.details:
                assert isinstance(error.details, dict), f"Error {i} 'details' should be dict"

    def test_field_level_error_grouping_capability(self) -> None:
        """Test that errors can be grouped by field for client convenience."""
        input_data = {
            "identifier": "",  # Missing required field
            "name": "X" * 150,  # Too long
            "email": "invalid-email",  # Invalid format
        }

        result = create_author_mutation_logic(input_data)
        assert isinstance(result, CreateAuthorError)

        errors = result.errors

        # Demonstrate client-side field grouping capability
        field_errors = {}
        for error in errors:
            field = error.details.get("field") if error.details else None
            if field:
                if field not in field_errors:
                    field_errors[field] = []
                field_errors[field].append(
                    {
                        "identifier": error.identifier,
                        "message": error.message,
                        "constraint": error.details.get("constraint") if error.details else None,
                    }
                )

        # Should have errors grouped by field
        assert "identifier" in field_errors
        assert "name" in field_errors
        assert "email" in field_errors

        # Each field should have appropriate error types
        name_errors = field_errors["name"]
        assert any("too_long" in e["identifier"] for e in name_errors)

        email_errors = field_errors["email"]
        assert any("format" in e["identifier"] for e in email_errors)

    def test_validation_summary_provides_aggregated_information(self) -> None:
        """Test that validation summary provides useful aggregated error information."""
        input_data = {
            "identifier": "",  # Missing field - validation error
            "name": "Valid Name",
            "email": "valid@example.com",
        }

        result = create_author_mutation_logic(input_data)
        assert isinstance(result, CreateAuthorError)

        validation_summary = result.validation_summary
        assert validation_summary is not None
        assert validation_summary["total_errors"] == 1
        assert validation_summary["has_validation_errors"] is True
        assert validation_summary["has_conflicts"] is False

    def test_error_array_supports_different_error_codes(self) -> None:
        """Test that error arrays can contain different HTTP status codes."""

        # Add conflict check to validation function temporarily for this test
        def extended_validation(input_data: dict) -> list[dict]:
            errors = validate_author_input(input_data)

            # Simulate conflict error
            if input_data.get("identifier") == "existing-author":
                errors.append(
                    {
                        "code": 409,
                        "identifier": "duplicate_identifier",
                        "message": "Author with this identifier already exists",
                        "details": {
                            "field": "identifier",
                            "constraint": "unique",
                            "conflict_id": str(uuid.uuid4()),
                        },
                    }
                )

            return errors

        input_data = {
            "identifier": "existing-author",  # Conflict
            "name": "B" * 150,  # Validation error - too long
            "email": "valid@example.com",  # Valid email
        }

        # Use extended validation for this test
        validation_errors = extended_validation(input_data)

        # Should have both 409 (conflict) and 422 (validation) errors
        error_codes = [e["code"] for e in validation_errors]
        assert 409 in error_codes  # Conflict error
        assert 422 in error_codes  # Validation error

        # Create result with extended errors
        result = CreateAuthorError(
            message=f"Author creation failed with {len(validation_errors)} validation errors",
            errors=[ValidationError(**error) for error in validation_errors],
            validation_summary={
                "total_errors": len(validation_errors),
                "has_validation_errors": any(e["code"] == 422 for e in validation_errors),
                "has_conflicts": any(e["code"] == 409 for e in validation_errors),
            },
        )

        # Validation summary should reflect mixed error types
        assert result.validation_summary["has_conflicts"] is True
        assert result.validation_summary["has_validation_errors"] is True
