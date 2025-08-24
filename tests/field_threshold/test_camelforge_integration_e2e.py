"""End-to-end integration tests for CamelForge functionality.

Tests the complete CamelForge flow from configuration to SQL generation
through the repository layer.
"""

import pytest

from fraiseql.core.ast_parser import FieldPath
from fraiseql.db import FraiseQLRepository
from fraiseql.fastapi.config import FraiseQLConfig



@pytest.mark.camelforge
@pytest.mark.database
@pytest.mark.e2e
class TestCamelForgeIntegrationE2E:
    """End-to-end tests for CamelForge integration."""

    @pytest.fixture
    def mock_pool(self):
        """Mock database pool."""
        from unittest.mock import MagicMock

        return MagicMock()

    @pytest.fixture
    def camelforge_config(self):
        """CamelForge enabled configuration."""
        return FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            camelforge_field_threshold=20,
        )

    @pytest.fixture
    def disabled_config(self):
        """CamelForge disabled configuration."""
        return FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=False,
            camelforge_field_threshold=20,
        )

    def test_repository_context_with_camelforge_enabled(self, mock_pool, camelforge_config):
        """Test that repository context includes CamelForge settings when enabled."""
        context = {
            "config": camelforge_config,
            "camelforge_enabled": camelforge_config.camelforge_enabled,
            "camelforge_function": camelforge_config.camelforge_function,
            "camelforge_field_threshold": camelforge_config.camelforge_field_threshold,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        assert repo.context["camelforge_enabled"] is True
        assert repo.context["camelforge_function"] == "turbo.fn_camelforge"
        assert repo.context["camelforge_field_threshold"] == 20

    def test_repository_context_with_camelforge_disabled(self, mock_pool, disabled_config):
        """Test that repository context handles CamelForge being disabled."""
        context = {
            "config": disabled_config,
            "camelforge_enabled": disabled_config.camelforge_enabled,
            "jsonb_field_limit_threshold": disabled_config.jsonb_field_limit_threshold,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        assert repo.context["camelforge_enabled"] is False

    def test_derive_entity_type_from_typename(self, mock_pool, camelforge_config):
        """Test entity type derivation from GraphQL typename."""
        context = {
            "camelforge_enabled": True,
            "camelforge_entity_mapping": True,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        # Test PascalCase to snake_case conversion
        assert repo._derive_entity_type("v_dns_server", "DnsServer") == "dns_server"
        assert repo._derive_entity_type("v_contract", "Contract") == "contract"
        assert repo._derive_entity_type("v_user_profile", "UserProfile") == "user_profile"

    def test_derive_entity_type_from_view_name(self, mock_pool, camelforge_config):
        """Test entity type derivation from view name when no typename."""
        context = {
            "camelforge_enabled": True,
            "camelforge_entity_mapping": True,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        # Test view name prefix removal
        assert repo._derive_entity_type("v_dns_server", None) == "dns_server"
        assert repo._derive_entity_type("tv_contract", None) == "contract"
        assert repo._derive_entity_type("mv_user_summary", None) == "user_summary"
        assert repo._derive_entity_type("dns_server", None) == "dns_server"  # No prefix

    def test_derive_entity_type_disabled(self, mock_pool):
        """Test that entity type derivation returns None when CamelForge is disabled."""
        context = {
            "camelforge_enabled": False,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        assert repo._derive_entity_type("v_dns_server", "DnsServer") is None
        assert repo._derive_entity_type("v_contract", None) is None

    def test_derive_entity_type_when_camelforge_disabled(self, mock_pool):
        """Test that entity type derivation returns None when CamelForge is disabled."""
        context = {
            "camelforge_enabled": False,
        }

        repo = FraiseQLRepository(pool=mock_pool, context=context)

        assert repo._derive_entity_type("v_dns_server", "DnsServer") is None
        assert repo._derive_entity_type("v_contract", None) is None

    def test_sql_generation_with_camelforge_below_threshold(self, mock_pool):
        """Test that SQL generation uses CamelForge when below field threshold."""
        from fraiseql.sql.sql_generator import build_sql_query

        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),
            FieldPath(alias="name", path=["name"]),
        ]

        # Test with CamelForge enabled and below threshold
        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,  # 3 fields < 20
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Should use CamelForge
        assert "turbo.fn_camelforge(" in sql_str
        assert "'dns_server'" in sql_str
        assert "jsonb_build_object(" in sql_str

    def test_sql_generation_with_camelforge_above_threshold(self, mock_pool):
        """Test that SQL generation bypasses CamelForge when above field threshold."""
        from fraiseql.sql.sql_generator import build_sql_query

        # Create 25 fields (above threshold of 20)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        # Test with CamelForge enabled but above threshold
        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,  # 25 fields > 20
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Should NOT use CamelForge (fall back to full data column)
        assert "turbo.fn_camelforge(" not in sql_str
        assert "jsonb_build_object(" not in sql_str
        assert "SELECT data AS result" in sql_str

    def test_configuration_integration(self):
        """Test that FraiseQLConfig properly handles CamelForge settings."""
        # Test default values
        config = FraiseQLConfig(database_url="postgresql://test@localhost/test")
        assert config.camelforge_enabled is False
        assert config.camelforge_function == "turbo.fn_camelforge"
        assert config.camelforge_field_threshold == 20

        # Test custom values
        custom_config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=True,
            camelforge_function="custom.my_camelforge",
            camelforge_field_threshold=30,
        )
        assert custom_config.camelforge_enabled is True
        assert custom_config.camelforge_function == "custom.my_camelforge"
        assert custom_config.camelforge_field_threshold == 30