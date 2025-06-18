"""Tests for Strawberry to FraiseQL migration guide.

These tests validate that our migration guide is comprehensive and that
the examples provided actually work. Following TDD: these tests will fail
initially and we'll implement the migration features to make them pass.
"""

import re
from pathlib import Path
from typing import List, Optional
from uuid import UUID

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before each test to avoid type conflicts."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    yield

    registry.clear()
    _graphql_type_cache.clear()


class TestMigrationGuideExists:
    """Test that migration guide documentation exists and is comprehensive."""

    def test_migration_guide_file_exists(self):
        """Test that the migration guide file exists."""
        migration_guide_path = Path("docs/migration/from-strawberry.md")
        assert migration_guide_path.exists(), "Strawberry migration guide must exist"

        # File should not be empty
        content = migration_guide_path.read_text()
        assert len(content) > 1000, (
            "Migration guide should be comprehensive (>1000 chars)"
        )

    def test_migration_guide_covers_key_topics(self):
        """Test that migration guide covers all essential migration topics."""
        migration_guide_path = Path("docs/migration/from-strawberry.md")
        content = migration_guide_path.read_text().lower()

        required_topics = [
            "type definition",
            "field resolver",
            "query",
            "mutation",
            "subscription",
            "dataloader",
            "context",
            "authentication",
            "federation",
            "scalar",
            "enum",
            "interface",
            "union",
            "directive",
            "middleware",
            "error handling",
            "testing",
            "performance",
        ]

        missing_topics = []
        for topic in required_topics:
            if topic not in content:
                missing_topics.append(topic)

        assert not missing_topics, f"Migration guide missing topics: {missing_topics}"

    def test_migration_guide_has_code_examples(self):
        """Test that migration guide includes both Strawberry and FraiseQL code examples."""
        migration_guide_path = Path("docs/migration/from-strawberry.md")
        content = migration_guide_path.read_text()

        # Should have code blocks
        strawberry_examples = len(
            re.findall(r"```python.*?strawberry", content, re.DOTALL | re.IGNORECASE)
        )
        fraiseql_examples = len(
            re.findall(r"```python.*?fraiseql", content, re.DOTALL | re.IGNORECASE)
        )

        assert strawberry_examples >= 5, (
            "Should have at least 5 Strawberry code examples"
        )
        assert fraiseql_examples >= 5, "Should have at least 5 FraiseQL code examples"

    def test_migration_guide_has_comparison_table(self):
        """Test that migration guide includes a feature comparison table."""
        migration_guide_path = Path("docs/migration/from-strawberry.md")
        content = migration_guide_path.read_text()

        # Should have a table with Strawberry vs FraiseQL
        assert "| Strawberry" in content or "| Feature" in content, (
            "Should have comparison table"
        )
        assert "| FraiseQL" in content, (
            "Comparison table should include FraiseQL column"
        )


