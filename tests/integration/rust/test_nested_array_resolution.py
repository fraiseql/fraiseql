"""Test fraiseql_rs schema-aware nested array resolution.

Phase 5, TDD Cycle 5.1 - RED: Test schema-based automatic type resolution
These tests should FAIL initially because the function doesn't exist yet.

This phase builds on Phase 4's typename injection by adding:
- Schema registry for automatic type detection
- Array field detection
- Polymorphic array support (union types)
- Cleaner API with schema awareness
"""
import json
import pytest


def test_schema_based_transformation_simple():
    """Test transformation with schema definition (no manual type map).

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing schema support, this should pass
    """
    import fraiseql_rs

    # Define schema
    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "name": "String",
                "email": "String",
            }
        }
    }

    input_json = '{"id": 1, "name": "John", "email": "john@example.com"}'
    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    assert result == {
        "__typename": "User",
        "id": 1,
        "name": "John",
        "email": "john@example.com",
    }


def test_schema_based_transformation_with_array():
    """Test automatic array type resolution from schema."""
    import fraiseql_rs

    # Schema defines that 'posts' is an array of Post objects
    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "name": "String",
                "posts": "[Post]",  # Array field notation
            }
        },
        "Post": {
            "fields": {
                "id": "Int",
                "title": "String",
            }
        },
    }

    input_json = json.dumps({
        "id": 1,
        "name": "John",
        "posts": [
            {"id": 1, "title": "First Post"},
            {"id": 2, "title": "Second Post"},
        ],
    })

    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    # Should automatically detect and apply Post typename to array elements
    assert result["__typename"] == "User"
    assert result["posts"][0]["__typename"] == "Post"
    assert result["posts"][0]["id"] == 1
    assert result["posts"][1]["__typename"] == "Post"


def test_schema_based_nested_arrays():
    """Test deeply nested array resolution (User → Posts → Comments)."""
    import fraiseql_rs

    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "name": "String",
                "posts": "[Post]",
            }
        },
        "Post": {
            "fields": {
                "id": "Int",
                "title": "String",
                "comments": "[Comment]",
            }
        },
        "Comment": {
            "fields": {
                "id": "Int",
                "text": "String",
            }
        },
    }

    input_json = json.dumps({
        "id": 1,
        "name": "John",
        "posts": [
            {
                "id": 1,
                "title": "First",
                "comments": [
                    {"id": 1, "text": "Great!"},
                    {"id": 2, "text": "Thanks!"},
                ],
            }
        ],
    })

    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    # All levels should have correct __typename
    assert result["__typename"] == "User"
    assert result["posts"][0]["__typename"] == "Post"
    assert result["posts"][0]["comments"][0]["__typename"] == "Comment"
    assert result["posts"][0]["comments"][0]["text"] == "Great!"


def test_schema_based_nullable_fields():
    """Test handling of nullable fields (None values)."""
    import fraiseql_rs

    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "name": "String",
                "profile": "Profile",  # Nullable object (can be None)
            }
        },
        "Profile": {
            "fields": {
                "bio": "String",
            }
        },
    }

    # Test with null profile
    input_json = json.dumps({"id": 1, "name": "John", "profile": None})
    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["profile"] is None

    # Test with actual profile
    input_json = json.dumps({"id": 1, "name": "John", "profile": {"bio": "Developer"}})
    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["profile"]["__typename"] == "Profile"
    assert result["profile"]["bio"] == "Developer"


def test_schema_based_empty_arrays():
    """Test handling of empty arrays."""
    import fraiseql_rs

    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "posts": "[Post]",
            }
        },
        "Post": {
            "fields": {
                "id": "Int",
            }
        },
    }

    input_json = json.dumps({"id": 1, "posts": []})
    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["posts"] == []


def test_schema_based_mixed_fields():
    """Test object with mix of scalars, objects, and arrays."""
    import fraiseql_rs

    schema = {
        "User": {
            "fields": {
                "id": "Int",
                "name": "String",
                "is_active": "Boolean",
                "profile": "Profile",
                "posts": "[Post]",
            }
        },
        "Profile": {
            "fields": {
                "bio": "String",
            }
        },
        "Post": {
            "fields": {
                "id": "Int",
                "title": "String",
            }
        },
    }

    input_json = json.dumps({
        "id": 1,
        "name": "John",
        "is_active": True,
        "profile": {"bio": "Developer"},
        "posts": [{"id": 1, "title": "First"}],
    })

    result_json = fraiseql_rs.transform_with_schema(input_json, "User", schema)
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["id"] == 1
    assert result["name"] == "John"
    assert result["isActive"] is True
    assert result["profile"]["__typename"] == "Profile"
    assert result["posts"][0]["__typename"] == "Post"


def test_schema_registry():
    """Test SchemaRegistry for registering and reusing schemas."""
    import fraiseql_rs

    # Create a schema registry
    registry = fraiseql_rs.SchemaRegistry()

    # Register types
    registry.register_type("User", {
        "fields": {
            "id": "Int",
            "name": "String",
            "posts": "[Post]",
        }
    })

    registry.register_type("Post", {
        "fields": {
            "id": "Int",
            "title": "String",
        }
    })

    input_json = json.dumps({
        "id": 1,
        "name": "John",
        "posts": [{"id": 1, "title": "First"}],
    })

    # Transform using registry
    result_json = registry.transform(input_json, "User")
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["posts"][0]["__typename"] == "Post"


def test_backward_compatibility_with_phase4():
    """Test that Phase 4's transform_json_with_typename still works."""
    import fraiseql_rs

    # Phase 4 API should still work
    type_map = {"$": "User", "posts": "Post"}
    input_json = json.dumps({"id": 1, "posts": [{"id": 1}]})

    result_json = fraiseql_rs.transform_json_with_typename(input_json, type_map)
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["posts"][0]["__typename"] == "Post"
    # This test should pass with Phase 4 implementation


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
