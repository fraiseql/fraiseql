"""Unit tests for OpenAPI schema generation and documentation."""

import pytest

from fraiseql.axum.openapi import OpenAPIConfig


class TestOpenAPIConfigInitialization:
    """Test OpenAPIConfig initialization."""

    def test_default_configuration(self) -> None:
        """Test default configuration values."""
        config = OpenAPIConfig()

        assert config.title == "GraphQL API"
        assert config.description == ""
        assert config.version == "1.0.0"
        assert config.graphql_endpoint == "/graphql"
        assert config.subscriptions_endpoint is None
        assert config.enable_swagger_ui is True
        assert config.swagger_ui_path == "/docs"
        assert config.enable_redoc is True
        assert config.redoc_path == "/redoc"
        assert config.openapi_path == "/openapi.json"

    def test_custom_configuration(self) -> None:
        """Test custom configuration."""
        servers = [{"url": "https://api.example.com", "description": "Production"}]
        tags = [{"name": "users", "description": "User operations"}]
        external_docs = {"url": "https://docs.example.com"}

        config = OpenAPIConfig(
            title="My API",
            description="My custom API",
            version="2.0.0",
            graphql_endpoint="/api/graphql",
            subscriptions_endpoint="/api/subscriptions",
            enable_swagger_ui=False,
            swagger_ui_path="/swagger",
            enable_redoc=True,
            redoc_path="/api-docs",
            openapi_path="/api/openapi.json",
            servers=servers,
            tags=tags,
            external_docs=external_docs,
        )

        assert config.title == "My API"
        assert config.description == "My custom API"
        assert config.version == "2.0.0"
        assert config.graphql_endpoint == "/api/graphql"
        assert config.subscriptions_endpoint == "/api/subscriptions"
        assert config.enable_swagger_ui is False
        assert config.swagger_ui_path == "/swagger"
        assert config.enable_redoc is True
        assert config.redoc_path == "/api-docs"
        assert config.openapi_path == "/api/openapi.json"
        assert config.servers == servers
        assert config.tags == tags
        assert config.external_docs == external_docs

    def test_invalid_swagger_path(self) -> None:
        """Test that invalid Swagger path is rejected."""
        with pytest.raises(ValueError):
            OpenAPIConfig(swagger_ui_path="docs")

    def test_invalid_redoc_path(self) -> None:
        """Test that invalid ReDoc path is rejected."""
        with pytest.raises(ValueError):
            OpenAPIConfig(redoc_path="redoc")

    def test_invalid_openapi_path(self) -> None:
        """Test that invalid OpenAPI path is rejected."""
        with pytest.raises(ValueError):
            OpenAPIConfig(openapi_path="openapi.json")

    def test_invalid_graphql_endpoint(self) -> None:
        """Test that invalid GraphQL endpoint is rejected."""
        with pytest.raises(ValueError):
            OpenAPIConfig(graphql_endpoint="graphql")

    def test_invalid_subscriptions_endpoint(self) -> None:
        """Test that invalid subscriptions endpoint is rejected."""
        with pytest.raises(ValueError):
            OpenAPIConfig(subscriptions_endpoint="subscriptions")

    def test_disabled_documentation(self) -> None:
        """Test disabling documentation UIs."""
        config = OpenAPIConfig(enable_swagger_ui=False, enable_redoc=False)

        assert config.enable_swagger_ui is False
        assert config.enable_redoc is False


