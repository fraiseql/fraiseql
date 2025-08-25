import pytest

"""Test automatic snake_case to camelCase field conversion."""

from graphql import graphql_sync

from fraiseql import query
from fraiseql import type as fraise_type
from fraiseql.gql.schema_builder import build_fraiseql_schema


@pytest.mark.unit
def test_automatic_snake_to_camel_conversion(clear_registry) -> None:
    """Test that snake_case fields are automatically converted to camelCase in GraphQL."""

    @fraise_type
    class Repository:
        id: int
        default_branch: str
        total_commits: int
        is_private: bool
        created_at_timestamp: float

    @query
    def get_repository(info) -> Repository:
        return Repository(
            id=1,
            default_branch="main",
            total_commits=100,
            is_private=False,
            created_at_timestamp=1234567890.0,
        )

    schema = build_fraiseql_schema(query_types=[get_repository])

    # Test querying with camelCase fields
    query_str = """
    query {
        getRepository {
            id
            defaultBranch
            totalCommits
            isPrivate
            createdAtTimestamp
        }
    }
    """
    # GraphQL expects a context object
    context = {}
    result = graphql_sync(schema, query_str, context_value=context)
    if result.errors:
        import traceback

        for error in result.errors:
            if hasattr(error, "__traceback__"):
                traceback.print_tb(error.__traceback__)
    assert result.errors is None, f"Query failed: {result.errors}"
    assert result.data == {
        "getRepository": {
            "id": 1,
            "defaultBranch": "main",
            "totalCommits": 100,
            "isPrivate": False,
            "createdAtTimestamp": 1234567890.0,
        }
    }

    # Verify snake_case fields don't work
    snake_query = """
    query {
        get_repository {
            defaultBranch
        }
    }
    """
    result = graphql_sync(schema, snake_query)
    assert result.errors is not None
    assert "Cannot query field" in str(result.errors[0])


def test_camelcase_conversion_with_config(clear_registry) -> None:
    """Test enabling/disabling camelCase conversion via configuration."""

    @fraise_type
    class User:
        user_name: str
        first_name: str
        last_login_time: float

    @query
    def current_user(info) -> User:
        return User(user_name="john_doe", first_name="John", last_login_time=1234567890.0)

    # Test with camelCase enabled (default)
    schema = build_fraiseql_schema(query_types=[current_user], camel_case_fields=True)

    result = graphql_sync(
        schema,
        """
    query {
        currentUser {
            userName
            firstName
            lastLoginTime
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data["currentUser"]["userName"] == "john_doe"

    # Test with camelCase disabled
    schema_snake = build_fraiseql_schema(query_types=[current_user], camel_case_fields=False)

    result = graphql_sync(
        schema_snake,
        """
    query {
        current_user {
            user_name
            first_name
            last_login_time
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data["current_user"]["user_name"] == "john_doe"


def test_explicit_graphql_name(clear_registry) -> None:
    """Test using explicit graphql_name parameter."""
    from fraiseql.fields import fraise_field

    @fraise_type
    class Product:
        internal_id: int = fraise_field(graphql_name="id")
        product_name: str = fraise_field(graphql_name="name")
        price_usd: float = fraise_field(graphql_name="price")
        # This should still be converted to camelCase
        stock_quantity: int

    @query
    def get_product(info) -> Product:
        return Product(internal_id=1, product_name="Widget", price_usd=9.99, stock_quantity=50)

    schema = build_fraiseql_schema(query_types=[Product])

    result = graphql_sync(
        schema,
        """
    query {
        getProduct {
            id
            name
            price
            stockQuantity
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data == {
        "getProduct": {"id": 1, "name": "Widget", "price": 9.99, "stockQuantity": 50}
    }


def test_mixed_case_preservation(clear_registry) -> None:
    """Test that certain naming patterns are preserved correctly."""

    @fraise_type
    class APIConfig:
        api_key: str  # Should become apiKey
        APIVersion: str  # Should stay APIVersion
        httpTimeout: int  # Should stay httpTimeout
        URL: str  # Should stay URL

    @query
    def config(info) -> APIConfig:
        return APIConfig(
            api_key="secret123", APIVersion="v2", httpTimeout=30, URL="https://api.example.com"
        )

    schema = build_fraiseql_schema(query_types=[APIConfig])

    result = graphql_sync(
        schema,
        """
    query {
        config {
            apiKey
            APIVersion
            httpTimeout
            URL
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data == {
        "config": {
            "apiKey": "secret123",
            "APIVersion": "v2",
            "httpTimeout": 30,
            "URL": "https://api.example.com",
        }
    }


def test_input_type_camelcase(clear_registry) -> None:
    """Test that input types also use camelCase."""
    from fraiseql import fraise_input, mutation

    @fraise_input
    class CreateUserInput:
        user_name: str
        email_address: str
        is_admin: bool = False

    @fraise_type
    class User:
        id: int
        user_name: str
        email_address: str
        is_admin: bool

    @query
    def test_query(info) -> str:
        return "test"

    @mutation
    def create_user(info, input: CreateUserInput) -> User:
        return User(
            id=1,
            user_name=input.user_name,
            email_address=input.email_address,
            is_admin=input.is_admin,
        )

    schema = build_fraiseql_schema(query_types=[User, CreateUserInput])

    result = graphql_sync(
        schema,
        """
    mutation {
        createUser(input: {
            userName: "john_doe"
            emailAddress: "john@example.com"
            isAdmin: true
        }) {
            id
            userName
            emailAddress
            isAdmin
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data == {
        "createUser": {
            "id": 1,
            "userName": "john_doe",
            "emailAddress": "john@example.com",
            "isAdmin": True,
        }
    }


def test_enum_value_preservation(clear_registry) -> None:
    """Test that enum values are not converted."""
    from enum import Enum

    from fraiseql import fraise_enum

    @fraise_enum
    class UserStatus(Enum):
        ACTIVE_USER = "ACTIVE_USER"
        inactive_user = "inactive_user"
        PendingApproval = "PendingApproval"

    @fraise_type
    class User:
        user_name: str
        user_status: UserStatus

    @query
    def get_user(info) -> User:
        return User(user_name="john", user_status=UserStatus.ACTIVE_USER)

    schema = build_fraiseql_schema(query_types=[User, UserStatus])

    # Enum values should not be converted
    result = graphql_sync(
        schema,
        """
    query {
        getUser {
            userName
            userStatus
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data == {"getUser": {"userName": "john", "userStatus": "ACTIVE_USER"}}


def test_nested_types_camelcase(clear_registry) -> None:
    """Test camelCase conversion works with nested types."""

    @fraise_type
    class Address:
        street_line_1: str
        street_line_2: str | None
        postal_code: str

    @fraise_type
    class Company:
        company_name: str
        employee_count: int
        head_office: Address

    @query
    def get_company(info) -> Company:
        return Company(
            company_name="Acme Corp",
            employee_count=100,
            head_office=Address(
                street_line_1="123 Main St", street_line_2="Suite 400", postal_code="12345"
            ),
        )

    schema = build_fraiseql_schema(query_types=[Company, Address])

    result = graphql_sync(
        schema,
        """
    query {
        getCompany {
            companyName
            employeeCount
            headOffice {
                streetLine1
                streetLine2
                postalCode
            }
        }
    }
    """,
        context_value={},
    )

    assert result.errors is None
    assert result.data == {
        "getCompany": {
            "companyName": "Acme Corp",
            "employeeCount": 100,
            "headOffice": {
                "streetLine1": "123 Main St",
                "streetLine2": "Suite 400",
                "postalCode": "12345",
            },
        }
    }
