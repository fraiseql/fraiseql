"""Test that mutations can use 'failure' instead of 'error'."""

import pytest

import fraiseql
from fraiseql import error, fraise_input, mutation, success

pytestmark = pytest.mark.integration


# Input type


@pytest.mark.unit
@fraise_input
class CreateUserInput:
    name: str
    email: str


# Success type
@success
@fraiseql.type
class CreateUserSuccess:
    user_id: int
    message: str = "User created successfully"


# Error/Failure type
@error
@fraiseql.type
class CreateUserFailure:
    code: str
    message: str


def test_mutation_with_failure_attribute() -> None:
    """Test mutation using 'failure' instead of 'error'."""

    @mutation
    class CreateUser:
        input: CreateUserInput
        success: CreateUserSuccess
        error: CreateUserFailure  # Using 'failure' instead of 'error'

        async def execute(self, db, input_data) -> None:
            # Mock implementation
            return CreateUserSuccess(user_id=1)

    # Should not raise an error
    assert CreateUser.__fraiseql_mutation__ is not None
    assert CreateUser.__fraiseql_mutation__.error_type == CreateUserFailure


def test_mutation_with_error_still_works() -> None:
    """Test that 'error' attribute still works for backwards compatibility."""

    @mutation
    class CreateUserLegacy:
        input: CreateUserInput
        success: CreateUserSuccess
        error: CreateUserFailure  # Using legacy 'error' name

        async def execute(self, db, input_data) -> None:
            return CreateUserSuccess(user_id=2)

    # Should work with 'error' too
    assert CreateUserLegacy.__fraiseql_mutation__ is not None
    assert CreateUserLegacy.__fraiseql_mutation__.error_type == CreateUserFailure


def test_mutation_without_failure_or_error_stores_none() -> None:
    """Test that mutation without failure/error type stores None (no validation at decoration time)."""

    @mutation
    class InvalidMutation:
        input: CreateUserInput
        success: CreateUserSuccess
        # Missing failure/error type!

        async def execute(self, db, input_data) -> None:
            return CreateUserSuccess(user_id=3)

    # Error type should be None when not provided
    assert InvalidMutation.__fraiseql_mutation__ is not None
    assert InvalidMutation.__fraiseql_mutation__.error_type is None


def test_mutation_prefers_error_over_failure() -> None:
    """Test that if both error and failure are defined, error takes precedence."""

    @fraiseql.type
    class OtherError:
        message: str

    @mutation
    class CreateUserBoth:
        input: CreateUserInput
        success: CreateUserSuccess
        error: OtherError  # Last assignment wins in Python

        async def execute(self, db, input_data) -> None:
            return CreateUserSuccess(user_id=4)

    # Should use 'error' (last assignment wins)
    assert CreateUserBoth.__fraiseql_mutation__.error_type == OtherError
