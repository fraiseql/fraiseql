"""Integration tests for AutoDiscovery."""

import pytest

from fraiseql.introspection.auto_discovery import AutoDiscovery

pytestmark = [pytest.mark.asyncio, pytest.mark.integration, pytest.mark.database]


class TestAutoDiscovery:
    """Test AutoDiscovery functionality with real database."""

    @pytest.fixture
    def auto_discovery(self, class_db_pool) -> AutoDiscovery:
        """Create AutoDiscovery instance with real database pool."""
        return AutoDiscovery(class_db_pool)

    @pytest.mark.asyncio
    async def test_discover_all_empty_database(self, auto_discovery: AutoDiscovery) -> None:
        """Test discovery on empty database returns empty results."""
        result = await auto_discovery.discover_all()

        assert isinstance(result, dict)
        assert "types" in result
        assert "queries" in result
        assert "mutations" in result
        assert result["types"] == []
        assert result["queries"] == []
        assert result["mutations"] == []

    @pytest.mark.asyncio
    async def test_discover_all_with_custom_patterns(self, auto_discovery: AutoDiscovery) -> None:
        """Test discovery with custom patterns."""
        result = await auto_discovery.discover_all(
            view_pattern="custom_%", function_pattern="custom_%", schemas=["public"]
        )

        assert isinstance(result, dict)
        assert "types" in result
        assert "queries" in result
        assert "mutations" in result

    def test_type_registry_initially_empty(self, auto_discovery: AutoDiscovery) -> None:
        """Test that type registry starts empty."""
        assert auto_discovery.type_registry == {}

    def test_components_initialized(self, auto_discovery: AutoDiscovery) -> None:
        """Test that all components are properly initialized."""
        assert hasattr(auto_discovery, "introspector")
        assert hasattr(auto_discovery, "metadata_parser")
        assert hasattr(auto_discovery, "type_mapper")
        assert hasattr(auto_discovery, "type_generator")
        assert hasattr(auto_discovery, "input_generator")
        assert hasattr(auto_discovery, "query_generator")
        assert hasattr(auto_discovery, "mutation_generator")

    @pytest.mark.asyncio
    async def test_discover_all_with_mock_data(
        self, auto_discovery: AutoDiscovery, monkeypatch
    ) -> None:
        """Test discovery pipeline with mocked database responses."""
        # Mock the introspector methods
        mock_views = [
            type(
                "MockView",
                (),
                {
                    "view_name": "v_users",
                    "comment": "@fraiseql:type\ntrinity: true\ndescription: User accounts",
                },
            )()
        ]
        mock_functions = [
            type(
                "MockFunction",
                (),
                {
                    "function_name": "fn_create_user",
                    "comment": "@fraiseql:mutation\ninput_schema:\n  name: {type: string}\n  email: {type: string}\nsuccess_type: User\nerror_type: Error",
                },
            )()
        ]

        async def mock_discover_views(*args, **kwargs):
            return mock_views

        async def mock_discover_functions(*args, **kwargs):
            return mock_functions

        monkeypatch.setattr(auto_discovery.introspector, "discover_views", mock_discover_views)
        monkeypatch.setattr(
            auto_discovery.introspector, "discover_functions", mock_discover_functions
        )

        # Mock the type generation methods
        mock_type_class = type("MockType", (), {"__name__": "User"})()

        async def mock_generate_type(*args, **kwargs):
            return mock_type_class

        def mock_generate_queries(*args, **kwargs):
            return [type("MockQuery", (), {})()]

        async def mock_generate_mutation(*args, **kwargs):
            return type("MockMutation", (), {})()

        monkeypatch.setattr(auto_discovery, "_generate_type_from_view", mock_generate_type)
        monkeypatch.setattr(auto_discovery, "_generate_queries_for_type", mock_generate_queries)
        monkeypatch.setattr(
            auto_discovery, "_generate_mutation_from_function", mock_generate_mutation
        )

        result = await auto_discovery.discover_all()

        assert isinstance(result, dict)
        assert "types" in result
        assert "queries" in result
        assert "mutations" in result
        assert len(result["types"]) == 1
        assert len(result["queries"]) == 1
        assert len(result["mutations"]) == 1
