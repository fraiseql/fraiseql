"""Tests for JSON and dict[str, Any] type support in FraiseQL.

Following TDD: these tests will initially fail, then we'll implement
the features to make them pass.
"""

from typing import Any
from uuid import UUID

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry


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


class TestJSONTypeSupport:
    """Test that FraiseQL supports JSON/dict types properly."""

    def test_dict_str_any_field_type(self) -> None:
        """Test that dict[str, Any] fields work in types."""

        @fraiseql.type
        class ConfigData:
            id: UUID
            name: str
            settings: dict[str, Any]  # Should work as JSON type
            metadata: dict[str, Any]  # Both syntaxes should work

        @fraiseql.query
        async def get_config(info) -> ConfigData:
            return ConfigData(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                name="App Config",
                settings={
                    "theme": "dark",
                    "language": "en",
                    "features": ["feature1", "feature2"],
                    "limits": {"max_users": 100, "max_storage": 1000},
                },
                metadata={"version": "1.0.0", "created_by": "admin"},
            )

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[ConfigData],
            queries=[get_config],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getConfig {
                                id
                                name
                                settings
                                metadata
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            config = data["data"]["getConfig"]
            assert config["settings"]["theme"] == "dark"
            assert config["settings"]["features"] == ["feature1", "feature2"]
            assert config["metadata"]["version"] == "1.0.0"

    def test_json_scalar_type(self) -> None:
        """Test that JSON scalar type is available and works."""

        @fraiseql.type
        class Document:
            id: UUID
            title: str
            content: fraiseql.JSON  # Should work as a JSON scalar

        @fraiseql.query
        async def get_document(info) -> Document:
            return Document(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                title="Complex Document",
                content={
                    "sections": [
                        {"title": "Introduction", "text": "Welcome"},
                        {"title": "Main", "text": "Content here"},
                    ],
                    "metadata": {"author": "John Doe", "tags": ["important", "draft"]},
                },
            )

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[Document],
            queries=[get_document],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getDocument {
                                id
                                title
                                content
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            doc = data["data"]["getDocument"]
            assert len(doc["content"]["sections"]) == 2
            assert doc["content"]["metadata"]["author"] == "John Doe"

    def test_json_input_type(self) -> None:
        """Test that JSON/dict can be used in input types."""

        @fraiseql.input
        class CreateDocumentInput:
            title: str
            content: dict[str, Any]
            tags: list[str]

        @fraiseql.type
        class Document:
            id: UUID
            title: str
            content: dict[str, Any]
            tags: list[str]

        @fraiseql.mutation
        async def create_document(info, input: CreateDocumentInput) -> Document:
            return Document(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                title=input.title,
                content=input.content,
                tags=input.tags,
            )

        @fraiseql.query
        async def version(info) -> str:
            return "1.0.0"

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[Document],
            queries=[version],
            mutations=[create_document],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        mutation {
                            createDocument(input: {
                                title: "New Document"
                                content: "{\\"text\\": \\"Hello World\\", \\"format\\": \\"markdown\\", \\"metadata\\": {\\"created\\": \\"2024-01-01\\"}}"  # noqa: E501
                                tags: ["new", "important"]
                            }) {
                                id
                                title
                                content
                                tags
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            doc = data["data"]["createDocument"]
            assert doc["title"] == "New Document"
            assert doc["content"]["text"] == "Hello World"
            assert doc["content"]["metadata"]["created"] == "2024-01-01"
            assert doc["tags"] == ["new", "important"]

    def test_nested_json_structures(self) -> None:
        """Test deeply nested JSON structures."""

        @fraiseql.type
        class APIResponse:
            status: str
            data: dict[str, Any]

        @fraiseql.query
        async def get_api_response(info) -> APIResponse:
            return APIResponse(
                status="success",
                data={
                    "users": [
                        {
                            "id": 1,
                            "name": "Alice",
                            "preferences": {
                                "notifications": {
                                    "email": True,
                                    "sms": False,
                                    "push": {"enabled": True, "categories": ["updates", "alerts"]},
                                }
                            },
                        }
                    ],
                    "pagination": {"page": 1, "total": 100, "per_page": 10},
                },
            )

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[APIResponse],
            queries=[get_api_response],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getApiResponse {
                                status
                                data
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            api_response = data["data"]["getApiResponse"]
            assert api_response["status"] == "success"

            # Test nested access
            users = api_response["data"]["users"]
            assert users[0]["preferences"]["notifications"]["push"]["enabled"] is True
            assert "updates" in users[0]["preferences"]["notifications"]["push"]["categories"]

    def test_optional_json_fields(self) -> None:
        """Test that Optional JSON fields work correctly."""

        @fraiseql.type
        class User:
            id: UUID
            name: str
            preferences: dict[str, Any] | None = None
            metadata: dict[str, Any] | None = None

        @fraiseql.query
        async def getUsers(info) -> list[User]:
            return [
                User(
                    id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                    name="User with preferences",
                    preferences={"theme": "dark"},
                    metadata={"role": "admin"},
                ),
                User(
                    id=UUID("223e4567-e89b-12d3-a456-426614174001"),
                    name="User without preferences",
                    preferences=None,
                    metadata=None,
                ),
            ]

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[User],
            queries=[getUsers],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query { getUsers {
                                id
                                name
                                preferences
                                metadata
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            users = data["data"]["getUsers"]
            assert len(users) == 2

            # First user has preferences
            assert users[0]["preferences"] == {"theme": "dark"}
            assert users[0]["metadata"] == {"role": "admin"}

            # Second user has null preferences
            assert users[1]["preferences"] is None
            assert users[1]["metadata"] is None

    def test_json_field_resolver(self) -> None:
        """Test that field resolvers can return JSON data."""

        @fraiseql.type
        class Product:
            id: UUID
            name: str

            @fraiseql.field
            async def specifications(self, info) -> dict[str, Any]:
                """Dynamic specifications as JSON."""
                return {
                    "dimensions": {"width": 10, "height": 20, "depth": 5},
                    "weight": 1.5,
                    "materials": ["plastic", "metal"],
                    "certifications": {"CE": True, "RoHS": True},
                }

        @fraiseql.query
        async def getProduct(info) -> Product:
            return Product(id=UUID("123e4567-e89b-12d3-a456-426614174000"), name="Widget Pro")

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[Product],
            queries=[getProduct],
            production=False,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getProduct {
                                id
                                name
                                specifications
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data

            product = data["data"]["getProduct"]
            specs = product["specifications"]
            assert specs["dimensions"]["width"] == 10
            assert "plastic" in specs["materials"]
            assert specs["certifications"]["CE"] is True


class TestJSONValidation:
    """Test JSON type validation and error handling."""

    def test_json_validation_in_mutations(self) -> None:
        """Test that invalid JSON structures are handled properly."""

        @fraiseql.input
        class UpdateSettingsInput:
            user_id: UUID
            settings: dict[str, Any]

        @fraiseql.type
        class Result:
            success: bool
            message: str

        @fraiseql.mutation
        async def updateSettings(info, input: UpdateSettingsInput) -> Result:
            # Validate the settings structure
            if "theme" not in input.settings:
                return Result(success=False, message="Settings must include 'theme'")

            return Result(success=True, message="Settings updated")

        @fraiseql.query
        async def version(info) -> str:
            return "1.0.0"

        app = create_fraiseql_app(
            database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
            types=[Result],
            queries=[version],
            mutations=[updateSettings],
            production=False,
        )

        with TestClient(app) as client:
            # Test with valid settings
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        mutation {
                            updateSettings(input: {
                                userId: "123e4567-e89b-12d3-a456-426614174000"
                                settings: "{\\"theme\\": \\"dark\\", \\"language\\": \\"en\\"}"
                            }) {
                                success
                                message
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data, f"Response had errors: {data}"
            assert data["data"]["updateSettings"]["success"] is True

            # Test with invalid settings
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        mutation {
                            updateSettings(input: {
                                userId: "123e4567-e89b-12d3-a456-426614174000"
                                settings: "{\\"language\\": \\"en\\"}"
                            }) {
                                success
                                message
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()
            result = data["data"]["updateSettings"]
            assert result["success"] is False
            assert "theme" in result["message"]
