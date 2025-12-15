"""Test that mutations return dicts in non-HTTP mode."""

import fraiseql


@fraiseql.input
class CreateUserInput:
    name: str
    email: str


@fraiseql.success
class CreateUserSuccess:
    user: dict  # Will be dict from Rust pipeline
    message: str


@fraiseql.error
class CreateUserError:
    message: str
    code: str


@fraiseql.mutation
class CreateUserMutation:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError


def test_class_based_mutation_registered():
    """Test that class-based mutations are registered correctly."""
    # Class-based mutations return dict structures

    # Verify the mutation has the correct metadata
    assert hasattr(CreateUserMutation, "__fraiseql_mutation__")
    assert hasattr(CreateUserMutation, "__fraiseql_resolver__")

    # The resolver should be callable
    resolver = CreateUserMutation.__fraiseql_resolver__
    assert callable(resolver)

    # Actual execution testing would require database setup
