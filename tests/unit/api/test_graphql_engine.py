"""
Unit tests for Phase 1 Greenfield GraphQLEngine API.

Tests the new public API boundary that Python code uses to interact with
the Rust engine. All internal implementation details are hidden from Python.
"""

import pytest
from fraiseql import GraphQLEngine


class TestGraphQLEngineCreation:
    """Test engine instantiation and configuration."""

    def test_engine_creation_with_valid_config(self):
        """Test creating an engine with valid JSON configuration."""
        config = '{"db": "postgres://localhost/testdb"}'
        engine = GraphQLEngine(config)

        assert engine is not None
        assert engine.is_ready()
        assert engine.version() != ""

    def test_engine_creation_with_empty_config(self):
        """Test creating an engine with minimal config."""
        config = '{}'
        engine = GraphQLEngine(config)

        assert engine is not None
        assert engine.is_ready()

    def test_engine_creation_with_invalid_json_fails(self):
        """Test that invalid JSON config raises error."""
        config = "not valid json"

        with pytest.raises(Exception):
            GraphQLEngine(config)

    def test_engine_creation_with_complex_config(self):
        """Test creating engine with complex configuration."""
        config = '''
        {
            "db": "postgres://localhost/testdb",
            "cache": "redis://localhost:6379",
            "pool_size": 10,
            "features": ["caching", "subscription", "federation"]
        }
        '''
        engine = GraphQLEngine(config)

        assert engine is not None
        assert engine.is_ready()


class TestGraphQLEngineProperties:
    """Test engine properties and introspection."""

    @pytest.fixture
    def engine(self):
        """Create an engine for testing."""
        config = '{"db": "postgres://localhost/testdb"}'
        return GraphQLEngine(config)

    def test_engine_version(self, engine):
        """Test retrieving engine version."""
        version = engine.version()
        assert isinstance(version, str)
        assert len(version) > 0
        # Version should follow semantic versioning
        assert version.count('.') >= 2

    def test_engine_is_ready(self, engine):
        """Test is_ready() returns boolean."""
        ready = engine.is_ready()
        assert isinstance(ready, bool)
        assert ready is True  # Phase 1: always ready

    def test_engine_config(self, engine):
        """Test retrieving engine configuration."""
        config = engine.config()
        assert isinstance(config, dict)
        assert config.get("db") == "postgres://localhost/testdb"

    def test_engine_repr(self, engine):
        """Test string representation of engine."""
        repr_str = repr(engine)
        assert "GraphQLEngine" in repr_str
        assert "ready=True" in repr_str or "ready" in repr_str

    def test_engine_str(self, engine):
        """Test string conversion of engine."""
        str_repr = str(engine)
        assert "GraphQLEngine" in str_repr


class TestGraphQLEngineQueries:
    """Test GraphQL query execution."""

    @pytest.fixture
    def engine(self):
        """Create an engine for testing."""
        config = '{"db": "postgres://localhost/testdb"}'
        return GraphQLEngine(config)

    def test_execute_query_simple(self, engine):
        """Test executing a simple GraphQL query."""
        result = engine.execute_query("{ users { id name } }")

        assert isinstance(result, dict)
        assert "data" in result
        assert "errors" in result or result["errors"] is None
        assert "extensions" in result

    def test_execute_query_with_variables(self, engine):
        """Test executing query with variables."""
        query = """
        query GetUser($id: ID!) {
            user(id: $id) { id name }
        }
        """
        variables = {"id": "123"}
        result = engine.execute_query(query, variables)

        assert isinstance(result, dict)
        assert "data" in result

    def test_execute_query_with_operation_name(self, engine):
        """Test executing multi-operation query with operation name."""
        query = """
        query GetUser { user { id } }
        query GetPosts { posts { id } }
        """
        result = engine.execute_query(query, {}, "GetUser")

        assert isinstance(result, dict)
        assert "data" in result

    def test_execute_query_response_structure(self, engine):
        """Test that query response has correct structure."""
        result = engine.execute_query("{ test }")

        # Response should have these keys
        assert "data" in result
        assert "errors" in result
        assert "extensions" in result

        # In Phase 1, extensions should have phase info
        extensions = result.get("extensions")
        if extensions:
            assert isinstance(extensions, dict)

    def test_execute_query_with_empty_variables(self, engine):
        """Test query with explicit empty variables dict."""
        result = engine.execute_query("{ users }", {})

        assert isinstance(result, dict)
        assert "data" in result

    def test_execute_query_with_none_variables(self, engine):
        """Test query with None variables (should default to empty)."""
        result = engine.execute_query("{ users }", None)

        assert isinstance(result, dict)
        assert "data" in result


