"""Test JSON and dict type support."""

from typing import Any

import pytest
from graphql import graphql

import fraiseql
from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema
from fraiseql.types import JSON


# Test types with various JSON/dict fields
@fraiseql.type
class ErrorWithDetails:
    """Error type with JSON details field."""

    message: str
    code: str
    details: dict[str, Any] | None = None


@fraiseql.type
class ConfigType:
    """Configuration with plain dict."""

    name: str
    settings: dict


@fraiseql.type
class MetadataType:
    """Type using JSON alias."""

    id: int
    metadata: JSON


# Query functions
async def get_error(info) -> ErrorWithDetails:
    """Return an error with details."""
    return ErrorWithDetails(
        message="Something went wrong",
        code="ERR_001",
        details={
            "field": "username",
            "reason": "already exists",
            "suggestions": ["try username2", "try username3"],
        },
    )


async def get_config(info) -> ConfigType:
    """Return configuration."""
    return ConfigType(
        name="app_config",
        settings={"debug": True, "timeout": 30, "features": ["feature1", "feature2"]},
    )


async def get_metadata(info) -> MetadataType:
    """Return metadata."""
    return MetadataType(
        id=1,
        metadata={
            "created_by": "admin",
            "tags": ["important", "reviewed"],
            "nested": {"level": 2, "data": [1, 2, 3]},
        },
    )


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear the registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


def test_dict_str_any_type() -> None:
    """Test that dict[str, Any] works properly."""
    schema = build_fraiseql_schema(query_types=[get_error])

    # Check that the schema was built successfully
    assert schema.query_type is not None

    # Check that the field is properly typed
    error_type = schema.type_map.get("ErrorWithDetails")
    assert error_type is not None
    assert "details" in error_type.fields

    # The type should be JSON (nullable)
    details_field = error_type.fields["details"]
    assert details_field.type.name == "JSON"  # It's nullable, so no wrapper


def test_plain_dict_type() -> None:
    """Test that plain dict works."""
    schema = build_fraiseql_schema(query_types=[get_config])

    config_type = schema.type_map.get("ConfigType")
    assert config_type is not None
    assert "settings" in config_type.fields

    # Should be JSON scalar
    settings_field = config_type.fields["settings"]
    # Check if it's wrapped in NonNull
    field_type = settings_field.type
    if hasattr(field_type, "of_type"):
        assert field_type.of_type.name == "JSON"
    else:
        assert field_type.name == "JSON"


def test_json_alias_type() -> None:
    """Test that JSON alias works."""
    schema = build_fraiseql_schema(query_types=[get_metadata])

    metadata_type = schema.type_map.get("MetadataType")
    assert metadata_type is not None
    assert "metadata" in metadata_type.fields

    # Should be JSON scalar
    metadata_field = metadata_type.fields["metadata"]
    field_type = metadata_field.type
    if hasattr(field_type, "of_type"):
        assert field_type.of_type.name == "JSON"
    else:
        assert field_type.name == "JSON"


@pytest.mark.asyncio
async def test_query_execution_with_json() -> None:
    """Test that JSON fields work in query execution."""
    schema = build_fraiseql_schema(query_types=[get_error, get_config, get_metadata])

    # Query all types
    query = """
        query {
            getError {
                message
                code
                details
            }
            getConfig {
                name
                settings
            }
            getMetadata {
                id
                metadata
            }
        }
    """
    result = await graphql(schema, query, context_value={})

    assert result.errors is None
    assert result.data == {
        "getError": {
            "message": "Something went wrong",
            "code": "ERR_001",
            "details": {
                "field": "username",
                "reason": "already exists",
                "suggestions": ["try username2", "try username3"],
            },
        },
        "getConfig": {
            "name": "app_config",
            "settings": {"debug": True, "timeout": 30, "features": ["feature1", "feature2"]},
        },
        "getMetadata": {
            "id": 1,
            "metadata": {
                "created_by": "admin",
                "tags": ["important", "reviewed"],
                "nested": {"level": 2, "data": [1, 2, 3]},
            },
        },
    }


@pytest.mark.asyncio
async def test_null_json_field() -> None:
    """Test that null JSON fields work properly."""

    async def get_error_no_details(info) -> ErrorWithDetails:
        return ErrorWithDetails(message="Simple error", code="ERR_002", details=None)

    schema = build_fraiseql_schema(query_types=[get_error_no_details])

    query = """
        query {
            getErrorNoDetails {
                message
                code
                details
            }
        }
    """
    result = await graphql(schema, query, context_value={})

    assert result.errors is None
    assert result.data == {
        "getErrorNoDetails": {"message": "Simple error", "code": "ERR_002", "details": None}
    }
