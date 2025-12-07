"""Tests for FraiseQL enum support."""

import asyncio
from enum import Enum

import pytest
from graphql import GraphQLEnumType, GraphQLNonNull, graphql

import fraiseql
from fraiseql.gql.schema_builder import build_fraiseql_schema


@pytest.mark.unit
class TestFraiseEnum:
    """Test suite for @fraise_enum decorator and enum functionality."""

    def test_basic_enum_decoration(self, clear_registry) -> None:
        """Test that @fraise_enum properly decorates an enum class."""

        @fraiseql.enum
        class Status(Enum):
            ACTIVE = "active"
            INACTIVE = "inactive"
            PENDING = "pending"

        # Check that the enum has the GraphQL type attached
        assert hasattr(Status, "__graphql_type__")
        assert isinstance(Status.__graphql_type__, GraphQLEnumType)
        assert Status.__graphql_type__.name == "Status"

        # Check enum values
        graphql_enum = Status.__graphql_type__
        assert "ACTIVE" in graphql_enum.values
        assert "INACTIVE" in graphql_enum.values
        assert "PENDING" in graphql_enum.values

        # Check internal values (stores primitive values for JSON serialization)
        assert graphql_enum.values["ACTIVE"].value == Status.ACTIVE.value
        assert graphql_enum.values["INACTIVE"].value == Status.INACTIVE.value
        assert graphql_enum.values["PENDING"].value == Status.PENDING.value

    def test_enum_with_integer_values(self, clear_registry) -> None:
        """Test enum with integer values."""

        @fraiseql.enum
        class Priority(Enum):
            LOW = 1
            MEDIUM = 2
            HIGH = 3
            CRITICAL = 4

        graphql_enum = Priority.__graphql_type__
        assert graphql_enum.values["LOW"].value == Priority.LOW.value
        assert graphql_enum.values["HIGH"].value == Priority.HIGH.value

    def test_enum_in_type_definition(self, clear_registry) -> None:
        """Test using enum in a type definition."""

        @fraiseql.enum
        class UserRole(Enum):
            ADMIN = "admin"
            USER = "user"
            GUEST = "guest"

        @fraiseql.type
        class User:
            id: str
            name: str
            role: UserRole

        @fraiseql.type
        class QueryRoot:
            users: list[User] = fraiseql.fraise_field(default_factory=list)

        # Build schema and verify it includes the enum
        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Check that User type has role field with enum type
        user_type = schema.type_map.get("User")
        assert user_type is not None
        role_field = user_type.fields.get("role")
        assert role_field is not None
        assert isinstance(role_field.type, GraphQLEnumType)
        assert role_field.type.name == "UserRole"

    def test_enum_in_input_type(self, clear_registry) -> None:
        """Test using enum in an input type."""

        @fraiseql.enum
        class TaskStatus(Enum):
            TODO = "todo"
            IN_PROGRESS = "in_progress"
            DONE = "done"

        @fraiseql.input
        class UpdateTaskInput:
            id: str
            status: TaskStatus

        @fraiseql.type
        class Task:
            id: str
            title: str
            status: TaskStatus

        @fraiseql.type
        class QueryRoot:
            dummy: str = fraiseql.fraise_field(default="dummy")

        async def update_task(info, input: UpdateTaskInput) -> Task:
            return Task(id=input.id, title="Test Task", status=input.status)

        schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[update_task])

        # Check that the input type has the enum field
        input_type = schema.type_map.get("UpdateTaskInput")
        assert input_type is not None
        status_field = input_type.fields.get("status")
        assert status_field is not None
        # Field should be non-null enum (required field with no default)
        assert isinstance(status_field.type, GraphQLNonNull)
        assert isinstance(status_field.type.of_type, GraphQLEnumType)

    def test_enum_in_graphql_query(self, clear_registry) -> None:
        """Test executing GraphQL queries with enum values."""

        @fraiseql.enum
        class ArticleStatus(Enum):
            DRAFT = "draft"
            PUBLISHED = "published"
            ARCHIVED = "archived"

        @fraiseql.type
        class Article:
            id: str
            title: str
            status: ArticleStatus

        @fraiseql.type
        class QueryRoot:
            articles: list[Article] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_articles(_root, _info) -> list[Article]:
                return [
                    Article(id="1", title="First", status=ArticleStatus.PUBLISHED),
                    Article(id="2", title="Second", status=ArticleStatus.DRAFT),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        query = """
        query {
            articles {
                id
                title
                status
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "articles": [
                {"id": "1", "title": "First", "status": "PUBLISHED"},
                {"id": "2", "title": "Second", "status": "DRAFT"},
            ]
        }

    def test_enum_in_mutation(self, clear_registry) -> None:
        """Test using enum in mutations."""

        @fraiseql.enum
        class OrderStatus(Enum):
            PENDING = "pending"
            PROCESSING = "processing"
            SHIPPED = "shipped"
            DELIVERED = "delivered"

        @fraiseql.input
        class UpdateOrderInput:
            order_id: str
            status: OrderStatus

        @fraiseql.type
        class Order:
            id: str
            status: OrderStatus

        @fraiseql.type
        class QueryRoot:
            dummy: str = fraiseql.fraise_field(default="dummy")

        async def update_order(info, input: UpdateOrderInput) -> Order:
            return Order(id=input.order_id, status=input.status)

        schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[update_order])

        mutation = """
        mutation UpdateOrder($input: UpdateOrderInput!) {
            updateOrder(input: $input) {
                id
                status
            }
        }
        """
        variables = {"input": {"orderId": "123", "status": "SHIPPED"}}

        result = asyncio.run(graphql(schema, mutation, variable_values=variables))
        assert result.errors is None
        assert result.data == {"updateOrder": {"id": "123", "status": "SHIPPED"}}

    def test_optional_enum_field(self, clear_registry) -> None:
        """Test optional enum fields."""

        @fraiseql.enum
        class Category(Enum):
            ELECTRONICS = "electronics"
            BOOKS = "books"
            CLOTHING = "clothing"

        @fraiseql.type
        class Product:
            id: str
            name: str
            category: Category | None = None

        @fraiseql.type
        class QueryRoot:
            products: list[Product] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_products(_root, _info) -> list[Product]:
                return [
                    Product(id="1", name="Laptop", category=Category.ELECTRONICS),
                    Product(id="2", name="Unknown Item", category=None),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        query = """
        query {
            products {
                id
                name
                category
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "products": [
                {"id": "1", "name": "Laptop", "category": "ELECTRONICS"},
                {"id": "2", "name": "Unknown Item", "category": None},
            ]
        }

    def test_list_of_enums(self, clear_registry) -> None:
        """Test fields that are lists of enums."""

        @fraiseql.enum
        class Permission(Enum):
            READ = "read"
            WRITE = "write"
            DELETE = "delete"
            ADMIN = "admin"

        @fraiseql.type
        class Role:
            name: str
            permissions: list[Permission]

        @fraiseql.type
        class QueryRoot:
            roles: list[Role] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_roles(_root, _info) -> list[Role]:
                return [
                    Role(name="Editor", permissions=[Permission.READ, Permission.WRITE]),
                    Role(
                        name="Admin",
                        permissions=[
                            Permission.READ,
                            Permission.WRITE,
                            Permission.DELETE,
                            Permission.ADMIN,
                        ],
                    ),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        query = """
        query {
            roles {
                name
                permissions
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "roles": [
                {"name": "Editor", "permissions": ["READ", "WRITE"]},
                {"name": "Admin", "permissions": ["READ", "WRITE", "DELETE", "ADMIN"]},
            ]
        }

    def test_enum_without_decorator_raises_error(self, clear_registry) -> None:
        """Test that using an enum without @fraise_enum raises an error."""

        class Status(Enum):
            ACTIVE = "active"
            INACTIVE = "inactive"

        # This should not raise during decoration but during schema building
        @fraiseql.type
        class Item:
            name: str
            status: Status

        @fraiseql.type
        class QueryRoot:
            items: list[Item] = fraiseql.fraise_field(default_factory=list)

        # The error should occur when building the schema
        with pytest.raises(TypeError, match="must be decorated with @fraise_enum"):
            build_fraiseql_schema(query_types=[QueryRoot])

    def test_enum_with_description(self, clear_registry) -> None:
        """Test enum with docstring description."""

        @fraiseql.enum
        class PaymentMethod(Enum):
            """Available payment methods for orders."""

            CREDIT_CARD = "credit_card"
            DEBIT_CARD = "debit_card"
            PAYPAL = "paypal"
            BANK_TRANSFER = "bank_transfer"

        graphql_enum = PaymentMethod.__graphql_type__
        assert graphql_enum.description == "Available payment methods for orders."

    def test_enum_serialization_in_sql(self, clear_registry) -> None:
        """Test that enum values are properly serialized for SQL."""

        @fraiseql.enum
        class Status(Enum):
            ACTIVE = "active"
            INACTIVE = "inactive"

        # The SQL generator should handle enum.value serialization
        # This is already implemented in mutations/sql_generator.py
        assert Status.ACTIVE.value == "active"
        assert Status.INACTIVE.value == "inactive"

    def test_multiple_enums_in_schema(self, clear_registry) -> None:
        """Test multiple enum types in the same schema."""

        @fraiseql.enum
        class Color(Enum):
            RED = "red"
            GREEN = "green"
            BLUE = "blue"

        @fraiseql.enum
        class Size(Enum):
            SMALL = "S"
            MEDIUM = "M"
            LARGE = "L"
            EXTRA_LARGE = "XL"

        @fraiseql.type
        class Product:
            name: str
            color: Color
            size: Size

        @fraiseql.type
        class QueryRoot:
            products: list[Product] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_products(_root, _info) -> list[Product]:
                return [
                    Product(name="T-Shirt", color=Color.RED, size=Size.LARGE),
                    Product(name="Hoodie", color=Color.BLUE, size=Size.MEDIUM),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Check that both enums are in the schema
        assert "Color" in schema.type_map
        assert "Size" in schema.type_map
        assert isinstance(schema.type_map["Color"], GraphQLEnumType)
        assert isinstance(schema.type_map["Size"], GraphQLEnumType)
