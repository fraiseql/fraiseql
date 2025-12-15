"""Test both simple and class-based mutation patterns."""

from datetime import UTC, datetime

import pytest

import fraiseql
from fraiseql.gql.schema_builder import SchemaRegistry

pytestmark = pytest.mark.integration


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test to avoid type conflicts."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    yield

    registry.clear()
    _graphql_type_cache.clear()


# Test types
@fraiseql.type
class User:
    id: int
    name: str
    email: str
    created_at: datetime


@fraiseql.input
class CreateUserInput:
    name: str
    email: str


# Test 1: Simple function-based mutation (should work in 0.1.0a5+)
@fraiseql.mutation
async def create_user_simple(info, input: CreateUserInput) -> User:
    """Simple mutation that returns the type directly."""
    return User(id=1, name=input.name, email=input.email, created_at=datetime.now(UTC))


# Test 2: Class-based mutation with success/error pattern
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"


@fraiseql.error
class CreateUserError:
    message: str
    code: str


@fraiseql.mutation
class CreateUserClassBased:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError


# Test that both patterns can be registered
def test_both_mutation_patterns_can_be_registered() -> None:
    """Test that both simple and class-based mutations can be registered."""
    from fraiseql.gql.schema_builder import SchemaRegistry

    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Register the simple mutation
    registry.register_mutation(create_user_simple)

    # Register the class-based mutation
    registry.register_mutation(CreateUserClassBased)

    # Both should be in the mutations registry
    assert "create_user_simple" in registry._mutations
    # Class-based mutations are registered with snake_case names
    assert "create_user_class_based" in registry._mutations

    # The simple mutation should be the function itself
    assert registry._mutations["create_user_simple"] == create_user_simple

    # The class-based mutation should have a resolver
    assert callable(registry._mutations["create_user_class_based"])


@pytest.mark.asyncio
async def test_simple_mutation_execution() -> None:
    """Test that simple mutations can be executed."""
    # Create test input
    test_input = CreateUserInput(name="John Doe", email="john@example.com")

    # Mock info object
    class MockInfo:
        context = {"db": None}

    # Execute the mutation
    result = await create_user_simple(MockInfo(), test_input)

    # Verify result
    assert isinstance(result, User)
    assert result.name == "John Doe"
    assert result.email == "john@example.com"
    assert result.id == 1


def test_simple_mutation_has_correct_metadata() -> None:
    """Test that simple mutations have the correct metadata."""
    assert hasattr(create_user_simple, "__fraiseql_mutation__")
    assert create_user_simple.__fraiseql_mutation__ is True
    assert hasattr(create_user_simple, "__fraiseql_resolver__")
    assert create_user_simple.__fraiseql_resolver__ is create_user_simple


def test_class_based_mutation_has_correct_metadata() -> None:
    """Test that class-based mutations have the correct metadata."""
    assert hasattr(CreateUserClassBased, "__fraiseql_mutation__")
    assert hasattr(CreateUserClassBased, "__fraiseql_resolver__")

    # The resolver should be a different function
    assert CreateUserClassBased.__fraiseql_resolver__ is not CreateUserClassBased
    assert callable(CreateUserClassBased.__fraiseql_resolver__)


def test_schema_builder_handles_both_patterns() -> None:
    """Test that schema builder can handle both mutation patterns."""
    from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema

    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Register types
    registry.register_type(User)
    registry.register_type(CreateUserSuccess)
    registry.register_type(CreateUserError)

    # Register mutations
    registry.register_mutation(create_user_simple)
    registry.register_mutation(CreateUserClassBased)

    # Add a dummy query to satisfy schema requirements
    @fraiseql.query
    async def dummy_query(info) -> str:
        return "dummy"

    registry.register_query(dummy_query)

    # Build schema - should not raise any errors
    schema = build_fraiseql_schema()

    # Check that both mutations are in the schema
    mutation_type = schema.mutation_type
    assert mutation_type is not None

    fields = mutation_type.fields
    assert "createUserSimple" in fields
    assert "createUserClassBased" in fields