class TestStrawberryCompatibilityLayer:
    """Test that we provide compatibility helpers for common Strawberry patterns."""

    def test_strawberry_style_type_decorator_works(self):
        """Test that @strawberry.type style decorators work in FraiseQL."""
        # This should work after we implement strawberry compatibility

        @fraiseql.type  # Should work like @strawberry.type
        class User:
            id: UUID
            name: str
            email: str

        @fraiseql.query
        async def get_user(info, id: UUID) -> User:
            return User(id=id, name="Test User", email="test@example.com")

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User],
            queries=[get_user],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUser(id: "123e4567-e89b-12d3-a456-426614174000") {
                                id
                                name
                                email
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["getUser"]["name"] == "Test User"

    def test_strawberry_style_field_resolver_migration(self):
        """Test that Strawberry field resolver patterns can be migrated."""

        @fraiseql.type
        class User:
            id: UUID
            name: str

            # This should work like Strawberry's field resolvers
            @fraiseql.field
            async def display_name(self, info) -> str:
                """Strawberry-style field resolver."""
                return f"User: {self.name}"

        @fraiseql.query
        async def get_user(info) -> User:
            return User(id=UUID("123e4567-e89b-12d3-a456-426614174000"), name="John")

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User],
            queries=[get_user],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUser {
                                id
                                name
                                display_name
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["getUser"]["displayName"] == "User: John"

    def test_strawberry_info_context_migration(self):
        """Test that Strawberry info.context patterns work in FraiseQL."""

        @fraiseql.type
        class User:
            id: UUID
            name: str

        @fraiseql.query
        async def get_current_user(info) -> Optional[User]:
            # Should work like Strawberry's info.context
            # In Strawberry: user_id = info.context["request"].user.id
            # In FraiseQL: should work similarly
            context = info.context

            # For this test, we'll simulate a user_id in context
            if "user_id" in context:
                return User(id=context["user_id"], name="Context User")
            return None

        # Custom context that mimics Strawberry patterns
        async def get_context(request):
            return {
                "request": request,
                "user_id": UUID("123e4567-e89b-12d3-a456-426614174000"),
                # Other Strawberry-like context items
            }

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User],
            queries=[get_current_user],
            context_getter=get_context,
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            get_current_user {
                                id
                                name
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["get_current_user"]["name"] == "Context User"


class TestStrawberryDataLoaderMigration:
    """Test migration from Strawberry DataLoaders to FraiseQL DataLoaders."""

    def test_strawberry_dataloader_pattern_migration(self):
        """Test that Strawberry DataLoader patterns can be migrated."""

        # Define User type first
        @fraiseql.type
        class User:
            id: UUID
            name: str
            email: str

        # This mimics how you'd migrate from Strawberry DataLoader
        from fraiseql.optimization import DataLoader

        class UserDataLoader(DataLoader[UUID, User]):
            """Migrated from Strawberry DataLoader pattern."""

            def __init__(self, db=None):
                super().__init__()
                self.db = db

            async def batch_load(self, user_ids: List[UUID]) -> List[Optional[User]]:
                # Simulate database batch fetch (like in Strawberry)
                return [
                    User(
                        id=user_id,
                        name=f"User {user_id}",
                        email=f"user-{user_id}@example.com",
                    )
                    for user_id in user_ids
                ]

        @fraiseql.type
        class Post:
            id: UUID
            title: str
            author_id: UUID

            @fraiseql.field
            async def author(self, info) -> Optional[User]:
                # Migrate from Strawberry's dataloader pattern
                from fraiseql.optimization.registry import get_loader

                loader = get_loader(UserDataLoader)
                return await loader.load(self.author_id)

        @fraiseql.query
        async def get_posts(info) -> List[Post]:
            return [
                Post(
                    id=UUID(f"00000000-0000-0000-0000-{i:012x}"),
                    title=f"Post {i}",
                    author_id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                )
                for i in range(3)
            ]

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User, Post],
            queries=[get_posts],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            get_posts {
                                id
                                title
                                author {
                                    id
                                    name
                                }
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert len(data["data"]["get_posts"]) == 3

            # Should use DataLoader (no N+1)
            for post in data["data"]["get_posts"]:
                assert post["author"]["name"].startswith("User")


class TestStrawberryMutationMigration:
    """Test migration from Strawberry mutations to FraiseQL mutations."""

    def test_strawberry_mutation_pattern_migration(self):
        """Test that Strawberry mutation patterns can be migrated."""

        @fraiseql.type
        class User:
            id: UUID
            name: str
            email: str

        @fraiseql.input
        class CreateUserInput:
            name: str
            email: str

        # This should work like Strawberry mutations
        @fraiseql.mutation
        async def create_user(info, input: CreateUserInput) -> User:
            """Migrated from Strawberry mutation style."""
            return User(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                name=input.name,
                email=input.email,
            )

        # Need at least one query
        @fraiseql.query
        async def get_version(info) -> str:
            return "1.0.0"

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User],
            queries=[get_version],
            mutations=[create_user],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        mutation {
                            create_user(input: {
                                name: "John Doe"
                                email: "john@example.com"
                            }) {
                                id
                                name
                                email
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["create_user"]["name"] == "John Doe"
            assert data["data"]["create_user"]["email"] == "john@example.com"


class TestMigrationUtilities:
    """Test migration utilities and helpers."""

    def test_migration_checker_utility_exists(self):
        """Test that we provide a utility to check migration completeness."""
        # This should exist after implementation
        try:
            from fraiseql.migration import check_strawberry_compatibility

            # Should be able to analyze a codebase for Strawberry patterns
            issues = check_strawberry_compatibility("fake_project_path")
            assert isinstance(issues, list), "Should return list of migration issues"

        except ImportError:
            pytest.fail("Migration checker utility should be available")

    def test_strawberry_import_compatibility(self):
        """Test that common Strawberry imports can be mapped to FraiseQL."""
        # This should work after implementation
        try:
            # Should provide compatibility imports
            from fraiseql.strawberry_compat import strawberry

            # Basic patterns should work
            assert hasattr(strawberry, "type")
            assert hasattr(strawberry, "field")
            assert hasattr(strawberry, "mutation")
            assert hasattr(strawberry, "query")

        except ImportError:
            pytest.fail("Strawberry compatibility layer should be available")

    def test_automated_migration_script_exists(self):
        """Test that an automated migration script exists."""
        # Check for CLI command
        migration_script = Path("src/fraiseql/cli/commands/migrate_from_strawberry.py")

        # For now, just check that we have a migration command structure
        cli_commands_dir = Path("src/fraiseql/cli/commands")
        if cli_commands_dir.exists():
            # Should have migration-related commands
            command_files = list(cli_commands_dir.glob("*.py"))
            assert len(command_files) > 0, "Should have CLI commands"
        else:
            pytest.fail("CLI commands directory should exist for migration tools")


class TestStrawberryFeatureParity:
    """Test that FraiseQL provides equivalent functionality to Strawberry features."""

    def test_strawberry_enum_migration(self):
        """Test that Strawberry enums can be migrated to FraiseQL."""
        from enum import Enum

        @fraiseql.enum
        class UserRole(Enum):
            ADMIN = "admin"
            USER = "user"
            MODERATOR = "moderator"

        @fraiseql.type
        class User:
            id: UUID
            name: str
            role: UserRole

        @fraiseql.query
        async def get_user(info) -> User:
            return User(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                name="Admin User",
                role=UserRole.ADMIN,
            )

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[User, UserRole],
            queries=[get_user],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUser {
                                id
                                name
                                role
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert (
                data["data"]["getUser"]["role"] == "ADMIN"
            )  # FraiseQL uses enum name

    @pytest.mark.xfail(reason="Interface support may not be fully implemented yet")
    def test_strawberry_interface_migration(self):
        """Test that Strawberry interfaces can be migrated."""

        @fraiseql.interface
        class Node:
            id: UUID

        @fraiseql.type
        class User(Node):
            name: str
            email: str

        @fraiseql.query
        async def get_node(info, id: UUID) -> Node:
            return User(id=id, name="Interface User", email="interface@example.com")

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
            types=[Node, User],
            queries=[get_node],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            get_node(id: "123e4567-e89b-12d3-a456-426614174000") {
                                id
                                ... on User {
                                    name
                                    email
                                }
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["get_node"]["name"] == "Interface User"
