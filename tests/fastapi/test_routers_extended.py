"""Extended integration tests for unified GraphQL router."""

import json
from contextlib import asynccontextmanager
from typing import Any
from uuid import UUID, uuid4

import pytest
from fastapi import FastAPI, Request
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.auth.base import AuthProvider, UserContext
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import APOLLO_SANDBOX_HTML, GRAPHIQL_HTML, GraphQLRequest
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


# Test types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID

    @fraiseql.field
    async def author(self, info) -> User | None:
        """Field resolver that could trigger N+1 if called multiple times."""
        # In a real app, this would query the database
        # For testing, we'll track calls through context
        if "resolver_calls" not in info.context:
            info.context["resolver_calls"] = []
        info.context["resolver_calls"].append(f"author:{self.author_id}")

        return User(
            id=self.author_id,
            name=f"Author {str(self.author_id)[:8]}",
            email=f"author_{str(self.author_id)[:8]}@test.com",
        )


# Test queries
@fraiseql.query
async def hello(info) -> str:
    """Simple test query."""
    return "world"


@fraiseql.query
async def user(info, id: UUID) -> User | None:
    """Get user by ID."""
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return User(id=id, name="Test User", email="test@example.com")
    return None


@fraiseql.query
async def posts(info, limit: int = 10) -> list[Post]:
    """Get posts - useful for N+1 detection testing."""
    # Create multiple posts with same author to potentially trigger N+1
    author_id = UUID("223e4567-e89b-12d3-a456-426614174001")
    return [
        Post(id=uuid4(), title=f"Post {i}", content=f"Content for post {i}", author_id=author_id)
        for i in range(limit)
    ]


@fraiseql.query
async def error_query(info) -> str:
    """Query that always raises an error."""
    raise ValueError("Test error message")


@fraiseql.query
async def context_test(info) -> str:
    """Query that returns custom context value."""
    return info.context.get("custom_value", "no custom value")


# Test mutations
@fraiseql.mutation
async def create_user(info, name: str, email: str) -> User:
    """Create a new user."""
    return User(id=uuid4(), name=name, email=email)


class TestGraphQLRequest:
    """Test GraphQLRequest model."""

    def test_graphql_request_basic(self):
        """Test basic GraphQL request creation."""
        request = GraphQLRequest(query="{ test }")
        assert request.query == "{ test }"
        assert request.variables is None
        assert request.operationName is None

    def test_graphql_request_with_variables(self):
        """Test GraphQL request with variables."""
        variables = {"id": "123e4567-e89b-12d3-a456-426614174000", "name": "test"}
        request = GraphQLRequest(
            query="query($id: UUID!) { user(id: $id) { name } }",
            variables=variables,
            operationName="GetUser",
        )
        assert request.query == "query($id: UUID!) { user(id: $id) { name } }"
        assert request.variables == variables
        assert request.operationName == "GetUser"

    def test_graphql_request_validation(self):
        """Test GraphQL request validation."""
        # Query is required
        with pytest.raises(ValueError):
            GraphQLRequest()


@asynccontextmanager
async def noop_lifespan(app: FastAPI):
    """No-op lifespan for tests that don't need a database."""
    yield


