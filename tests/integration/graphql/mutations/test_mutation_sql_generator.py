from dataclasses import dataclass

import pytest
from psycopg import sql
from psycopg.types.json import Jsonb

from fraiseql.mutations.sql_generator import generate_insert_json_call
from fraiseql.types.definitions import UNSET

pytestmark = pytest.mark.integration


@pytest.mark.unit
class TestGenerateInsertJsonCall:
    """Test suite for SQL mutation query generation."""

    @pytest.fixture
    def sample_input_type(self) -> None:
        """Create a sample input type for testing."""

        @dataclass
        class UpdateUserInput:
            email: str | None = None
            name: str | None = None
            age: int | None = None
            metadata: dict | None = None

        return UpdateUserInput

    def test_generate_basic_mutation_query(self, sample_input_type) -> None:
        """Test generating a basic mutation query."""
        input_obj = sample_input_type(email="test@example.com", name="Test User")

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        # Check that we have a DatabaseQuery object
        assert hasattr(query, "statement")
        assert hasattr(query, "params")
        assert query.fetch_result is True

        # Check parameters
        assert "input_json" in query.params
        input_json = query.params["input_json"]
        assert isinstance(input_json, Jsonb)
        # All fields are included even if None
        assert input_json.obj == {
            "email": "test@example.com",
            "name": "Test User",
            "age": None,
            "metadata": None,
        }

        # Statement should be a SQL Composable
        assert isinstance(query.statement, sql.Composable)

    def test_generate_mutation_with_context(self, sample_input_type) -> None:
        """Test generating mutation with context values."""
        input_obj = sample_input_type(email="test@example.com")

        query = generate_insert_json_call(
            input_object=input_obj,
            context={"tenant_id": "org_123", "contact_id": "user_456"},
            sql_function_name="update_user",
        )

        # Check context parameters are included
        assert "auth_tenant_id" in query.params
        assert query.params["auth_tenant_id"] == "org_123"
        assert "auth_contact_id" in query.params
        assert query.params["auth_contact_id"] == "user_456"

    def test_generate_mutation_with_empty_input(self, sample_input_type) -> None:
        """Test generating mutation with empty input (all None fields)."""
        input_obj = sample_input_type()  # All fields None,

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        # None values should be included in JSON
        input_json = query.params["input_json"]
        assert isinstance(input_json, Jsonb)
        # All fields with None values should be included
        assert input_json.obj == {"email": None, "name": None, "age": None, "metadata": None}

    def test_generate_mutation_with_unset_fields(self) -> None:
        """Test generating mutation with UNSET fields."""

        @dataclass
        class UpdateUserInput:
            email: str | None = UNSET
            name: str | None = UNSET
            age: int | None = None  # This one is None, not UNSET

        input_obj = UpdateUserInput()

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        # UNSET fields should be excluded, None fields included
        input_json = query.params["input_json"]
        assert isinstance(input_json, Jsonb)
        assert input_json.obj == {"age": None}  # Only age is included

    def test_generate_mutation_with_complex_input(self, sample_input_type) -> None:
        """Test generating mutation with complex nested data."""
        input_obj = sample_input_type(
            email="test@example.com",
            metadata={
                "preferences": {"theme": "dark", "notifications": True},
                "tags": ["admin", "verified"],
            },
        )

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        input_json = query.params["input_json"]
        assert isinstance(input_json, Jsonb)
        assert input_json.obj["email"] == "test@example.com"
        assert input_json.obj["metadata"]["preferences"]["theme"] == "dark"
        assert input_json.obj["metadata"]["tags"] == ["admin", "verified"]

    def test_generate_mutation_with_special_characters(self, sample_input_type) -> None:
        """Test generating mutation with special characters in input."""
        input_obj = sample_input_type(name="O'Brien", email="test+special@example.com")

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        input_json = query.params["input_json"]
        assert input_json.obj["name"] == "O'Brien"
        assert input_json.obj["email"] == "test+special@example.com"

    def test_generate_mutation_with_custom_context_keys(self, sample_input_type) -> None:
        """Test generating mutation with custom context keys."""
        input_obj = sample_input_type(email="test@example.com")

        query = generate_insert_json_call(
            input_object=input_obj,
            context={"org_id": "org_123", "userId": "user_456"},
            sql_function_name="update_user",
            context_keys=("org_id", "userId"),  # Custom keys instead of default
        )

        # Check custom context parameters are included
        assert "auth_org_id" in query.params
        assert query.params["auth_org_id"] == "org_123"
        assert "auth_userId" in query.params
        assert query.params["auth_userId"] == "user_456"
        # Default keys should not be included
        assert "auth_tenant_id" not in query.params
        assert "auth_contact_id" not in query.params

    def test_input_with_falsy_values(self, sample_input_type) -> None:
        """Test that falsy values are properly included in the mutation."""
        input_obj = sample_input_type(
            email="",  # Empty string
            age=0,  # Zero
            metadata={},  # Empty dict
        )

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        input_json = query.params["input_json"]
        assert "email" in input_json.obj
        assert input_json.obj["email"] == ""  # Empty strings are trimmed
        assert "age" in input_json.obj
        assert input_json.obj["age"] == 0
        assert "metadata" in input_json.obj
        assert input_json.obj["metadata"] == {}

    def test_non_dataclass_input_raises_error(self) -> None:
        """Test that non-dataclass input raises TypeError."""

        class NotADataclass:
            def __init__(self) -> None:
                self.email = "test@example.com"

        with pytest.raises(TypeError, match="Expected a dataclass instance"):
            generate_insert_json_call(
                input_object=NotADataclass(), context={}, sql_function_name="update_user"
            )

    def test_string_values_are_stripped(self, sample_input_type) -> None:
        """Test that string values are stripped of whitespace."""
        input_obj = sample_input_type(email="  test@example.com  ", name="  Test User  ")

        query = generate_insert_json_call(
            input_object=input_obj, context={}, sql_function_name="update_user"
        )

        input_json = query.params["input_json"]
        assert input_json.obj["email"] == "test@example.com"
        assert input_json.obj["name"] == "Test User"
