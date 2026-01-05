"""Unit tests for GraphQL Playground configuration."""

import pytest

from fraiseql.axum.playground import PlaygroundConfig


class TestPlaygroundConfigInitialization:
    """Test PlaygroundConfig initialization."""

    def test_default_configuration(self) -> None:
        """Test default configuration values."""
        config = PlaygroundConfig()

        assert config.enabled is True
        assert config.path == "/playground"
        assert config.title == "GraphQL Playground"
        assert config.subscriptions_endpoint == "/graphql/subscriptions"
        assert config.settings is None

    def test_custom_configuration(self) -> None:
        """Test custom configuration."""
        settings = {"scheme": "dark", "presets": ["graphQL"]}
        config = PlaygroundConfig(
            enabled=False,
            path="/gql",
            title="My API",
            subscriptions_endpoint="/ws/subscriptions",
            settings=settings,
        )

        assert config.enabled is False
        assert config.path == "/gql"
        assert config.title == "My API"
        assert config.subscriptions_endpoint == "/ws/subscriptions"
        assert config.settings == settings

    def test_invalid_path_no_slash(self) -> None:
        """Test that paths without leading slash are rejected."""
        with pytest.raises(ValueError):
            PlaygroundConfig(path="playground")

    def test_invalid_subscriptions_endpoint(self) -> None:
        """Test that subscription endpoints without leading slash are rejected."""
        with pytest.raises(ValueError):
            PlaygroundConfig(subscriptions_endpoint="ws/subscriptions")

    def test_disabled_playground(self) -> None:
        """Test creating disabled playground."""
        config = PlaygroundConfig(enabled=False)

        assert config.enabled is False


class TestPlaygroundHTMLGeneration:
    """Test HTML generation for playground."""

    def test_basic_html_generation(self) -> None:
        """Test basic HTML generation."""
        config = PlaygroundConfig()
        html = config.generate_html()

        assert "<!DOCTYPE html>" in html
        assert "GraphQL Playground" in html
        assert "graphql-playground" in html
        assert "/graphql" in html

    def test_custom_endpoint(self) -> None:
        """Test HTML with custom GraphQL endpoint."""
        config = PlaygroundConfig()
        html = config.generate_html(graphql_endpoint="/api/graphql")

        assert "endpoint: '/api/graphql'" in html

    def test_subscriptions_endpoint_included(self) -> None:
        """Test that subscription endpoint is included in HTML."""
        config = PlaygroundConfig(subscriptions_endpoint="/graphql/ws")
        html = config.generate_html()

        assert "subscriptionEndpoint: '/graphql/ws'" in html

    def test_subscriptions_endpoint_none(self) -> None:
        """Test HTML when subscriptions endpoint is None."""
        config = PlaygroundConfig(subscriptions_endpoint=None)
        html = config.generate_html()

        # Should not have subscription endpoint line
        assert "subscriptionEndpoint:" not in html

    def test_custom_title_in_html(self) -> None:
        """Test that custom title appears in HTML."""
        config = PlaygroundConfig(title="My API Explorer")
        html = config.generate_html()

        assert "My API Explorer" in html
        assert "<title>My API Explorer</title>" in html

    def test_html_contains_css_and_js(self) -> None:
        """Test that HTML includes CSS and JavaScript dependencies."""
        config = PlaygroundConfig()
        html = config.generate_html()

        assert "graphql-playground-react" in html
        assert ".css" in html
        assert ".js" in html
        assert "cdn.jsdelivr.net" in html

    def test_html_contains_root_div(self) -> None:
        """Test that HTML contains root div for playground."""
        config = PlaygroundConfig()
        html = config.generate_html()

        assert 'id="root"' in html

    def test_html_valid_structure(self) -> None:
        """Test that generated HTML has valid basic structure."""
        config = PlaygroundConfig()
        html = config.generate_html()

        assert html.count("<html>") == 1
        assert html.count("</html>") == 1
        assert html.count("<head>") == 1
        assert html.count("</head>") == 1
        assert html.count("<body>") == 1
        assert html.count("</body>") == 1

    def test_html_with_custom_settings(self) -> None:
        """Test HTML generation with custom settings."""
        settings = {
            "schema.disableComments": True,
            "editor.fontSize": 14,
            "editor.theme": "dark",
        }
        config = PlaygroundConfig(settings=settings)
        html = config.generate_html()

        assert "schema.disableComments" in html
        assert "editor.fontSize" in html
        assert "editor.theme" in html


class TestHTMLEscaping:
    """Test HTML and JavaScript escaping."""

    def test_title_html_escaping(self) -> None:
        """Test that special characters in title are escaped."""
        config = PlaygroundConfig(title='<script>alert("xss")</script>')
        html = config.generate_html()

        # Should be escaped
        assert "&lt;script&gt;" in html
        assert '<script>alert("xss")</script>' not in html

    def test_endpoint_js_escaping(self) -> None:
        """Test that endpoint URL is properly escaped for JavaScript."""
        config = PlaygroundConfig()
        html = config.generate_html(graphql_endpoint="/api/graphql'test")

        # Should have escaped quotes
        assert "\\'test" in html or "&#39;" in html or "&#x27;" in html

    def test_ampersand_escaping(self) -> None:
        """Test that ampersands are properly escaped."""
        config = PlaygroundConfig(title="Query & Mutation")
        html = config.generate_html()

        assert "Query &amp; Mutation" in html

    def test_quote_escaping_in_title(self) -> None:
        """Test quote escaping in title."""
        config = PlaygroundConfig(title='API "Explorer"')
        html = config.generate_html()

        assert "&quot;" in html or "&#39;" in html


