"""Tests for automatic docstring extraction in FraiseQL query and mutation descriptions."""

import pytest
from graphql import build_schema, print_schema

import fraiseql
from fraiseql.gql.schema_builder import build_fraiseql_schema


class TestQueryDescriptions:
    """Test that @query decorated functions automatically use docstrings for GraphQL descriptions."""

    def test_query_uses_docstring_as_description(self):
        """Test that @query decorated functions use their docstring as GraphQL field description."""

        @fraiseql.query
        async def get_user_profile(info, user_id: int) -> str:
            """Retrieve the user's profile information and settings."""
            return f"Profile for user {user_id}"

        # Build schema to get the query type
        schema = build_fraiseql_schema(
            query_types=[get_user_profile],
            mutation_resolvers=[],
        )

        # Check that the query field has the correct description
        query_type = schema.query_type
        assert query_type is not None

        user_profile_field = query_type.fields.get("getUserProfile")  # camelCase by default
        assert user_profile_field is not None
        assert user_profile_field.description == "Retrieve the user's profile information and settings."

    def test_query_without_docstring_has_no_description(self):
        """Test that @query functions without docstrings have no description."""

        @fraiseql.query
        async def get_data(info) -> str:
            return "test data"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[get_data],
            mutation_resolvers=[],
        )

        # Check that the query field has no description
        query_type = schema.query_type
        assert query_type is not None

        data_field = query_type.fields.get("getData")
        assert data_field is not None
        assert data_field.description is None

    def test_query_multiline_docstring_is_cleaned(self):
        """Test that multiline docstrings are properly cleaned and formatted."""

        @fraiseql.query
        async def search_products(info, query: str) -> str:
            """
            Search for products in the catalog.

            Performs a full-text search across product names,
            descriptions, and categories.
            """
            return f"Search results for: {query}"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[search_products],
            mutation_resolvers=[],
        )

        # Check that the query field has cleaned description
        query_type = schema.query_type
        assert query_type is not None

        search_field = query_type.fields.get("searchProducts")
        assert search_field is not None

        expected_description = "Search for products in the catalog.\n\nPerforms a full-text search across product names,\ndescriptions, and categories."
        assert search_field.description == expected_description

    def test_query_description_preserved_with_existing_functionality(self):
        """Test that adding docstring descriptions doesn't break existing query functionality."""

        @fraiseql.type(sql_source="users")
        class User:
            """A user in the system."""
            id: int
            name: str

        @fraiseql.query
        async def get_users(info) -> list[User]:
            """Get all users in the system."""
            return []  # Mock implementation

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[User, get_users],
            mutation_resolvers=[],
        )

        # Check that the query field has description
        query_type = schema.query_type
        assert query_type is not None

        users_field = query_type.fields.get("getUsers")
        assert users_field is not None
        assert users_field.description == "Get all users in the system."

        # Check that User type also has description (from Phase 1)
        user_type = schema.type_map.get("User")
        assert user_type is not None
        assert user_type.description == "A user in the system."


class TestMutationDescriptions:
    """Test that @mutation decorated classes automatically use docstrings for GraphQL descriptions."""

    def test_mutation_uses_docstring_as_description(self):
        """Test that @mutation decorated classes use their docstring as GraphQL field description."""

        @fraiseql.input
        class CreateUserInput:
            name: str
            email: str

        @fraiseql.success
        class CreateUserSuccess:
            message: str

        @fraiseql.failure
        class CreateUserError:
            message: str

        @fraiseql.mutation
        class CreateUser:
            """Create a new user account with validation."""

            input: CreateUserInput
            success: CreateUserSuccess
            failure: CreateUserError

            async def resolve(self, info):
                return CreateUserSuccess(message="User created")

        # Need at least one query for valid GraphQL schema
        @fraiseql.query
        async def dummy_query(info) -> str:
            return "dummy"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[dummy_query],
            mutation_resolvers=[CreateUser],
        )

        # Check that the mutation field has the correct description
        mutation_type = schema.mutation_type
        assert mutation_type is not None

        create_user_field = mutation_type.fields.get("createUser")
        assert create_user_field is not None
        assert create_user_field.description == "Create a new user account with validation."

    def test_mutation_without_docstring_has_no_description(self):
        """Test that @mutation classes without docstrings have no description."""

        @fraiseql.input
        class UpdateDataInput:
            value: str

        @fraiseql.success
        class UpdateDataSuccess:
            message: str

        @fraiseql.failure
        class UpdateDataError:
            message: str

        @fraiseql.mutation
        class UpdateData:
            input: UpdateDataInput
            success: UpdateDataSuccess
            failure: UpdateDataError

            async def resolve(self, info):
                return UpdateDataSuccess(message="Data updated")

        # Need at least one query for valid GraphQL schema
        @fraiseql.query
        async def dummy_query2(info) -> str:
            return "dummy"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[dummy_query2],
            mutation_resolvers=[UpdateData],
        )

        # Check that the mutation field has no description
        mutation_type = schema.mutation_type
        assert mutation_type is not None

        update_field = mutation_type.fields.get("updateData")
        assert update_field is not None
        assert update_field.description is None

    def test_mutation_multiline_docstring_is_cleaned(self):
        """Test that mutation multiline docstrings are properly cleaned and formatted."""

        @fraiseql.input
        class ProcessOrderInput:
            order_id: int

        @fraiseql.success
        class ProcessOrderSuccess:
            message: str

        @fraiseql.failure
        class ProcessOrderError:
            message: str

        @fraiseql.mutation
        class ProcessOrder:
            """
            Process a customer order through the fulfillment pipeline.

            Validates inventory, calculates shipping costs,
            and initiates payment processing.
            """

            input: ProcessOrderInput
            success: ProcessOrderSuccess
            failure: ProcessOrderError

            async def resolve(self, info):
                return ProcessOrderSuccess(message="Order processed")

        # Need at least one query for valid GraphQL schema
        @fraiseql.query
        async def dummy_query3(info) -> str:
            return "dummy"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[dummy_query3],
            mutation_resolvers=[ProcessOrder],
        )

        # Check that the mutation field has cleaned description
        mutation_type = schema.mutation_type
        assert mutation_type is not None

        process_field = mutation_type.fields.get("processOrder")
        assert process_field is not None

        expected_description = "Process a customer order through the fulfillment pipeline.\n\nValidates inventory, calculates shipping costs,\nand initiates payment processing."
        assert process_field.description == expected_description
