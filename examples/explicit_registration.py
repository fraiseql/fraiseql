"""Example of explicit registration pattern for FraiseQL.

This example shows how to use the enhanced architecture that reduces
import order dependencies and provides better testability.
"""


import fraiseql
from fraiseql.db import FraiseQLRepository
from fraiseql.mutations.decorators_v2 import create_mutations
from fraiseql.mutations.registry_v2 import ScopedResultRegistry, isolated_registry


# Define types
@fraiseql.type
class User:
    id: int
    name: str
    email: str
    active: bool = True


@fraiseql.input
class CreateUserInput:
    name: str
    email: str


@fraiseql.type
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"


@fraiseql.type
class CreateUserError:
    code: str
    message: str
    field: str | None = None


# Example 1: Using explicit registration
def setup_mutations_explicit(registry: ScopedResultRegistry):
    """Set up mutations with explicit registration."""
    # Create mutation builder with injected registry
    builder = create_mutations(registry)

    @builder.mutation(
        result_type=CreateUserSuccess,
        error_type=CreateUserError,
        sql_function="users.create_user",
        description="Create a new user",
    )
    async def create_user(input: CreateUserInput, repo: FraiseQLRepository):
        # Implementation would go here
        pass

    @builder.mutation(
        result_type=UpdateUserSuccess,
        error_type=UpdateUserError,
        sql_function="users.update_user",
    )
    async def update_user(id: int, input: UpdateUserInput, repo: FraiseQLRepository):
        # Implementation would go here
        pass

    return builder


# Example 2: Using isolated registry for testing
async def test_user_mutations():
    """Test mutations with isolated registry."""
    # Create isolated registry for test
    with isolated_registry() as registry:
        # Set up mutations in isolated scope
        builder = setup_mutations_explicit(registry)

        # Registry is isolated - won't affect other tests
        assert registry.get_error_type(CreateUserSuccess) == CreateUserError

        # Test mutations...
        mutations = builder.get_mutations()
        assert "create_user" in mutations
        assert mutations["create_user"].sql_function == "users.create_user"

    # Registry is cleaned up automatically


# Example 3: Application setup with dependency injection
class Application:
    """Application with explicit dependency management."""

    def __init__(self):
        self.registry = ScopedResultRegistry()
        self.mutation_builders = []

    def register_mutations(self, setup_func):
        """Register mutations using a setup function."""
        builder = setup_func(self.registry)
        self.mutation_builders.append(builder)
        return builder

    def get_all_mutations(self):
        """Get all registered mutations."""
        all_mutations = {}
        for builder in self.mutation_builders:
            all_mutations.update(builder.get_mutations())
        return all_mutations

    def create_schema(self):
        """Create GraphQL schema with registered types."""
        # Schema creation would use the registry and builders


# Example 4: Module-based organization
# users/mutations.py
def setup_user_mutations(registry: ScopedResultRegistry):
    """Set up user-related mutations."""
    return create_mutations(registry)

    # Register user mutations...


# posts/mutations.py
def setup_post_mutations(registry: ScopedResultRegistry):
    """Set up post-related mutations."""
    return create_mutations(registry)

    # Register post mutations...


# main.py
def create_app():
    """Create application with explicit registration."""
    app = Application()

    # Register mutations from different modules
    app.register_mutations(setup_user_mutations)
    app.register_mutations(setup_post_mutations)

    # Create schema after all registrations
    schema = app.create_schema()

    return app, schema


# Example 5: Testing with different configurations
async def test_with_mock_registry():
    """Test with a mock registry."""

    class MockRegistry:
        def __init__(self):
            self.registered = []

        def register(self, success_cls, error_cls):
            self.registered.append((success_cls, error_cls))

        def get_error_type(self, success_cls):
            for s, e in self.registered:
                if s == success_cls:
                    return e
            return None

    # Use mock registry for testing
    mock_registry = MockRegistry()
    builder = create_mutations(mock_registry)

    # Test registration behavior
    @builder.mutation(
        result_type=CreateUserSuccess,
        error_type=CreateUserError,
    )
    async def test_mutation(input):
        pass

    assert len(mock_registry.registered) == 1
    assert mock_registry.registered[0] == (CreateUserSuccess, CreateUserError)


# Additional type definitions for examples
@fraiseql.type
class UpdateUserSuccess:
    user: User
    message: str = "User updated successfully"


@fraiseql.type
class UpdateUserError:
    code: str
    message: str


@fraiseql.input
class UpdateUserInput:
    name: str | None = None
    email: str | None = None
    active: bool | None = None


if __name__ == "__main__":
    # Example usage
    import asyncio

    # Create app with explicit registration
    app, schema = create_app()

    # Run tests
    asyncio.run(test_user_mutations())
    asyncio.run(test_with_mock_registry())
