"""
Integration tests for TOML-based schema workflow (Phase 2, Cycle 4)

Tests the complete workflow:
1. Language SDK generates minimal types.json
2. TOML defines queries, mutations, federation, security, observers
3. fraiseql-cli merge combines both into schema.compiled.json
4. Server loads compiled schema with all configuration

This validates the Tier 1 refactoring: Python, TypeScript, and Java
all use TOML-based configuration for queries, mutations, federation,
security, observers, and analytics.
"""

import json
import os
import subprocess
import tempfile
from pathlib import Path


# ==============================================================================
# FIXTURES AND UTILITIES
# ==============================================================================


def create_test_directory():
    """Create temporary test directory with fraiseql.toml and types.json"""
    tmpdir = tempfile.mkdtemp(prefix="fraiseql_toml_test_")
    return Path(tmpdir)


def load_json(path):
    """Load JSON file"""
    with open(path) as f:
        return json.load(f)


def save_json(path, obj):
    """Save JSON file"""
    with open(path, "w") as f:
        json.dump(obj, f, indent=2)


# ==============================================================================
# PYTHON TESTS
# ==============================================================================


def test_python_toml_workflow():
    """Test Python SDK + TOML compilation workflow

    Workflow:
    1. Python decorator generates types.json with @type decorator
    2. fraiseql.toml defines queries, mutations, federation, security, observers
    3. fraiseql-cli compile combines them into schema.compiled.json
    4. Verify compiled schema has types from Python + config from TOML
    """
    tmpdir = create_test_directory()

    # Create minimal Python types.json
    types_json = {
        "types": [
            {
                "name": "User",
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "name": {"type": "String", "nullable": False},
                    "email": {"type": "String", "nullable": False},
                },
            }
        ]
    }
    types_file = tmpdir / "types.json"
    save_json(types_file, types_json)

    # Create fraiseql.toml with queries, mutations, etc.
    toml_content = """
[fraiseql]
version = "2.0"

[fraiseql.queries.users]
return_type = "User"
returns_list = true

[fraiseql.mutations.createUser]
return_type = "User"
operation = "CREATE"

[fraiseql.security]
enabled = true

[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"
"""
    toml_file = tmpdir / "fraiseql.toml"
    toml_file.write_text(toml_content)

    # Run fraiseql-cli compile with --types parameter
    output_file = tmpdir / "schema.compiled.json"
    result = subprocess.run(
        ["fraiseql", "compile", str(toml_file), "--types", str(types_file), "-o", str(output_file)],
        capture_output=True,
        text=True,
    )

    # Verify compilation succeeded
    assert result.returncode == 0, f"Compilation failed: {result.stderr}"
    assert output_file.exists(), "schema.compiled.json not created"

    # Load and verify compiled schema
    compiled = load_json(output_file)

    # Should have types from Python
    assert "types" in compiled, "Missing 'types' section"
    assert len(compiled["types"]) > 0, "No types in compiled schema"
    assert any(t["name"] == "User" for t in compiled["types"]), "Missing User type"

    # Should have queries from TOML
    assert "queries" in compiled, "Missing 'queries' section"
    assert "users" in compiled["queries"], "Missing 'users' query from TOML"

    # Should have mutations from TOML
    assert "mutations" in compiled, "Missing 'mutations' section"
    assert "createUser" in compiled["mutations"], "Missing 'createUser' mutation from TOML"

    # Should have security from TOML
    assert "security" in compiled, "Missing 'security' section"
    assert compiled["security"]["enabled"], "Security not enabled"

    # Should have observers from TOML
    assert "observers" in compiled, "Missing 'observers' section"
    assert "userCreated" in compiled["observers"], "Missing 'userCreated' observer from TOML"

    print("✅ Python + TOML workflow test passed")


