import asyncio
import uuid
from collections.abc import Generator
from typing import Any, ClassVar

import pytest
from graphql import get_introspection_query, graphql

import fraiseql
from fraiseql.core.graphql_type import _graphql_type_cache
from fraiseql.fields import FRAISE_MISSING, FraiseQLField, fraise_field
from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema
from fraiseql.types import JSON


@pytest.fixture
def clear_registry() -> Generator[None]:
    """Clear schema registry and type cache for tests that define GraphQL types.

    This fixture provides full type isolation for tests that define types like
    CreateUserSuccess, CreateUserError, etc. to prevent conflicts with
    identically-named types from other tests.
    """
    # Clear before test
    registry = SchemaRegistry.get_instance()
    registry.clear()
    _graphql_type_cache.clear()

    yield

    # Clear after test
    registry.clear()
    _graphql_type_cache.clear()


def test_fraise_field_with_purpose() -> None:
    field = fraise_field(
        field_type=str,
        purpose="input",  # purpose should be one of 'input', 'output', or 'both'
    )

    assert field.purpose == "input", f"Expected purpose to be 'input', but got {field.purpose}"


def test_fraise_field_with_annotation() -> None:
    # Test that field_type is inferred from annotation if not passed explicitly
    class ExampleClass:
        my_field: str = fraise_field(field_type=str)

    field = ExampleClass.my_field
    assert field.field_type is str, f"Expected field_type to be 'str', but got {field.field_type}"


def test_fraise_field_with_default() -> None:
    field = fraise_field(field_type=int, default=42)

    assert field.default == 42, f"Expected default value to be 42, but got {field.default}"


def test_schema_introspection_v2(clear_registry) -> None:
    """Tests the GraphQL schema introspection to ensure CreateUserResult is present."""

    @fraiseql.type
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInput:
        email: str
        metadata: JSON | None = None

    @fraiseql.success
    class CreateUserSuccess:
        user: GQLUser

    @fraiseql.error
    class CreateUserError:
        message: str
        code: int

    async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
        if input.email.endswith("@example.com"):
            return CreateUserError(message="Blocked domain", code=403)
        return CreateUserSuccess(user=GQLUser(id=str(uuid.uuid4()), email=input.email))

    @fraiseql.type
    class QueryRoot:
        dummy: str = fraise_field(default="dummy")

    schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[create_user])

    introspection_query = get_introspection_query()
    result = asyncio.run(graphql(schema=schema, source=introspection_query))

    assert result.errors is None, f"Introspection errors: {result.errors}"
    assert result.data is not None

    types = result.data["__schema"]["types"]
    type_names = [t["name"] for t in types]

    assert "CreateUserResult" in type_names


@pytest.mark.asyncio
async def test_manual_mutation_execution_v2(clear_registry) -> None:
    """Tests the direct execution of the create_user resolver with a MockInfo object."""

    @fraiseql.type
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInput:
        email: str
        metadata: JSON | None = None

    @fraiseql.success
    class CreateUserSuccess:
        id_: uuid.UUID
        updated_fields: ClassVar[list[str]] = ["email"]
        status: str = "ok"
        message: str = "User created"
        metadata: JSON | None = None
        user: GQLUser
        code: int = 200

    @fraiseql.error
    class CreateUserError:
        message: str
        code: int

    async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
        if input.email.endswith("@example.com"):
            return CreateUserError(message="Blocked domain", code=403)

        return CreateUserSuccess(
            id_=uuid.uuid4(),
            user=GQLUser(id=str(uuid.uuid4()), email=input.email),
            metadata=input.metadata or JSON({}),
        )

    class MockInfo:
        """A mock info object for testing GraphQL resolvers directly."""

        def __init__(self, context_value: dict[str, Any]) -> None:
            self.context = context_value
            self.schema = None  # Not needed for direct resolver call,

    info = MockInfo(context_value={"tenant_id": "demo", "contact_id": "test"})
    user_input = CreateUserInput(email="hello@fraise.dev")
    result = await create_user(info, user_input)

    assert isinstance(result, CreateUserSuccess), (
        f"Expected success but got: {type(result).__name__} with fields: {vars(result)}"
    )
    assert result.status == "ok"
    assert result.message == "User created"
    assert result.user.email == "hello@fraise.dev"
    assert isinstance(result.id_, uuid.UUID)
    assert isinstance(uuid.UUID(result.user.id), uuid.UUID)