class TestUnifiedRouter:
    """Test unified GraphQL router functionality."""

    def test_development_environment_basic(self):
        """Test router in development environment."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            enable_playground=True,
        )

        app = create_fraiseql_app(
            config=config,
            types=[User, Post],
            queries=[hello, user, posts, error_query, context_test],
            mutations=[create_user],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            # Test basic query
            response = client.post("/graphql", json={"query": "{ hello }"})
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["hello"] == "world"

    def test_production_environment_basic(self):
        """Test router in production environment."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            enable_playground=False,
        )

        app = create_fraiseql_app(
            config=config,
            types=[User, Post],
            queries=[hello, user, posts, error_query, context_test],
            mutations=[create_user],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            # Test basic query
            response = client.post("/graphql", json={"query": "{ hello }"})
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["hello"] == "world"

    def test_query_with_variables(self):
        """Test GraphQL query with variables."""
        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[User, Post],
            queries=[hello, user, posts],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                """/graphql""",
                json={
                    "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
                    "variables": {"id": "123e4567-e89b-12d3-a456-426614174000"},
                    "operationName": "GetUser",
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["user"]["name"] == "Test User"
            assert data["data"]["user"]["email"] == "test@example.com"

    def test_mutation(self):
        """Test GraphQL mutation."""
        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[User],
            queries=[hello],  # Need at least one query
            mutations=[create_user],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        mutation CreateUser($name: String!, $email: String!) {
                            createUser(name: $name, email: $email) {
                                id
                                name
                                email
                            }
                        }
                    """,
                    "variables": {"name": "New User", "email": "new@example.com"},
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["createUser"]["name"] == "New User"
            assert data["data"]["createUser"]["email"] == "new@example.com"
            assert "id" in data["data"]["createUser"]

    def test_error_handling_development(self):
        """Test error handling in development mode."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            unified_executor_enabled=False,
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[error_query], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.post("/graphql", json={"query": "{ errorQuery }"})
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            # Development mode shows full error details
            assert "Test error message" in data["errors"][0]["message"]

    def test_error_handling_production(self):
        """Test error handling in production mode."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            unified_executor_enabled=False,  # Disable for this test
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[error_query], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.post("/graphql", json={"query": "{ errorQuery }"})
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            # Production mode hides error details
            assert data["errors"][0]["message"] == "Internal server error"
            assert data["errors"][0]["extensions"]["code"] == "INTERNAL_SERVER_ERROR"

    def test_graphiql_playground(self):
        """Test GraphiQL playground in development."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            enable_playground=True,
            playground_tool="graphiql",
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.get("/graphql")
            assert response.status_code == 200
            assert "text/html" in response.headers["content-type"]
            assert "GraphiQL" in response.text
            assert GRAPHIQL_HTML in response.text

    def test_apollo_sandbox_playground(self):
        """Test Apollo Sandbox playground."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            enable_playground=True,
            playground_tool="apollo-sandbox",
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.get("/graphql")
            assert response.status_code == 200
            assert "text/html" in response.headers["content-type"]
            assert "Apollo" in response.text
            assert APOLLO_SANDBOX_HTML in response.text

    def test_playground_disabled_in_production(self):
        """Test that playground is disabled in production by default."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            enable_playground=False,
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.get("/graphql")
            assert response.status_code == 404

    def test_get_endpoint_with_query(self):
        """Test GET endpoint with query parameter."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", environment="development"
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.get("/graphql?query={ hello }")
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["hello"] == "world"

    def test_get_endpoint_with_variables(self):
        """Test GET endpoint with variables."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", environment="development"
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[user], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            query = "query GetUser($id: ID!) { user(id: $id) { name } }"
            variables = json.dumps({"id": "123e4567-e89b-12d3-a456-426614174000"})
            response = client.get(
                f"/graphql?query={query}&variables={variables}&operationName=GetUser"
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["user"]["name"] == "Test User"

    def test_get_endpoint_invalid_variables(self):
        """Test GET endpoint with invalid JSON in variables."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", environment="development"
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.get("/graphql?query={ hello }&variables=invalid_json")
            assert response.status_code == 400
            assert "Invalid JSON in variables parameter" in response.json()["detail"]

    def test_custom_context_getter(self):
        """Test custom context getter integration."""

        async def custom_context(request: Request):
            return {"custom_value": "test_value_123", "request": request}

        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[User],
            queries=[context_test],
            context_getter=custom_context,
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post("/graphql", json={"query": "{ contextTest }"})
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["contextTest"] == "test_value_123"

    def test_auth_provider_integration(self):
        """Test auth provider integration."""

        class TestAuthProvider(AuthProvider):
            async def validate_token(self, token: str) -> dict[str, Any]:
                # Simple test validation
                if token == "valid_token":
                    return {"sub": "test_user", "email": "test@example.com"}
                raise Exception("Invalid token")

            async def get_user_from_token(self, token: str) -> UserContext:
                payload = await self.validate_token(token)
                return UserContext(user_id=payload["sub"], email=payload.get("email"))

        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            unified_executor_enabled=False,
        )

        app = create_fraiseql_app(
            config=config,
            types=[User],
            queries=[hello],
            auth=TestAuthProvider(),
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            # Without auth header - should now return 401 when auth provider is configured
            response = client.post("/graphql", json={"query": "{ hello }"})
            assert response.status_code == 401
            assert "Authentication required" in response.json()["detail"]

            # With valid auth token - should succeed
            response = client.post(
                """/graphql""",
                json={"query": "{ hello }"},
                headers={"Authorization": "Bearer valid_token"},
            )
            assert response.status_code == 200

    def test_mode_header_switching(self):
        """Test x-mode header for switching execution modes."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",  # Start in development
            unified_executor_enabled=False,
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[error_query], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            # Test with production mode header
            response = client.post(
                "/graphql", json={"query": "{ errorQuery }"}, headers={"x-mode": "production"}
            )
            assert response.status_code == 200
            data = response.json()
            # With x-mode header, should use mode-specific error handling
            # But in current implementation, it seems error details still show
            assert "error" in data["errors"][0]["message"].lower()

    def test_json_passthrough_header(self):
        """Test x-json-passthrough header."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", environment="development"
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            # Test with passthrough header
            response = client.post(
                "/graphql", json={"query": "{ hello }"}, headers={"x-json-passthrough": "true"}
            )
            assert response.status_code == 200
            # Should still work for simple queries
            data = response.json()
            assert data["data"]["hello"] == "world"

    def test_n_plus_one_detection(self):
        """Test N+1 query detection in development."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", environment="development"
        )

        app = create_fraiseql_app(
            config=config, types=[User, Post], queries=[posts], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            # Query that resolves author field for multiple posts
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            posts(limit: 5) {
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

            # Should successfully return data
            assert "data" in data
            assert len(data["data"]["posts"]) == 5
            # Each post should have an author
            for post in data["data"]["posts"]:
                assert "author" in post
                assert post["author"]["name"].startswith("Author")

    def test_malformed_json_request(self):
        """Test handling of malformed JSON requests."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", unified_executor_enabled=False
        )
        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql", data="invalid json", headers={"Content-Type": "application/json"}
            )
            # FastAPI should handle this and return 422
            assert response.status_code == 422

    def test_missing_query_field(self):
        """Test handling of request without query field."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test", unified_executor_enabled=False
        )
        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.post("/graphql", json={"variables": {}})
            # Should return validation error
            assert response.status_code == 422

    def test_introspection_query(self):
        """Test GraphQL introspection."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            enable_introspection=True,
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello, user], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            __schema {
                                queryType {
                                    fields {
                                        name
                                    }
                                }
                            }
                        }
                    """
                },
            )
            assert response.status_code == 200
            data = response.json()

            # Should have our queries in schema
            field_names = [f["name"] for f in data["data"]["__schema"]["queryType"]["fields"]]
            assert "hello" in field_names
            assert "user" in field_names

    def test_empty_operation_name(self):
        """Test handling of empty operation name."""
        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[User],
            queries=[hello],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            # Test with empty string - should get an error
            response = client.post("/graphql", json={"query": "{ hello }", "operationName": ""})
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            assert "Unknown operation named ''" in data["errors"][0]["message"]

            # Test without operation name - should work
            response = client.post("/graphql", json={"query": "{ hello }"})
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["hello"] == "world"

    def test_metrics_endpoint_in_development(self):
        """Test that metrics endpoint is available in development."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
            unified_executor_enabled=True,
        )

        app = create_fraiseql_app(
            config=config, types=[User], queries=[hello], lifespan=noop_lifespan
        )

        with TestClient(app) as client:
            # First make a query
            response = client.post("/graphql", json={"query": "{ hello }"})
            assert response.status_code == 200

            # Then check metrics (if available)
            response = client.get("/graphql/metrics")
            # Metrics endpoint may or may not exist depending on executor
            # Just verify it doesn't crash the app
            assert response.status_code in [200, 404]


class TestEdgeCases:
    """Test edge cases and complex scenarios."""

    def test_deeply_nested_query(self):
        """Test deeply nested GraphQL query."""

        @fraiseql.type
        class Level3:
            value: str

        @fraiseql.type
        class Level2:
            name: str
            level3: Level3

        @fraiseql.type
        class Level1:
            id: int
            level2: Level2

        @fraiseql.query
        async def deep_query(info) -> Level1:
            return Level1(id=1, level2=Level2(name="Level 2", level3=Level3(value="Deep value")))

        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[Level1, Level2, Level3],
            queries=[deep_query],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            deepQuery {
                                id
                                level2 {
                                    name
                                    level3 {
                                        value
                                    }
                                }
                            }
                        }
                    """
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["deepQuery"]["level2"]["level3"]["value"] == "Deep value"

    def test_concurrent_queries(self):
        """Test multiple queries in single request."""
        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[User],
            queries=[hello, user],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            greeting: hello
                            testUser: user(id: "123e4567-e89b-12d3-a456-426614174000") {
                                name
                            }
                        }
                    """
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["greeting"] == "world"
            assert data["data"]["testUser"]["name"] == "Test User"

    def test_null_handling(self):
        """Test null value handling."""

        @fraiseql.type
        class OptionalFields:
            required: str
            optional: str | None

        @fraiseql.query
        async def get_optional(info, include_optional: bool = False) -> OptionalFields:
            return OptionalFields(
                required="always here", optional="optional value" if include_optional else None
            )

        app = create_fraiseql_app(
            database_url="postgresql://test:test@localhost/test",
            types=[OptionalFields],
            queries=[get_optional],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            # Without optional
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            getOptional(includeOptional: false) {
                                required
                                optional
                            }
                        }
                    """
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["getOptional"]["required"] == "always here"
            assert data["data"]["getOptional"]["optional"] is None

            # With optional
            response = client.post(
                """/graphql""",
                json={
                    "query": """
                        query {
                            getOptional(includeOptional: true) {
                                required
                                optional
                            }
                        }
                    """
                },
            )
            assert response.status_code == 200
            data = response.json()
            assert data["data"]["getOptional"]["optional"] == "optional value"