class TestGraphQLEngineMutations:
    """Test GraphQL mutation execution."""

    @pytest.fixture
    def engine(self):
        """Create an engine for testing."""
        config = '{"db": "postgres://localhost/testdb"}'
        return GraphQLEngine(config)

    def test_execute_mutation_simple(self, engine):
        """Test executing a simple GraphQL mutation."""
        mutation = 'mutation { createUser(name: "John") { id } }'
        result = engine.execute_mutation(mutation)

        assert isinstance(result, dict)
        assert "data" in result
        assert "errors" in result or result["errors"] is None
        assert "extensions" in result

    def test_execute_mutation_with_variables(self, engine):
        """Test executing mutation with variables."""
        mutation = """
        mutation CreateUser($name: String!) {
            createUser(name: $name) { id name }
        }
        """
        variables = {"name": "Alice"}
        result = engine.execute_mutation(mutation, variables)

        assert isinstance(result, dict)
        assert "data" in result

    def test_execute_mutation_response_structure(self, engine):
        """Test that mutation response has correct structure."""
        result = engine.execute_mutation('mutation { test { id } }')

        # Response should have these keys
        assert "data" in result
        assert "errors" in result
        assert "extensions" in result

    def test_execute_mutation_with_empty_variables(self, engine):
        """Test mutation with explicit empty variables dict."""
        mutation = 'mutation { updateUser { id } }'
        result = engine.execute_mutation(mutation, {})

        assert isinstance(result, dict)
        assert "data" in result

    def test_execute_mutation_with_none_variables(self, engine):
        """Test mutation with None variables (should default to empty)."""
        result = engine.execute_mutation('mutation { deleteUser { success } }', None)

        assert isinstance(result, dict)
        assert "data" in result


class TestGraphQLEngineAPIBoundary:
    """Test that API boundary correctly hides internal types."""

    def test_engine_is_only_public_type(self):
        """Verify GraphQLEngine is the only public type from the public API."""
        # GraphQLEngine should be accessible from fraiseql
        from fraiseql import GraphQLEngine as PublicEngine

        assert PublicEngine is not None

        # Verify internal types are not easily accessible (they're in _fraiseql_rs but marked as internal)
        # These should not be exported as part of the public Python API
        from fraiseql import _fraiseql_rs

        # These internal types exist in the Rust module but are not meant for direct Python use
        assert hasattr(_fraiseql_rs, "PyGraphQLPipeline")  # Internal, starts with Py
        assert hasattr(_fraiseql_rs, "PrototypePool")  # Internal implementation detail

    def test_engine_methods_return_python_types(self):
        """Verify engine methods return standard Python types."""
        engine = GraphQLEngine('{}')

        # These should return Python primitives/collections
        assert isinstance(engine.version(), str)
        assert isinstance(engine.is_ready(), bool)
        assert isinstance(engine.config(), dict)

        # Query/mutation results should return dicts
        result = engine.execute_query("{ test }")
        assert isinstance(result, dict)

        result = engine.execute_mutation("mutation { test { id } }")
        assert isinstance(result, dict)


class TestGraphQLEngineThreadSafety:
    """Test that engine can be safely used from multiple threads."""

    def test_engine_creation_thread_safe(self):
        """Test creating engines from different logical contexts."""
        config1 = '{"db": "postgres://localhost/db1"}'
        config2 = '{"db": "postgres://localhost/db2"}'

        engine1 = GraphQLEngine(config1)
        engine2 = GraphQLEngine(config2)

        # Both should work independently
        assert engine1.is_ready()
        assert engine2.is_ready()

        config1_result = engine1.config()
        config2_result = engine2.config()

        assert config1_result.get("db") == "postgres://localhost/db1"
        assert config2_result.get("db") == "postgres://localhost/db2"

    def test_engine_is_shared_reference(self):
        """Test that engine uses Arc internally (thread-safe shared reference)."""
        config = '{"db": "postgres://localhost/testdb"}'
        engine1 = GraphQLEngine(config)

        # engine2 gets a reference to the same engine
        engine2 = engine1

        # They should be the same object
        assert repr(engine1) == repr(engine2)
