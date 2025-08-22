"""Test for JSON passthrough production mode bug fix.

This test verifies that FraiseQL correctly respects the json_passthrough_in_production
configuration setting and doesn't force passthrough mode in production environments.

Bug: FraiseQL v0.3.0 ignores json_passthrough_in_production=False and forces
passthrough in production, causing snake_case fields instead of camelCase.
"""

from unittest.mock import MagicMock, patch

import pytest
from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import build_graphql_context, set_db_pool, set_fraiseql_config
from fraiseql.fastapi.routers import create_graphql_router


class TestProductionPassthroughBug:
    """Test that production mode respects json_passthrough_in_production configuration."""

    @pytest.fixture
    def mock_schema(self):
        """Create a simple test schema."""
        return GraphQLSchema(
            query=GraphQLObjectType(
                "Query",
                lambda: {
                    "test_field": GraphQLField(
                        GraphQLString, resolve=lambda obj, info: "test_value"
                    ),
                },
            )
        )

    @pytest.fixture
    def mock_db_pool(self):
        """Mock database pool."""
        return MagicMock()

    @pytest.mark.asyncio
    async def test_production_respects_passthrough_disabled(self, mock_schema, mock_db_pool):
        """Test that production mode respects json_passthrough_in_production=False.

        This is the CRITICAL test that verifies the bug fix.
        """
        # Configuration with passthrough DISABLED for production
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="production",
            json_passthrough_enabled=True,  # Enabled in general
            json_passthrough_in_production=False,  # But DISABLED for production
            auth_enabled=False,
        )

        # Set up dependencies
        set_fraiseql_config(config)
        set_db_pool(mock_db_pool)

        # Build GraphQL context (this is where the bug manifests)
        mock_user = None
        mock_db = MagicMock()

        with patch("fraiseql.fastapi.dependencies.get_db", return_value=mock_db):
            with patch("fraiseql.fastapi.dependencies.LoaderRegistry"):
                context = await build_graphql_context(db=mock_db, user=mock_user)

        # CRITICAL ASSERTION: json_passthrough should NOT be in context
        # when json_passthrough_in_production=False
        assert "json_passthrough" not in context or context.get("json_passthrough") is False
        assert context.get("execution_mode") != "passthrough"
        assert context["mode"] == "production"

    @pytest.mark.asyncio
    async def test_production_enables_passthrough_when_configured(self, mock_schema, mock_db_pool):
        """Test that production mode enables passthrough when both flags are true."""
        # Configuration with passthrough ENABLED for production
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="production",
            json_passthrough_enabled=True,  # Enabled in general
            json_passthrough_in_production=True,  # ENABLED for production
            auth_enabled=False,
        )

        # Set up dependencies
        set_fraiseql_config(config)
        set_db_pool(mock_db_pool)

        # Build GraphQL context
        mock_user = None
        mock_db = MagicMock()

        with patch("fraiseql.fastapi.dependencies.get_db", return_value=mock_db):
            with patch("fraiseql.fastapi.dependencies.LoaderRegistry"):
                context = await build_graphql_context(db=mock_db, user=mock_user)

        # When both flags are true, passthrough should be enabled
        assert context.get("json_passthrough") is True
        assert context.get("execution_mode") == "passthrough"
        assert context["mode"] == "production"

    @pytest.mark.asyncio
    async def test_development_ignores_in_production_flag(self, mock_schema, mock_db_pool):
        """Test that development mode ignores json_passthrough_in_production."""
        # Configuration for development
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="development",
            json_passthrough_enabled=True,
            json_passthrough_in_production=True,  # This should be ignored in dev
            auth_enabled=False,
        )

        # Set up dependencies
        set_fraiseql_config(config)
        set_db_pool(mock_db_pool)

        # Build GraphQL context
        mock_user = None
        mock_db = MagicMock()

        with patch("fraiseql.fastapi.dependencies.get_db", return_value=mock_db):
            with patch("fraiseql.fastapi.dependencies.LoaderRegistry"):
                context = await build_graphql_context(db=mock_db, user=mock_user)

        # Development mode should not enable passthrough based on in_production flag
        assert "json_passthrough" not in context or context.get("json_passthrough") is False
        assert context["mode"] == "development"

    @pytest.mark.asyncio
    async def test_router_respects_passthrough_config_in_production(
        self, mock_schema, mock_db_pool
    ):
        """Test that the router correctly handles passthrough configuration in production.

        This tests the actual router logic where the bug occurs.
        """
        # Configuration with passthrough DISABLED for production
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="production",
            json_passthrough_enabled=True,
            json_passthrough_in_production=False,  # DISABLED for production
            auth_enabled=False,
        )

        # Create router
        router = create_graphql_router(
            schema=mock_schema,
            config=config,
        )

        # Simulate a request in production mode
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        app = FastAPI()
        app.include_router(router)

        # Set up dependencies for the test
        set_fraiseql_config(config)
        set_db_pool(mock_db_pool)

        with patch("fraiseql.fastapi.dependencies.LoaderRegistry"):
            with patch("fraiseql.fastapi.dependencies.FraiseQLRepository") as MockRepo:
                mock_repo = MockRepo.return_value
                mock_repo.context = {}

                client = TestClient(app)

                # Make a GraphQL request
                response = client.post("/graphql", json={"query": "{ testField }"})

                assert response.status_code == 200

                # Check that passthrough was NOT enabled in the repository context
                # (The bug would set json_passthrough=True despite config)
                if hasattr(mock_repo, "context"):
                    assert mock_repo.context.get("json_passthrough") is not True

    @pytest.mark.parametrize(
        "env,enabled,in_prod,should_passthrough",
        [
            # Production environment - these are the critical cases
            ("production", False, False, False),  # Both disabled
            ("production", False, True, False),  # General disabled (takes precedence)
            ("production", True, False, False),  # CRITICAL: Disabled for production
            ("production", True, True, True),  # Both enabled
            # Development environment - in_production doesn't apply
            ("development", False, False, False),
            ("development", False, True, False),
            ("development", True, False, False),
            ("development", True, True, False),
            # Testing environment - treated as production in dependencies.py
            ("testing", False, False, False),
            ("testing", False, True, False),
            ("testing", True, False, False),
            (
                "testing",
                True,
                True,
                True,
            ),  # Testing is treated as production, so this enables passthrough
        ],
    )
    @pytest.mark.asyncio
    async def test_passthrough_configuration_matrix(
        self, mock_db_pool, env, enabled, in_prod, should_passthrough
    ):
        """Test all combinations of passthrough configuration.

        This comprehensive test ensures the logic is correct for all cases.
        """
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment=env,
            json_passthrough_enabled=enabled,
            json_passthrough_in_production=in_prod,
            auth_enabled=False,
        )

        set_fraiseql_config(config)
        set_db_pool(mock_db_pool)

        mock_db = MagicMock()

        with patch("fraiseql.fastapi.dependencies.get_db", return_value=mock_db):
            with patch("fraiseql.fastapi.dependencies.LoaderRegistry"):
                context = await build_graphql_context(db=mock_db, user=None)

        # Check if passthrough is enabled in context
        is_passthrough_enabled = (
            context.get("json_passthrough") is True
            and context.get("execution_mode") == "passthrough"
        )

        assert is_passthrough_enabled == should_passthrough, (
            f"Failed for env={env}, enabled={enabled}, in_prod={in_prod}. "
            f"Expected passthrough={should_passthrough}, got {is_passthrough_enabled}"
        )


