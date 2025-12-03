"""Test rust_transformer.py with fraiseql_rs v0.2.0 API."""

import fraiseql._fraiseql_rs as fraiseql_rs
from src.fraiseql.core.rust_transformer import RustTransformer


def test_transform_without_schema_registry() -> None:
    """Ensure transformation works without SchemaRegistry."""
    json_str = '{"user_id": 1, "first_name": "John"}'

    # Test the new RustTransformer (v0.2.0)
    transformer = RustTransformer()

    # Register a type (should work without schema)
    class User:
        user_id: int
        first_name: str

    transformer.register_type(User, "User")

    # Transform using new API (with fallback for now)
    result = transformer.transform(json_str, "User")

    # Should contain __typename injection
    assert '"__typename":"User"' in result
    assert '"userId"' in result
    assert '"firstName"' in result


def test_transform_json_only_camelcase() -> None:
    """Test simple camelCase transformation without typename."""
    json_str = '{"user_name": "Alice", "email_address": "alice@example.com"}'

    # Use transform_json for simple camelCase (no typename)
    result = fraiseql_rs.transform_json(json_str)

    assert '"userName"' in result
    assert '"emailAddress"' in result
    # Should NOT have __typename
    assert "__typename" not in result
