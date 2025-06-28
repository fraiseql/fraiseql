"""Extended tests for FastAPI routers to improve coverage."""

import json
from unittest.mock import AsyncMock, Mock, patch

import pytest
from fastapi import Request
from fastapi.testclient import TestClient
from graphql import GraphQLError, GraphQLSchema, build_schema

from fraiseql.auth.base import AuthProvider
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import (
    APOLLO_SANDBOX_HTML,
    GRAPHIQL_HTML,
    GraphQLRequest,
    create_development_router,
    create_graphql_router,
    create_production_router,
)
from fraiseql.fastapi.turbo import TurboRegistry
from fraiseql.optimization.n_plus_one_detector import N1QueryDetectedError, QueryPattern


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
        variables = {"id": 123, "name": "test"}
        request = GraphQLRequest(
            query="query($id: Int!) { user(id: $id) { name } }",
            variables=variables,
            operationName="GetUser"
        )
        assert request.query == "query($id: Int!) { user(id: $id) { name } }"
        assert request.variables == variables
        assert request.operationName == "GetUser"

    def test_graphql_request_validation(self):
        """Test GraphQL request validation."""
        # Query is required
        with pytest.raises(ValueError):
            GraphQLRequest()


class TestCreateGraphQLRouter:
    """Test create_graphql_router function."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("type Query { hello: String }")
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")

    def test_create_router_development(self):
        """Test router creation for development environment."""
        self.config.environment = "development"
        router = create_graphql_router(self.schema, self.config)
        
        # Should return development router
        assert router is not None
        assert len(router.routes) > 0

    def test_create_router_production(self):
        """Test router creation for production environment."""
        self.config.environment = "production"
        router = create_graphql_router(self.schema, self.config)
        
        # Should return production router
        assert router is not None
        assert len(router.routes) > 0

    def test_create_router_with_auth_provider(self):
        """Test router creation with auth provider."""
        auth_provider = Mock(spec=AuthProvider)
        router = create_graphql_router(self.schema, self.config, auth_provider=auth_provider)
        assert router is not None

    def test_create_router_with_context_getter(self):
        """Test router creation with custom context getter."""
        async def custom_context(request: Request):
            return {"custom": "value"}
        
        router = create_graphql_router(
            self.schema, 
            self.config, 
            context_getter=custom_context
        )
        assert router is not None

    def test_create_router_with_turbo_registry(self):
        """Test router creation with turbo registry."""
        turbo_registry = Mock(spec=TurboRegistry)
        self.config.environment = "production"
        
        router = create_graphql_router(
            self.schema, 
            self.config, 
            turbo_registry=turbo_registry
        )
        assert router is not None


class TestDevelopmentRouter:
    """Test development router functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("""
            type Query {
                hello: String
                user(id: Int!): User
            }
            
            type User {
                id: Int!
                name: String!
            }
        """)
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")
        self.config.environment = "development"
        self.config.enable_playground = True

    def test_development_router_creation(self):
        """Test development router creation."""
        router = create_development_router(self.schema, self.config)
        
        assert router is not None
        assert len(router.routes) >= 2  # POST and GET endpoints

    def test_development_router_with_custom_context(self):
        """Test development router with custom context getter."""
        async def custom_context(request: Request):
            return {"user_id": 123}
        
        router = create_development_router(
            self.schema, 
            self.config, 
            context_getter=custom_context
        )
        assert router is not None

    @patch('fraiseql.fastapi.routers.graphql')
    def test_development_post_endpoint_success(self, mock_graphql):
        """Test successful POST request to development endpoint."""
        # Mock GraphQL execution result
        mock_result = Mock()
        mock_result.data = {"hello": "world"}
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["data"] == {"hello": "world"}

    @patch('fraiseql.fastapi.routers.graphql')
    def test_development_post_endpoint_with_errors(self, mock_graphql):
        """Test POST request with GraphQL errors."""
        # Mock GraphQL execution result with errors
        mock_error = Mock()
        mock_error.message = "Test error"
        mock_error.locations = [Mock(line=1, column=1)]
        mock_error.path = ["hello"]
        mock_error.extensions = {"code": "TEST_ERROR"}
        
        mock_result = Mock()
        mock_result.data = None
        mock_result.errors = [mock_error]
        mock_graphql.return_value = mock_result

        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["message"] == "Test error"

    @patch('fraiseql.fastapi.routers.n1_detection_context')
    def test_development_n1_detection(self, mock_n1_context):
        """Test N+1 query detection in development."""
        # Mock N+1 detection to raise error
        patterns = [QueryPattern(field_name="user", parent_type="Query", count=5)]
        error = N1QueryDetectedError("N+1 detected", patterns)
        
        mock_detector = AsyncMock()
        mock_n1_context.return_value.__aenter__.return_value = mock_detector
        mock_n1_context.return_value.__aexit__.return_value = None
        
        # Make graphql execution raise N1 error
        with patch('fraiseql.fastapi.routers.graphql', side_effect=error):
            router = create_development_router(self.schema, self.config)
            client = TestClient(router)
            
            response = client.post(
                "/graphql",
                json={"query": "{ hello }"}
            )
            
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            assert data["errors"][0]["extensions"]["code"] == "N1_QUERY_DETECTED"

    def test_development_general_exception(self):
        """Test general exception handling in development."""
        with patch('fraiseql.fastapi.routers.graphql', side_effect=ValueError("Test error")):
            router = create_development_router(self.schema, self.config)
            client = TestClient(router)
            
            response = client.post(
                "/graphql",
                json={"query": "{ hello }"}
            )
            
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            assert data["errors"][0]["message"] == "Test error"
            assert data["errors"][0]["extensions"]["code"] == "INTERNAL_SERVER_ERROR"

    def test_development_get_playground_graphiql(self):
        """Test GET endpoint serving GraphiQL playground."""
        self.config.playground_tool = "graphiql"
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql")
        
        assert response.status_code == 200
        assert "graphiql" in response.text.lower()
        assert GRAPHIQL_HTML in response.text

    def test_development_get_playground_apollo(self):
        """Test GET endpoint serving Apollo Sandbox."""
        self.config.playground_tool = "apollo-sandbox"
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql")
        
        assert response.status_code == 200
        assert "apollo" in response.text.lower()
        assert APOLLO_SANDBOX_HTML in response.text

    def test_development_get_without_query_no_playground(self):
        """Test GET endpoint without query and playground disabled."""
        self.config.enable_playground = False
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql")
        
        assert response.status_code == 400
        assert "Query parameter is required" in response.json()["detail"]

    @patch('fraiseql.fastapi.routers.graphql')
    def test_development_get_with_query(self, mock_graphql):
        """Test GET endpoint with query parameter."""
        mock_result = Mock()
        mock_result.data = {"hello": "world"}
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql?query={ hello }")
        
        assert response.status_code == 200
        data = response.json()
        assert data["data"] == {"hello": "world"}

    def test_development_get_with_variables(self):
        """Test GET endpoint with variables parameter."""
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        variables = json.dumps({"id": 123})
        
        with patch('fraiseql.fastapi.routers.graphql') as mock_graphql:
            mock_result = Mock()
            mock_result.data = {"user": {"id": 123}}
            mock_result.errors = None
            mock_graphql.return_value = mock_result
            
            response = client.get(
                f"/graphql?query=query($id: Int!) {{ user(id: $id) {{ id }} }}&variables={variables}"
            )
            
            assert response.status_code == 200

    def test_development_get_invalid_variables(self):
        """Test GET endpoint with invalid JSON variables."""
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql?query={ hello }&variables=invalid_json")
        
        assert response.status_code == 400
        assert "Invalid JSON in variables parameter" in response.json()["detail"]

    def test_development_n1_detector_configuration(self):
        """Test N+1 detector configuration in development router."""
        with patch('fraiseql.fastapi.routers.configure_detector') as mock_configure:
            with patch('fraiseql.fastapi.routers.get_detector') as mock_get:
                mock_detector = Mock()
                mock_get.return_value = mock_detector
                
                # First call - detector not configured
                router = create_development_router(self.schema, self.config)
                mock_configure.assert_called_once()
                
                # Second call - detector already configured
                mock_detector._configured = True
                router2 = create_development_router(self.schema, self.config)
                # configure_detector should not be called again
                assert mock_configure.call_count == 1