def test_typescript_toml_workflow():
    """Test TypeScript SDK + TOML compilation workflow

    Workflow:
    1. TypeScript decorator generates types.json with @Type decorator
    2. fraiseql.toml defines queries, mutations, federation, security, observers
    3. fraiseql-cli compile combines them into schema.compiled.json
    4. Verify compiled schema has types from TypeScript + config from TOML
    """
    tmpdir = create_test_directory()

    # Create minimal TypeScript types.json
    types_json = {
        "types": [
            {
                "name": "Post",
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "title": {"type": "String", "nullable": False},
                    "content": {"type": "String", "nullable": False},
                },
            }
        ]
    }
    types_file = tmpdir / "types.json"
    save_json(types_file, types_json)

    # Create fraiseql.toml
    toml_content = """
[fraiseql]
version = "2.0"

[fraiseql.queries.posts]
return_type = "Post"
returns_list = true

[fraiseql.mutations.createPost]
return_type = "Post"
operation = "CREATE"

[fraiseql.federation]
enabled = false

[fraiseql.observers.postCreated]
entity = "Post"
event = "INSERT"
"""
    toml_file = tmpdir / "fraiseql.toml"
    toml_file.write_text(toml_content)

    # Run fraiseql-cli compile
    output_file = tmpdir / "schema.compiled.json"
    result = subprocess.run(
        ["fraiseql", "compile", str(toml_file), "--types", str(types_file), "-o", str(output_file)],
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, f"Compilation failed: {result.stderr}"
    assert output_file.exists(), "schema.compiled.json not created"

    # Verify compiled schema
    compiled = load_json(output_file)

    # Types from TypeScript
    assert any(t["name"] == "Post" for t in compiled["types"]), "Missing Post type"

    # Queries from TOML
    assert "posts" in compiled["queries"], "Missing 'posts' query from TOML"

    # Mutations from TOML
    assert "createPost" in compiled["mutations"], "Missing 'createPost' mutation from TOML"

    # Observers from TOML
    assert "postCreated" in compiled["observers"], "Missing 'postCreated' observer from TOML"

    print("✅ TypeScript + TOML workflow test passed")


def test_java_toml_workflow():
    """Test Java SDK + TOML compilation workflow

    Workflow:
    1. Java annotation generates types.json with @GraphQLType annotation
    2. fraiseql.toml defines queries, mutations, federation, security, observers
    3. fraiseql-cli compile combines them into schema.compiled.json
    4. Verify compiled schema has types from Java + config from TOML
    """
    tmpdir = create_test_directory()

    # Create minimal Java types.json
    types_json = {
        "types": [
            {
                "name": "Order",
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "total": {"type": "Float", "nullable": False},
                    "status": {"type": "String", "nullable": False},
                },
            }
        ]
    }
    types_file = tmpdir / "types.json"
    save_json(types_file, types_json)

    # Create fraiseql.toml
    toml_content = """
[fraiseql]
version = "2.0"

[fraiseql.queries.orders]
return_type = "Order"
returns_list = true

[fraiseql.mutations.createOrder]
return_type = "Order"
operation = "CREATE"

[fraiseql.security.rate_limiting]
enabled = true

[fraiseql.observers.orderCreated]
entity = "Order"
event = "INSERT"
condition = "total > 100"
"""
    toml_file = tmpdir / "fraiseql.toml"
    toml_file.write_text(toml_content)

    # Run fraiseql-cli compile
    output_file = tmpdir / "schema.compiled.json"
    result = subprocess.run(
        ["fraiseql", "compile", str(toml_file), "--types", str(types_file), "-o", str(output_file)],
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, f"Compilation failed: {result.stderr}"
    assert output_file.exists(), "schema.compiled.json not created"

    # Verify compiled schema
    compiled = load_json(output_file)

    # Types from Java
    assert any(t["name"] == "Order" for t in compiled["types"]), "Missing Order type"

    # Queries from TOML
    assert "orders" in compiled["queries"], "Missing 'orders' query from TOML"

    # Mutations from TOML
    assert "createOrder" in compiled["mutations"], "Missing 'createOrder' mutation from TOML"

    # Security from TOML
    assert compiled["security"]["rate_limiting"]["enabled"], "Rate limiting not enabled"

    # Observers from TOML
    assert "orderCreated" in compiled["observers"], "Missing 'orderCreated' observer from TOML"
    assert compiled["observers"]["orderCreated"]["condition"] == "total > 100", "Observer condition not set"

    print("✅ Java + TOML workflow test passed")


# ==============================================================================
# COMBINED WORKFLOW TESTS
# ==============================================================================


def test_all_three_languages_with_single_toml():
    """Test combining types.json from all three languages with single TOML

    This validates that developers can choose different languages for
    different parts of their API while using shared TOML configuration.
    """
    tmpdir = create_test_directory()

    # Create merged types.json from all three languages
    types_json = {
        "types": [
            {
                "name": "User",  # From Python
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "name": {"type": "String", "nullable": False},
                },
            },
            {
                "name": "Post",  # From TypeScript
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "title": {"type": "String", "nullable": False},
                },
            },
            {
                "name": "Order",  # From Java
                "fields": {
                    "id": {"type": "ID", "nullable": False},
                    "total": {"type": "Float", "nullable": False},
                },
            },
        ]
    }
    types_file = tmpdir / "types.json"
    save_json(types_file, types_json)

    # Single TOML for all types
    toml_content = """
[fraiseql]
version = "2.0"

[fraiseql.queries.users]
return_type = "User"
returns_list = true

[fraiseql.queries.posts]
return_type = "Post"
returns_list = true

[fraiseql.queries.orders]
return_type = "Order"
returns_list = true

[fraiseql.security]
enabled = true

[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"

[fraiseql.observers.postCreated]
entity = "Post"
event = "INSERT"

[fraiseql.observers.orderCreated]
entity = "Order"
event = "INSERT"
"""
    toml_file = tmpdir / "fraiseql.toml"
    toml_file.write_text(toml_content)

    # Compile
    output_file = tmpdir / "schema.compiled.json"
    result = subprocess.run(
        ["fraiseql", "compile", str(toml_file), "--types", str(types_file), "-o", str(output_file)],
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, f"Compilation failed: {result.stderr}"

    # Verify all types from all languages are present
    compiled = load_json(output_file)
    type_names = {t["name"] for t in compiled["types"]}
    assert "User" in type_names, "Missing User type from Python"
    assert "Post" in type_names, "Missing Post type from TypeScript"
    assert "Order" in type_names, "Missing Order type from Java"

    # Verify queries
    assert len(compiled["queries"]) >= 3, "Should have queries for all three types"

    # Verify observers
    assert len(compiled["observers"]) >= 3, "Should have observers for all three types"

    print("✅ All three languages combined test passed")


def test_toml_validation_errors():
    """Test that compilation fails gracefully with invalid TOML"""
    tmpdir = create_test_directory()

    types_json = {"types": []}
    types_file = tmpdir / "types.json"
    save_json(types_file, types_json)

    # Invalid TOML (missing required fields)
    toml_content = """
[fraiseql]
[fraiseql.queries.invalidQuery]
return_type = "NonExistentType"
"""
    toml_file = tmpdir / "fraiseql.toml"
    toml_file.write_text(toml_content)

    output_file = tmpdir / "schema.compiled.json"
    result = subprocess.run(
        ["fraiseql", "compile", str(toml_file), "--types", str(types_file), "-o", str(output_file)],
        capture_output=True,
        text=True,
    )

    # Should fail due to invalid schema
    assert result.returncode != 0, "Should have failed with invalid schema"
    assert not output_file.exists() or not load_json(output_file), "Should not create valid output"

    print("✅ TOML validation test passed")


if __name__ == "__main__":
    # Run all tests
    print("🧪 Running TOML-based workflow integration tests...\n")

    try:
        test_python_toml_workflow()
        test_typescript_toml_workflow()
        test_java_toml_workflow()
        test_all_three_languages_with_single_toml()
        test_toml_validation_errors()

        print("\n✅ All integration tests passed!")
    except AssertionError as e:
        print(f"\n❌ Test failed: {e}")
        exit(1)
    except Exception as e:
        print(f"\n❌ Error: {e}")
        exit(1)
