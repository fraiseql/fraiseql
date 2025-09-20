"""Integration tests for automatic docstring extraction in GraphQL schema introspection."""

import pytest
from graphql import execute_sync, parse

import fraiseql
from fraiseql.gql.schema_builder import build_fraiseql_schema


class TestSchemaDescriptions:
    """Test that descriptions appear correctly in GraphQL introspection queries."""

    def test_graphql_introspection_includes_type_descriptions(self):
        """Test that GraphQL introspection returns type descriptions from docstrings."""

        @fraiseql.type(sql_source="users")
        class User:
            """A user account with authentication and profile information."""

            id: int
            name: str
            email: str

        @fraiseql.query
        async def get_user(info, user_id: int) -> User:
            """Retrieve a specific user by their ID."""
            # Mock implementation for testing
            return None

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[User, get_user],
            mutation_resolvers=[],
        )

        # Test basic type introspection for description only
        type_introspection_query = """
        query {
            __type(name: "User") {
                name
                description
            }
        }
        """

        result = execute_sync(schema, parse(type_introspection_query))
        assert result.errors is None
        assert result.data is not None

        user_type = result.data["__type"]
        assert user_type["name"] == "User"
        assert user_type["description"] == "A user account with authentication and profile information."

        # Test basic query field introspection for description only
        query_introspection_query = """
        query {
            __schema {
                queryType {
                    fields {
                        name
                        description
                    }
                }
            }
        }
        """

        result = execute_sync(schema, parse(query_introspection_query))
        assert result.errors is None
        assert result.data is not None

        query_fields = result.data["__schema"]["queryType"]["fields"]
        get_user_field = next((f for f in query_fields if f["name"] == "getUser"), None)
        assert get_user_field is not None
        assert get_user_field["description"] == "Retrieve a specific user by their ID."

    def test_graphql_introspection_includes_mutation_descriptions(self):
        """Test that GraphQL introspection returns mutation descriptions from docstrings."""

        @fraiseql.input
        class CreateUserInput:
            name: str
            email: str

        @fraiseql.success
        class CreateUserSuccess:
            id: int
            message: str

        @fraiseql.failure
        class CreateUserError:
            message: str

        @fraiseql.mutation
        class CreateUser:
            """Create a new user account with validation and welcome email."""

            input: CreateUserInput
            success: CreateUserSuccess
            failure: CreateUserError

            async def resolve(self, info):
                return CreateUserSuccess(id=1, message="User created successfully")

        # Need at least one query for valid schema
        @fraiseql.query
        async def dummy_query(info) -> str:
            return "dummy"

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[dummy_query],
            mutation_resolvers=[CreateUser],
        )

        # Test basic mutation field introspection for description only
        mutation_introspection_query = """
        query {
            __schema {
                mutationType {
                    fields {
                        name
                        description
                    }
                }
            }
        }
        """

        result = execute_sync(schema, parse(mutation_introspection_query))
        assert result.errors is None
        assert result.data is not None

        mutation_fields = result.data["__schema"]["mutationType"]["fields"]
        create_user_field = next((f for f in mutation_fields if f["name"] == "createUser"), None)
        assert create_user_field is not None
        assert create_user_field["description"] == "Create a new user account with validation and welcome email."

    def test_graphql_introspection_apollo_studio_compatible(self):
        """Test that the schema introspection is compatible with Apollo Studio requirements."""

        @fraiseql.type(sql_source="products")
        class Product:
            """A product in the e-commerce catalog with pricing and inventory."""

            id: int
            name: str
            description: str
            price: float

        @fraiseql.input
        class UpdatePriceInput:
            product_id: int
            new_price: float

        @fraiseql.success
        class UpdatePriceSuccess:
            product: Product
            message: str

        @fraiseql.failure
        class UpdatePriceError:
            message: str

        @fraiseql.query
        async def get_products(info) -> list[Product]:
            """Get all products in the catalog with current pricing."""
            return []

        @fraiseql.mutation
        class UpdatePrice:
            """Update the price of a product with validation and audit logging."""

            input: UpdatePriceInput
            success: UpdatePriceSuccess
            failure: UpdatePriceError

            async def resolve(self, info):
                # Mock implementation for testing
                return None

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[Product, get_products],
            mutation_resolvers=[UpdatePrice],
        )

        # Full Apollo Studio-style introspection query
        full_introspection_query = """
        query {
            __schema {
                queryType {
                    name
                    description
                    fields {
                        name
                        description
                        type {
                            name
                            kind
                        }
                        args {
                            name
                            description
                            type {
                                name
                            }
                        }
                    }
                }
                mutationType {
                    name
                    description
                    fields {
                        name
                        description
                        type {
                            name
                            kind
                        }
                        args {
                            name
                            description
                            type {
                                name
                            }
                        }
                    }
                }
                types {
                    name
                    description
                    kind
                    fields {
                        name
                        description
                        type {
                            name
                            kind
                        }
                    }
                }
            }
        }
        """

        result = execute_sync(schema, parse(full_introspection_query))
        assert result.errors is None
        assert result.data is not None

        # Verify that all descriptions are present
        schema_data = result.data["__schema"]

        # Check Product type description
        product_type = next((t for t in schema_data["types"] if t["name"] == "Product"), None)
        assert product_type is not None
        assert product_type["description"] == "A product in the e-commerce catalog with pricing and inventory."

        # Check query field description
        query_fields = schema_data["queryType"]["fields"]
        get_products_field = next((f for f in query_fields if f["name"] == "getProducts"), None)
        assert get_products_field is not None
        assert get_products_field["description"] == "Get all products in the catalog with current pricing."

        # Check mutation field description
        mutation_fields = schema_data["mutationType"]["fields"]
        update_price_field = next((f for f in mutation_fields if f["name"] == "updatePrice"), None)
        assert update_price_field is not None
        assert update_price_field["description"] == "Update the price of a product with validation and audit logging."