class TestProductionRouter:
    """Test production router functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("""
            type Query {
                hello: String
                user(id: Int!): User
            }
            
            type User {
                id: Int!
                name: String!
            }
        """)
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")
        self.config.environment = "production"

    def test_production_router_creation(self):
        """Test production router creation."""
        router = create_production_router(self.schema, self.config)
        
        assert router is not None
        # Production router only has POST endpoint
        assert len(router.routes) == 1

    def test_production_router_with_turbo_registry(self):
        """Test production router with turbo registry."""
        turbo_registry = Mock(spec=TurboRegistry)
        router = create_production_router(
            self.schema, 
            self.config, 
            turbo_registry=turbo_registry
        )
        assert router is not None

    @patch('fraiseql.fastapi.routers.graphql')
    def test_production_post_endpoint_success(self, mock_graphql):
        """Test successful POST request to production endpoint."""
        mock_result = Mock()
        mock_result.data = {"hello": "world"}
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        router = create_production_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["data"] == {"hello": "world"}

    @patch('fraiseql.fastapi.routers.TurboRouter')
    def test_production_turbo_router_execution(self, mock_turbo_class):
        """Test turbo router execution in production."""
        # Mock turbo registry and router
        turbo_registry = Mock(spec=TurboRegistry)
        mock_turbo_router = Mock()
        mock_turbo_router.execute.return_value = {"data": {"fast": "result"}}
        mock_turbo_class.return_value = mock_turbo_router

        router = create_production_router(
            self.schema, 
            self.config, 
            turbo_registry=turbo_registry
        )
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["data"] == {"fast": "result"}

    @patch('fraiseql.fastapi.routers.TurboRouter')
    @patch('fraiseql.fastapi.routers.graphql')
    def test_production_fallback_to_standard_graphql(self, mock_graphql, mock_turbo_class):
        """Test fallback to standard GraphQL when turbo router returns None."""
        # Mock turbo router to return None (no match)
        turbo_registry = Mock(spec=TurboRegistry)
        mock_turbo_router = Mock()
        mock_turbo_router.execute.return_value = None
        mock_turbo_class.return_value = mock_turbo_router

        # Mock standard GraphQL execution
        mock_result = Mock()
        mock_result.data = {"hello": "standard"}
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        router = create_production_router(
            self.schema, 
            self.config, 
            turbo_registry=turbo_registry
        )
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["data"] == {"hello": "standard"}

    @patch('fraiseql.fastapi.routers.parse')
    def test_production_parse_error(self, mock_parse):
        """Test parse error handling in production."""
        mock_parse.side_effect = Exception("Parse failed")

        router = create_production_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "invalid query"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["message"] == "Invalid query"
        assert data["errors"][0]["extensions"]["code"] == "GRAPHQL_PARSE_FAILED"

    @patch('fraiseql.fastapi.routers.validate')
    @patch('fraiseql.fastapi.routers.parse')
    def test_production_validation_error(self, mock_parse, mock_validate):
        """Test validation error handling in production."""
        mock_parse.return_value = Mock()  # Successful parse
        mock_error = Mock()
        mock_error.message = "Validation failed"
        mock_validate.return_value = [mock_error]

        router = create_production_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ invalidField }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["message"] == "Validation failed"
        assert data["errors"][0]["extensions"]["code"] == "GRAPHQL_VALIDATION_FAILED"

    @patch('fraiseql.fastapi.routers.graphql')
    def test_production_execution_error_hidden(self, mock_graphql):
        """Test that execution errors are hidden in production."""
        mock_error = Mock()
        mock_error.message = "Sensitive error information"
        
        mock_result = Mock()
        mock_result.data = None
        mock_result.errors = [mock_error]
        mock_graphql.return_value = mock_result

        # Config with hidden error details (default)
        config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")
        config.config = {"hide_error_details": True}
        
        router = create_production_router(self.schema, config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["message"] == "Internal server error"
        assert "Sensitive error information" not in str(data)

    @patch('fraiseql.fastapi.routers.graphql')
    def test_production_execution_error_exposed(self, mock_graphql):
        """Test that execution errors can be exposed in production."""
        mock_error = Mock()
        mock_error.message = "Detailed error information"
        
        mock_result = Mock()
        mock_result.data = None
        mock_result.errors = [mock_error]
        mock_graphql.return_value = mock_result

        # Config with exposed error details
        config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")
        config.config = {"hide_error_details": False}
        
        router = create_production_router(self.schema, config)
        client = TestClient(router)
        
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )
        
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["message"] == "Detailed error information"

    def test_production_general_exception(self):
        """Test general exception handling in production."""
        with patch('fraiseql.fastapi.routers.graphql', side_effect=RuntimeError("Server error")):
            router = create_production_router(self.schema, self.config)
            client = TestClient(router)
            
            response = client.post(
                "/graphql",
                json={"query": "{ hello }"}
            )
            
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
            assert data["errors"][0]["message"] == "Internal server error"
            assert data["errors"][0]["extensions"]["code"] == "INTERNAL_SERVER_ERROR"

    def test_production_no_get_endpoint(self):
        """Test that production router has no GET endpoint."""
        router = create_production_router(self.schema, self.config)
        client = TestClient(router)
        
        response = client.get("/graphql")
        
        # Should return 405 Method Not Allowed
        assert response.status_code == 405


class TestContextHandling:
    """Test context handling in routers."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("type Query { hello: String }")
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")

    def test_custom_context_merge_development(self):
        """Test custom context merging in development router."""
        async def custom_context(request: Request):
            return {"custom_key": "custom_value"}

        router = create_development_router(
            self.schema, 
            self.config, 
            context_getter=custom_context
        )
        
        # Router should be created successfully with custom context
        assert router is not None

    def test_custom_context_merge_production(self):
        """Test custom context merging in production router."""
        async def custom_context(request: Request):
            return {"custom_key": "custom_value"}

        router = create_production_router(
            self.schema, 
            self.config, 
            context_getter=custom_context
        )
        
        # Router should be created successfully with custom context
        assert router is not None

    def test_no_custom_context_development(self):
        """Test development router without custom context."""
        router = create_development_router(self.schema, self.config)
        assert router is not None

    def test_no_custom_context_production(self):
        """Test production router without custom context."""
        router = create_production_router(self.schema, self.config)
        assert router is not None