class TestPlaygroundSerialization:
    """Test serialization methods."""

    def test_serialize_empty_settings(self) -> None:
        """Test serializing empty settings."""
        config = PlaygroundConfig()
        serialized = config._serialize_settings({})

        assert serialized == "{}"

    def test_serialize_string_settings(self) -> None:
        """Test serializing string settings."""
        config = PlaygroundConfig()
        settings = {"theme": "dark", "mode": "simple"}
        serialized = config._serialize_settings(settings)

        assert "theme: 'dark'" in serialized
        assert "mode: 'simple'" in serialized

    def test_serialize_boolean_settings(self) -> None:
        """Test serializing boolean settings."""
        config = PlaygroundConfig()
        settings = {"enabled": True, "debug": False}
        serialized = config._serialize_settings(settings)

        assert "enabled: true" in serialized
        assert "debug: false" in serialized

    def test_serialize_numeric_settings(self) -> None:
        """Test serializing numeric settings."""
        config = PlaygroundConfig()
        settings = {"fontSize": 14, "timeout": 30.5}
        serialized = config._serialize_settings(settings)

        assert "fontSize: 14" in serialized
        assert "timeout: 30.5" in serialized

    def test_serialize_nested_settings(self) -> None:
        """Test serializing nested object settings."""
        config = PlaygroundConfig()
        settings = {"editor": {"fontSize": 12, "theme": "dark"}}
        serialized = config._serialize_settings(settings)

        assert "editor:" in serialized
        assert "fontSize: 12" in serialized
        assert "theme: 'dark'" in serialized

    def test_to_dict(self) -> None:
        """Test converting config to dictionary."""
        settings = {"theme": "dark"}
        config = PlaygroundConfig(
            enabled=True,
            path="/gql",
            title="My API",
            subscriptions_endpoint="/ws",
            settings=settings,
        )

        config_dict = config.to_dict()

        assert isinstance(config_dict, dict)
        assert config_dict["enabled"] is True
        assert config_dict["path"] == "/gql"
        assert config_dict["title"] == "My API"
        assert config_dict["subscriptions_endpoint"] == "/ws"
        assert config_dict["settings"] == settings

    def test_to_dict_default_settings(self) -> None:
        """Test that to_dict returns empty dict for None settings."""
        config = PlaygroundConfig()
        config_dict = config.to_dict()

        assert config_dict["settings"] == {}


class TestPlaygroundStringRepresentation:
    """Test string representations."""

    def test_repr(self) -> None:
        """Test __repr__ method."""
        config = PlaygroundConfig(enabled=True, path="/playground")
        repr_str = repr(config)

        assert "PlaygroundConfig" in repr_str
        assert "enabled=True" in repr_str
        assert "/playground" in repr_str

    def test_repr_disabled(self) -> None:
        """Test __repr__ for disabled playground."""
        config = PlaygroundConfig(enabled=False)
        repr_str = repr(config)

        assert "enabled=False" in repr_str

    def test_str_enabled(self) -> None:
        """Test __str__ for enabled playground."""
        config = PlaygroundConfig(enabled=True, path="/playground")
        str_repr = str(config)

        assert "enabled" in str_repr
        assert "/playground" in str_repr

    def test_str_disabled(self) -> None:
        """Test __str__ for disabled playground."""
        config = PlaygroundConfig(enabled=False)
        str_repr = str(config)

        assert "disabled" in str_repr


class TestPlaygroundIntegration:
    """Integration tests for playground."""

    def test_development_setup(self) -> None:
        """Test typical development playground setup."""
        config = PlaygroundConfig(
            enabled=True,
            path="/playground",
            title="GraphQL Development",
            subscriptions_endpoint="/graphql/subscriptions",
        )

        assert config.enabled is True
        html = config.generate_html()
        assert "GraphQL Development" in html
        assert "/graphql" in html

    def test_production_disabled_setup(self) -> None:
        """Test production setup with playground disabled."""
        config = PlaygroundConfig(enabled=False)

        assert config.enabled is False
        # Even though disabled, HTML can still be generated for testing
        html = config.generate_html()
        assert "GraphQL Playground" in html

    def test_custom_api_setup(self) -> None:
        """Test custom API with custom endpoints."""
        config = PlaygroundConfig(
            enabled=True,
            path="/api/playground",
            title="Custom API Explorer",
            subscriptions_endpoint="/api/graphql/subscriptions",
        )

        assert config.path == "/api/playground"
        html = config.generate_html(graphql_endpoint="/api/graphql")

        assert "/api/graphql" in html
        assert "/api/graphql/subscriptions" in html
        assert "Custom API Explorer" in html

    def test_settings_preservation(self) -> None:
        """Test that custom settings are preserved in HTML."""
        settings = {
            "editor.cursorShape": "line",
            "editor.fontSize": 14,
            "schema.disableComments": True,
        }
        config = PlaygroundConfig(settings=settings)
        html = config.generate_html()

        assert "cursorShape: 'line'" in html
        assert "fontSize: 14" in html
        assert "disableComments: true" in html

    def test_multiple_instances(self) -> None:
        """Test multiple playground configs don't interfere."""
        config1 = PlaygroundConfig(path="/playground", title="API 1")
        config2 = PlaygroundConfig(path="/gql", title="API 2")

        html1 = config1.generate_html()
        html2 = config2.generate_html()

        assert "API 1" in html1
        assert "API 2" in html2
        assert "API 1" not in html2
        assert "API 2" not in html1
