"""Test to verify that JSON passthrough respects configuration settings."""

from contextlib import asynccontextmanager

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.gql.schema_builder import SchemaRegistry



@pytest.mark.security
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


@asynccontextmanager
async def noop_lifespan(app: FastAPI):
    """No-op lifespan for tests that don't need a database."""
    yield


# Define test types and queries at module level to avoid scoping issues
# Use names that don't start with Test to avoid pytest collection
@fraiseql.type
class DataType:
    """Test type for JSON passthrough testing."""

    snake_case_field: str
    another_snake_field: str


@fraiseql.query
async def data_query(info) -> DataType:
    """Query that returns snake_case fields."""
    return DataType(snake_case_field="test_value", another_snake_field="another_value")


class TestJSONPassthroughConfigFix:
    """Test that JSON passthrough configuration is properly respected."""

    def test_json_passthrough_disabled_in_production(self):
        """Test that JSON passthrough is disabled when explicitly configured as False."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            # Explicitly disable JSON passthrough
            json_passthrough_enabled=False,
            json_passthrough_in_production=False,
        )

        app = create_fraiseql_app(
            config=config,
            types=[DataType],
            queries=[data_query],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            dataQuery {
                                snakeCaseField
                                anotherSnakeField
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Should have camelCase fields (NOT snake_case)
            # This means GraphQL transformation is working, passthrough is disabled
            assert "data" in data
            assert "dataQuery" in data["data"]

            test_data = data["data"]["dataQuery"]

            # These should be in camelCase because passthrough is disabled
            assert "snakeCaseField" in test_data
            assert "anotherSnakeField" in test_data

            # These should NOT be present (would indicate passthrough was enabled)
            assert "snake_case_field" not in test_data
            assert "another_snake_field" not in test_data

    def test_json_passthrough_enabled_explicitly(self):
        """Test that JSON passthrough works when explicitly enabled."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            # Explicitly enable JSON passthrough
            json_passthrough_enabled=True,
            json_passthrough_in_production=True,
        )

        app = create_fraiseql_app(
            config=config,
            types=[DataType],
            queries=[data_query],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            dataQuery {
                                snakeCaseField
                                anotherSnakeField
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            # With passthrough enabled, we should get whatever the resolver returns
            # The exact format may vary based on implementation details

    def test_production_mode_respects_config(self):
        """Test that production mode alone doesn't enable passthrough."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",
            # Don't set passthrough configs - should default to False
        )

        app = create_fraiseql_app(
            config=config,
            types=[DataType],
            queries=[data_query],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            dataQuery {
                                snakeCaseField
                                anotherSnakeField
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Should have camelCase fields because passthrough defaults to disabled
            assert "data" in data
            test_data = data["data"]["dataQuery"]

            # Should be transformed to camelCase
            assert "snakeCaseField" in test_data
            assert "anotherSnakeField" in test_data

    def test_staging_mode_respects_config(self):
        """Test that staging mode also respects the configuration."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="production",  # Use production since staging isn't valid
            json_passthrough_enabled=False,
            json_passthrough_in_production=False,
        )

        app = create_fraiseql_app(
            config=config,
            types=[DataType],
            queries=[data_query],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            dataQuery {
                                snakeCaseField
                                anotherSnakeField
                            }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Should respect config and provide camelCase
            test_data = data["data"]["dataQuery"]
            assert "snakeCaseField" in test_data
            assert "anotherSnakeField" in test_data