class TestHTMLContent:
    """Test HTML content constants."""

    def test_graphiql_html_content(self):
        """Test GraphiQL HTML content."""
        assert "GraphiQL" in GRAPHIQL_HTML
        assert "graphiql" in GRAPHIQL_HTML.lower()
        assert "/graphql" in GRAPHIQL_HTML
        assert "<!DOCTYPE html>" in GRAPHIQL_HTML

    def test_apollo_sandbox_html_content(self):
        """Test Apollo Sandbox HTML content."""
        assert "Apollo" in APOLLO_SANDBOX_HTML
        assert "sandbox" in APOLLO_SANDBOX_HTML.lower()
        assert "/graphql" in APOLLO_SANDBOX_HTML
        assert "<!DOCTYPE html>" in APOLLO_SANDBOX_HTML
        assert "EmbeddedSandbox" in APOLLO_SANDBOX_HTML


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("type Query { hello: String }")
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")

    def test_malformed_json_request(self):
        """Test handling of malformed JSON requests."""
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        # Send malformed JSON
        response = client.post(
            "/graphql",
            data="invalid json",
            headers={"Content-Type": "application/json"}
        )
        
        # FastAPI should handle this and return 422
        assert response.status_code == 422

    def test_missing_query_field(self):
        """Test handling of request without query field."""
        router = create_development_router(self.schema, self.config)
        client = TestClient(router)
        
        # Send JSON without query field
        response = client.post(
            "/graphql",
            json={"variables": {}}
        )
        
        # Should return validation error
        assert response.status_code == 422

    def test_introspection_enabled_config(self):
        """Test router creation with introspection enabled."""
        self.config.enable_introspection = True
        router = create_development_router(self.schema, self.config)
        
        # Should create router successfully
        assert router is not None

    def test_empty_operation_name(self):
        """Test handling of empty operation name."""
        with patch('fraiseql.fastapi.routers.graphql') as mock_graphql:
            mock_result = Mock()
            mock_result.data = {"hello": "world"}
            mock_result.errors = None
            mock_graphql.return_value = mock_result

            router = create_development_router(self.schema, self.config)
            client = TestClient(router)
            
            response = client.post(
                "/graphql",
                json={
                    "query": "{ hello }",
                    "operationName": ""
                }
            )
            
            assert response.status_code == 200