"""Tests for simple mutations in quickstart examples."""

from datetime import UTC, datetime

import pytest

import fraiseql
from fraiseql import fraise_field
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

    # Clear global dependencies that might be set by create_fraiseql_app
    import fraiseql.fastapi.app
    import fraiseql.fastapi.dependencies as deps

    deps._db_pool = None
    deps._auth_provider = None
    deps._fraiseql_config = None
    fraiseql.fastapi.app._global_turbo_registry = None

    yield

    registry.clear()
    _graphql_type_cache.clear()
    deps._db_pool = None
    deps._auth_provider = None
    deps._fraiseql_config = None
    fraiseql.fastapi.app._global_turbo_registry = None


# Types from the quickstart example that failed
@fraiseql.type
class Branch:
    """A branch pointing to a specific commit."""

    name: str = fraise_field(description="Branch name")
    commit_hash: str = fraise_field(description="Current commit hash")
    created_at: datetime
    updated_at: datetime


@fraiseql.input
class CreateBranchInput:
    name: str
    commit_hash: str


# This is the exact mutation pattern from the quickstart that failed in 0.1.0a5
@fraiseql.mutation
async def create_branch(info, input: CreateBranchInput) -> Branch:
    """Create a new branch."""
    return Branch(
        name=input.name,
        commit_hash=input.commit_hash,
        created_at=datetime.now(UTC),
        updated_at=datetime.now(UTC),
    )


def test_todo_quickstart_mutation_pattern_works() -> None:
    """Test that the mutation pattern from todo_quickstart.py works."""
    # This should not raise "TypeError: Mutation create_branch must define 'success' type"
    assert hasattr(create_branch, "__fraiseql_mutation__")
    assert create_branch.__fraiseql_mutation__ is True
    assert create_branch.__fraiseql_resolver__ is create_branch


@pytest.mark.asyncio
async def test_quickstart_mutation_can_execute() -> None:
    """Test that quickstart mutations can be executed."""
    # Create test input
    test_input = CreateBranchInput(name="feature/test", commit_hash="abc123")

    # Mock info
    class MockInfo:
        context = {"db": None}

    # This should work without errors
    result = await create_branch(MockInfo(), test_input)

    assert isinstance(result, Branch)
    assert result.name == "feature/test"
    assert result.commit_hash == "abc123"
    assert isinstance(result.created_at, datetime)
    assert isinstance(result.updated_at, datetime)


def test_simple_mutation_in_schema() -> None:
    """Test that simple mutations are correctly added to GraphQL schema."""
    from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema

    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Register types
    registry.register_type(Branch)

    # Re-register the mutation since we cleared the registry
    registry.register_mutation(create_branch)

    # The mutation should be registered (with camelCase)
    assert "createBranch" in registry._mutations or "create_branch" in registry._mutations

    # Add a dummy query
    @fraiseql.query
    async def dummy(info) -> str:
        return "test"

    registry.register_query(dummy)

    # Build schema - this should NOT raise an error about missing 'success' type
    try:
        schema = build_fraiseql_schema()
    except TypeError as e:
        if "must define 'success' type" in str(e):
            pytest.fail(f"Simple mutations should not require success type: {e}")
        raise

    # Verify mutation is in schema
    assert schema.mutation_type is not None
    assert "createBranch" in schema.mutation_type.fields


def test_quickstart_app_creation() -> None:
    """Test that quickstart app can be created with simple mutations."""
    from unittest.mock import patch

    from fraiseql import create_fraiseql_app
    from fraiseql.gql.schema_builder import SchemaRegistry

    # Clear registry
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Define minimal types and queries for app
    @fraiseql.type
    class Commit:
        hash: str
        message: str

    @fraiseql.query
    async def commits(info) -> list[Commit]:
        return []

    # Import our mutations
    # (In real quickstart, these would be imported from the file)

    # Mock database pool creation to avoid actual database connections
    with (
        patch("fraiseql.fastapi.app.create_db_pool") as mock_pool,
        patch("fraiseql.fastapi.app.set_db_pool"),
        patch("fraiseql.fastapi.app.set_auth_provider"),
        patch("fraiseql.fastapi.app.set_fraiseql_config"),
    ):
        # Mock the pool to avoid async issues
        mock_pool.return_value = None

        # This should work without any errors
        app = create_fraiseql_app(
            types=[Branch, Commit],
            production=False,
            database_url="postgresql://test:test@localhost:5432/test",
        )

        assert app is not None


def test_both_mutation_styles_in_same_app() -> None:
    """Test that both mutation styles can coexist."""
    from fraiseql.gql.schema_builder import SchemaRegistry

    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Simple mutation
    @fraiseql.mutation
    async def simple_mutation(info, name: str) -> Branch:
        return Branch(
            name=name,
            commit_hash="test",
            created_at=datetime.now(UTC),
            updated_at=datetime.now(UTC),
        )

    # Class-based mutation
    @fraiseql.success
    class CreateSuccess:
        branch: Branch
        message: str

    @fraiseql.error
    class CreateError:
        message: str

    @fraiseql.mutation
    class ClassBasedMutation:
        input: CreateBranchInput
        success: CreateSuccess
        error: CreateError

    # Register both
    registry.register_mutation(simple_mutation)
    registry.register_mutation(ClassBasedMutation)

    # Both should be registered
    assert "simple_mutation" in registry._mutations
    assert "class_based_mutation" in registry._mutations