class TestOpenAPISchemaGeneration:
    """Test OpenAPI schema generation."""

    def test_basic_schema_generation(self) -> None:
        """Test basic OpenAPI schema generation."""
        config = OpenAPIConfig(title="Test API", version="1.0.0")
        schema = config.generate_openapi_schema()

        assert schema["openapi"] == "3.0.0"
        assert schema["info"]["title"] == "Test API"
        assert schema["info"]["version"] == "1.0.0"
        assert "/graphql" in schema["paths"]

    def test_schema_contains_graphql_endpoint(self) -> None:
        """Test that schema includes GraphQL endpoint."""
        config = OpenAPIConfig()
        schema = config.generate_openapi_schema()

        assert "/graphql" in schema["paths"]
        assert "post" in schema["paths"]["/graphql"]

    def test_schema_graphql_post_operation(self) -> None:
        """Test GraphQL POST operation details."""
        config = OpenAPIConfig()
        schema = config.generate_openapi_schema()
        graphql_op = schema["paths"]["/graphql"]["post"]

        assert "summary" in graphql_op
        assert "description" in graphql_op
        assert "requestBody" in graphql_op
        assert "responses" in graphql_op

    def test_schema_request_body_schema(self) -> None:
        """Test request body schema."""
        config = OpenAPIConfig()
        schema = config.generate_openapi_schema()
        request_body = schema["paths"]["/graphql"]["post"]["requestBody"]

        assert request_body["required"] is True
        assert "application/json" in request_body["content"]

        body_schema = request_body["content"]["application/json"]["schema"]
        assert "query" in body_schema["properties"]
        assert "variables" in body_schema["properties"]
        assert "operationName" in body_schema["properties"]
        assert "query" in body_schema["required"]

    def test_schema_responses(self) -> None:
        """Test response codes in schema."""
        config = OpenAPIConfig()
        schema = config.generate_openapi_schema()
        responses = schema["paths"]["/graphql"]["post"]["responses"]

        assert "200" in responses
        assert "400" in responses
        assert "500" in responses

    def test_schema_with_custom_endpoint(self) -> None:
        """Test schema with custom GraphQL endpoint."""
        config = OpenAPIConfig(graphql_endpoint="/api/v1/graphql")
        schema = config.generate_openapi_schema()

        assert "/api/v1/graphql" in schema["paths"]
        assert "/graphql" not in schema["paths"]

    def test_schema_with_description(self) -> None:
        """Test schema includes description."""
        config = OpenAPIConfig(description="My GraphQL API")
        schema = config.generate_openapi_schema()

        assert schema["info"]["description"] == "My GraphQL API"

    def test_schema_with_servers(self) -> None:
        """Test schema includes servers."""
        servers = [
            {"url": "https://api.example.com", "description": "Production"},
            {"url": "https://staging.example.com", "description": "Staging"},
        ]
        config = OpenAPIConfig(servers=servers)
        schema = config.generate_openapi_schema()

        assert schema["servers"] == servers

    def test_schema_with_tags(self) -> None:
        """Test schema includes tags."""
        tags = [
            {"name": "users", "description": "User operations"},
            {"name": "posts", "description": "Post operations"},
        ]
        config = OpenAPIConfig(tags=tags)
        schema = config.generate_openapi_schema()

        assert schema["tags"] == tags

    def test_schema_with_external_docs(self) -> None:
        """Test schema includes external docs."""
        external_docs = {"url": "https://docs.example.com", "description": "Full documentation"}
        config = OpenAPIConfig(external_docs=external_docs)
        schema = config.generate_openapi_schema()

        assert schema["externalDocs"] == external_docs

    def test_schema_with_subscriptions_endpoint(self) -> None:
        """Test schema includes WebSocket subscriptions endpoint."""
        config = OpenAPIConfig(subscriptions_endpoint="/graphql/subscriptions")
        schema = config.generate_openapi_schema()

        assert "/graphql/subscriptions" in schema["paths"]


class TestSwaggerUIGeneration:
    """Test Swagger UI HTML generation."""

    def test_basic_swagger_html_generation(self) -> None:
        """Test basic Swagger UI HTML generation."""
        config = OpenAPIConfig()
        html = config.generate_swagger_html()

        assert "<!DOCTYPE html>" in html
        assert "Swagger UI" in html
        assert "swagger-ui" in html
        assert "/openapi.json" in html

    def test_swagger_html_contains_required_elements(self) -> None:
        """Test Swagger HTML contains required elements."""
        config = OpenAPIConfig()
        html = config.generate_swagger_html()

        assert "<html>" in html
        assert "<head>" in html
        assert "<body>" in html
        assert 'id="swagger-ui"' in html

    def test_swagger_html_with_custom_title(self) -> None:
        """Test Swagger HTML with custom title."""
        config = OpenAPIConfig(title="My Custom API")
        html = config.generate_swagger_html()

        assert "My Custom API - Swagger UI" in html

    def test_swagger_html_with_custom_openapi_path(self) -> None:
        """Test Swagger HTML with custom OpenAPI path."""
        config = OpenAPIConfig(openapi_path="/api/openapi.json")
        html = config.generate_swagger_html()

        assert "/api/openapi.json" in html

    def test_swagger_html_escaping(self) -> None:
        """Test HTML escaping in Swagger."""
        config = OpenAPIConfig(title='<script>alert("xss")</script>')
        html = config.generate_swagger_html()

        assert "&lt;script&gt;" in html
        assert '<script>alert("xss")</script>' not in html

    def test_swagger_html_contains_cdn_resources(self) -> None:
        """Test Swagger HTML includes CDN resources."""
        config = OpenAPIConfig()
        html = config.generate_swagger_html()

        assert "cdnjs.cloudflare.com" in html or "cdn.jsdelivr.net" in html
        assert ".css" in html
        assert ".js" in html