def test_user_metadata() -> None:
    """Tests the metadata attributes automatically added to User type by FraiseQL."""

    @fraiseql.type
    class User:
        email: str
        name: str = fraise_field(default="Anonymous")

    # Check FraiseQL metadata
    assert hasattr(User, "__gql_typename__")
    assert User.__gql_typename__ == "User"  # pyright: ignore[reportAttributeAccessIssue]

    assert hasattr(User, "__gql_fields__")
    assert "email" in User.__gql_fields__  # pyright: ignore[reportAttributeAccessIssue]

    email_field = User.__gql_fields__["email"]  # pyright: ignore[reportAttributeAccessIssue]
    assert isinstance(email_field, FraiseQLField)
    assert email_field.purpose == "output"
    assert email_field.default is FRAISE_MISSING

    # Check type hints
    assert hasattr(User, "__gql_type_hints__")
    assert isinstance(User.__gql_type_hints__["email"], type)
    assert User.__gql_type_hints__["email"] is str


def test_schema_structure(clear_registry) -> None:
    """Tests the basic structure of the generated GraphQL schema."""

    @fraiseql.type
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInput:
        email: str
        metadata: JSON | None = None

    @fraiseql.success
    class CreateUserSuccess:
        user: GQLUser
        status: str
        message: str

    @fraiseql.error
    class CreateUserError:
        status: str
        message: str
        code: int

    async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
        return CreateUserSuccess(
            user=GQLUser(id="abc", email=input.email), status="ok", message="User created"
        )

    @fraiseql.type
    class QueryRoot:
        dummy: str = fraise_field(default="dummy")

    schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[create_user])

    # Debug tip: To inspect the schema structure, use:
    # from graphql import print_schema
    # print(print_schema(schema))

    assert schema.query_type.name == "Query"
    assert schema.mutation_type.name == "Mutation"
    assert "createUser" in schema.mutation_type.fields


@pytest.mark.asyncio
async def test_manual_mutation_execution_v3(clear_registry) -> None:
    """Tests the direct execution of the create_user resolver with a MockInfo object."""

    @fraiseql.type
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInput:
        email: str
        metadata: JSON | None = None

    @fraiseql.success
    class CreateUserSuccess:
        id_: uuid.UUID
        status: str
        message: str
        user: GQLUser
        metadata: JSON | None = None
        code: int = 200

    @fraiseql.error
    class CreateUserError:
        status: str
        message: str
        code: int

    async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
        if input.email.endswith("@example.com"):
            return CreateUserSuccess(
                id_=uuid.uuid4(),
                user=GQLUser(id=str(uuid.uuid4()), email=input.email),
                status="ok",
                message="User created",
                metadata=input.metadata or JSON({}),
                code=200,
            )
        return CreateUserError(status="error", message="Invalid domain", code=400)

    class MockInfo:
        def __init__(self, context_value: dict[str, Any]) -> None:
            self.context = context_value
            self.schema = None  # not needed here,

    info = MockInfo(context_value={"tenant_id": "demo", "contact_id": "test"})
    user_input = CreateUserInput(email="hello@example.com")
    result = await create_user(info, user_input)

    assert isinstance(result, CreateUserSuccess)
    assert result.status == "ok"
    assert result.message == "User created"
    assert result.user.email == "hello@example.com"
    assert isinstance(result.id_, uuid.UUID)
    assert isinstance(uuid.UUID(result.user.id), uuid.UUID)


def test_mutation_through_graphql(clear_registry) -> None:
    """Tests the create_user mutation by sending a GraphQL query and asserting the response."""

    @fraiseql.type
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInput:
        email: str
        metadata: JSON | None = None

    @fraiseql.success
    class CreateUserSuccess:
        id_: uuid.UUID
        status: str
        message: str
        user: GQLUser
        metadata: JSON | None = None
        code: int = 200

    @fraiseql.error
    class CreateUserError:
        status: str
        message: str
        code: int

    async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
        return CreateUserSuccess(
            id_=uuid.uuid4(),
            user=GQLUser(id=str(uuid.uuid4()), email=input.email),
            status="ok",
            message="User created",
            metadata=input.metadata or JSON({}),
            code=200,
        )

    @fraiseql.type
    class QueryRoot:
        dummy: str = fraise_field(default="dummy")

    schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[create_user])

    mutation = """
    mutation CreateUser($input: CreateUserInput!) {
        createUser(input: $input) {
            ... on CreateUserSuccess {
                status
                message
                user {
                    id
                    email
                }
            }
            ... on CreateUserError {
                status
                message
                code
            }
        }
    }
    """
    result = asyncio.run(
        graphql(
            schema=schema,
            source=mutation,
            variable_values={"input": {"email": "hello@example.com"}},
            context_value={"tenant_id": "demo", "contact_id": "test"},
        )
    )

    assert result.errors is None
    assert result.data is not None

    payload = result.data["createUser"]
    assert payload["status"] == "ok"
    assert payload["message"] == "User created"
    assert payload["user"]["email"] == "hello@example.com"
    assert isinstance(uuid.UUID(payload["user"]["id"]), uuid.UUID)