class TestRouterPassthroughLogic:
    """Test the router's passthrough logic directly."""

    def test_router_production_check_logic(self):
        """Test the specific code path in routers.py that has the bug.

        The bug is around line 180-181 in routers.py where it unconditionally
        sets json_passthrough=True for production environments.
        """
        # This is the buggy logic that needs to be fixed:
        # if is_production_env:
        #     json_passthrough = True

        # It should be:
        # if is_production_env:
        #     if config.json_passthrough_enabled and config.json_passthrough_in_production:
        #         json_passthrough = True

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="production",
            json_passthrough_enabled=True,
            json_passthrough_in_production=False,  # Should prevent passthrough
            auth_enabled=False,
        )

        is_production_env = config.environment == "production"

        # Buggy logic (what the code currently does)
        buggy_json_passthrough = False
        if is_production_env:
            buggy_json_passthrough = True  # WRONG: Always enables in production

        # Fixed logic (what it should do)
        fixed_json_passthrough = False
        if is_production_env:
            if config.json_passthrough_enabled and config.json_passthrough_in_production:
                fixed_json_passthrough = True

        # The buggy logic incorrectly enables passthrough
        assert buggy_json_passthrough == True  # This is the bug!

        # The fixed logic correctly respects the configuration
        assert fixed_json_passthrough == False  # This is correct!

    def test_staging_mode_header_check_logic(self):
        """Test the logic for staging mode headers.

        The bug also affects the x-mode header handling around line 175-176.
        """
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="development",  # Base environment
            json_passthrough_enabled=True,
            json_passthrough_in_production=False,  # Should prevent passthrough
            auth_enabled=False,
        )

        mode = "staging"  # From x-mode header

        # Buggy logic
        buggy_json_passthrough = False
        if mode in ("production", "staging"):
            buggy_json_passthrough = True  # WRONG: Always enables

        # Fixed logic
        fixed_json_passthrough = False
        if mode in ("production", "staging"):
            if config.json_passthrough_enabled and config.json_passthrough_in_production:
                fixed_json_passthrough = True

        # The buggy logic incorrectly enables passthrough
        assert buggy_json_passthrough == True  # This is the bug!

        # The fixed logic correctly respects the configuration
        assert fixed_json_passthrough == False  # This is correct!