class TestRedocGeneration:
    """Test ReDoc UI HTML generation."""

    def test_basic_redoc_html_generation(self) -> None:
        """Test basic ReDoc HTML generation."""
        config = OpenAPIConfig()
        html = config.generate_redoc_html()

        assert "<!DOCTYPE html>" in html
        assert "ReDoc" in html
        assert "<redoc" in html
        assert "/openapi.json" in html

    def test_redoc_html_contains_required_elements(self) -> None:
        """Test ReDoc HTML contains required elements."""
        config = OpenAPIConfig()
        html = config.generate_redoc_html()

        assert "<html>" in html
        assert "<head>" in html
        assert "<body>" in html
        assert "<redoc" in html

    def test_redoc_html_with_custom_title(self) -> None:
        """Test ReDoc HTML with custom title."""
        config = OpenAPIConfig(title="My API")
        html = config.generate_redoc_html()

        assert "My API - ReDoc" in html

    def test_redoc_html_with_description(self) -> None:
        """Test ReDoc HTML includes description."""
        config = OpenAPIConfig(description="My GraphQL API description")
        html = config.generate_redoc_html()

        assert "My GraphQL API description" in html

    def test_redoc_html_with_custom_openapi_path(self) -> None:
        """Test ReDoc HTML with custom OpenAPI path."""
        config = OpenAPIConfig(openapi_path="/api/v1/openapi.json")
        html = config.generate_redoc_html()

        assert "/api/v1/openapi.json" in html

    def test_redoc_html_escaping(self) -> None:
        """Test HTML escaping in ReDoc."""
        config = OpenAPIConfig(title='Test<>&"')
        html = config.generate_redoc_html()

        assert "&lt;" in html
        assert "&gt;" in html
        assert "&quot;" in html

    def test_redoc_html_contains_cdn_resources(self) -> None:
        """Test ReDoc HTML includes CDN resources."""
        config = OpenAPIConfig()
        html = config.generate_redoc_html()

        assert "cdn.jsdelivr.net" in html or "cdnjs.cloudflare.com" in html
        assert "redoc" in html.lower()


class TestOpenAPISerialization:
    """Test serialization methods."""

    def test_to_dict(self) -> None:
        """Test converting config to dictionary."""
        servers = [{"url": "https://api.example.com"}]
        tags = [{"name": "users"}]
        external_docs = {"url": "https://docs.example.com"}

        config = OpenAPIConfig(
            title="Test API",
            description="Test description",
            version="2.0.0",
            servers=servers,
            tags=tags,
            external_docs=external_docs,
        )

        config_dict = config.to_dict()

        assert isinstance(config_dict, dict)
        assert config_dict["title"] == "Test API"
        assert config_dict["description"] == "Test description"
        assert config_dict["version"] == "2.0.0"
        assert config_dict["servers"] == servers
        assert config_dict["tags"] == tags
        assert config_dict["external_docs"] == external_docs

    def test_to_dict_empty_servers_and_tags(self) -> None:
        """Test to_dict with empty servers and tags."""
        config = OpenAPIConfig()
        config_dict = config.to_dict()

        assert config_dict["servers"] == []
        assert config_dict["tags"] == []


