"""Test that mutations can use 'failure' instead of 'error'."""

import pytest

from fraiseql import failure, fraise_input, fraise_type, mutation, success


# Input type
@fraise_input
class CreateUserInput:
    name: str
    email: str


# Success type
@success
@fraise_type
class CreateUserSuccess:
    user_id: int
    message: str = "User created successfully"


# Error/Failure type
@failure
@fraise_type
class CreateUserFailure:
    code: str
    message: str


def test_mutation_with_failure_attribute():
    """Test mutation using 'failure' instead of 'error'."""

    @mutation
    class CreateUser:
        input: CreateUserInput
        success: CreateUserSuccess
        failure: CreateUserFailure  # Using 'failure' instead of 'error'

        async def execute(self, db, input_data):
            # Mock implementation
            return CreateUserSuccess(user_id=1)

    # Should not raise an error
    assert CreateUser.__fraiseql_mutation__ is not None
    assert CreateUser.__fraiseql_mutation__.error_type == CreateUserFailure


def test_mutation_with_error_still_works():
    """Test that 'error' attribute still works for backwards compatibility."""

    @mutation
    class CreateUserLegacy:
        input: CreateUserInput
        success: CreateUserSuccess
        error: CreateUserFailure  # Using legacy 'error' name

        async def execute(self, db, input_data):
            return CreateUserSuccess(user_id=2)

    # Should work with 'error' too
    assert CreateUserLegacy.__fraiseql_mutation__ is not None
    assert CreateUserLegacy.__fraiseql_mutation__.error_type == CreateUserFailure


def test_mutation_without_failure_or_error_fails():
    """Test that mutation without failure/error type fails."""

    with pytest.raises(TypeError, match="must define 'failure' type"):

        @mutation
        class InvalidMutation:
            input: CreateUserInput
            success: CreateUserSuccess
            # Missing failure/error type!

            async def execute(self, db, input_data):
                return CreateUserSuccess(user_id=3)


def test_mutation_prefers_error_over_failure():
    """Test that if both error and failure are defined, error takes precedence."""

    @fraise_type
    class OtherError:
        message: str

    @mutation
    class CreateUserBoth:
        input: CreateUserInput
        success: CreateUserSuccess
        error: CreateUserFailure  # This should be used
        failure: OtherError  # This should be ignored

        async def execute(self, db, input_data):
            return CreateUserSuccess(user_id=4)

    # Should use 'error' when both are present
    assert CreateUserBoth.__fraiseql_mutation__.error_type == CreateUserFailure