class TestOpenAPIStringRepresentation:
    """Test string representations."""

    def test_repr(self) -> None:
        """Test __repr__ method."""
        config = OpenAPIConfig(title="Test API", version="1.0.0")
        repr_str = repr(config)

        assert "OpenAPIConfig" in repr_str
        assert "Test API" in repr_str
        assert "1.0.0" in repr_str

    def test_str_with_swagger_and_redoc(self) -> None:
        """Test __str__ with both UIs enabled."""
        config = OpenAPIConfig(enable_swagger_ui=True, enable_redoc=True)
        str_repr = str(config)

        assert "OpenAPI" in str_repr
        assert "/docs" in str_repr
        assert "/redoc" in str_repr

    def test_str_with_only_swagger(self) -> None:
        """Test __str__ with only Swagger UI."""
        config = OpenAPIConfig(enable_swagger_ui=True, enable_redoc=False)
        str_repr = str(config)

        assert "Swagger" in str_repr
        assert "/docs" in str_repr

    def test_str_with_only_redoc(self) -> None:
        """Test __str__ with only ReDoc."""
        config = OpenAPIConfig(enable_swagger_ui=False, enable_redoc=True)
        str_repr = str(config)

        assert "ReDoc" in str_repr
        assert "/redoc" in str_repr

    def test_str_with_no_documentation(self) -> None:
        """Test __str__ with no documentation enabled."""
        config = OpenAPIConfig(enable_swagger_ui=False, enable_redoc=False)
        str_repr = str(config)

        assert "No docs" in str_repr


class TestOpenAPIIntegration:
    """Integration tests for OpenAPI."""

    def test_development_setup(self) -> None:
        """Test typical development setup."""
        config = OpenAPIConfig(
            title="Development API",
            description="Local development GraphQL API",
            version="0.1.0",
            enable_swagger_ui=True,
            enable_redoc=True,
        )

        assert config.enable_swagger_ui is True
        assert config.enable_redoc is True

        schema = config.generate_openapi_schema()
        assert schema["info"]["title"] == "Development API"

        swagger_html = config.generate_swagger_html()
        assert "Development API" in swagger_html

        redoc_html = config.generate_redoc_html()
        assert "Development API" in redoc_html

    def test_production_setup(self) -> None:
        """Test production setup."""
        config = OpenAPIConfig(
            title="Production GraphQL API",
            description="Official production API",
            version="3.0.0",
            servers=[
                {"url": "https://api.example.com", "description": "Production"},
                {"url": "https://staging.example.com", "description": "Staging"},
            ],
            enable_swagger_ui=True,
            enable_redoc=True,
        )

        schema = config.generate_openapi_schema()
        assert len(schema["servers"]) == 2

    def test_full_documentation_workflow(self) -> None:
        """Test complete documentation workflow."""
        config = OpenAPIConfig(
            title="Full API",
            description="Complete API with all features",
            version="1.0.0",
            servers=[{"url": "/"}],
            tags=[
                {"name": "queries", "description": "GraphQL Queries"},
                {"name": "mutations", "description": "GraphQL Mutations"},
            ],
            external_docs={
                "url": "https://docs.example.com",
                "description": "Full documentation",
            },
        )

        # Generate all documentation
        schema = config.generate_openapi_schema()
        swagger_html = config.generate_swagger_html()
        redoc_html = config.generate_redoc_html()
        config_dict = config.to_dict()

        assert schema["info"]["title"] == "Full API"
        assert "Swagger" in swagger_html
        assert "ReDoc" in redoc_html
        assert config_dict["title"] == "Full API"

    def test_multiple_instances(self) -> None:
        """Test multiple OpenAPI configs don't interfere."""
        config1 = OpenAPIConfig(title="API v1", version="1.0.0")
        config2 = OpenAPIConfig(title="API v2", version="2.0.0")

        schema1 = config1.generate_openapi_schema()
        schema2 = config2.generate_openapi_schema()

        assert schema1["info"]["title"] == "API v1"
        assert schema2["info"]["title"] == "API v2"

        html1 = config1.generate_swagger_html()
        html2 = config2.generate_swagger_html()

        assert "API v1" in html1
        assert "API v2" in html2
