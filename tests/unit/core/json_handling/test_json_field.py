import logging
import uuid
from typing import Annotated, Any

import pytest
from graphql import graphql

import fraiseql
from fraiseql.fields import fraise_field
from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema
from fraiseql.types import JSON, Error

# Configure logger for this test module
logger = logging.getLogger(__name__)


@pytest.mark.asyncio
async def test_json_field_in_mutation() -> None:
    # Clear and reset the schema registry (if needed)
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # --- Define QueryRoot inside the test, so it registers post-clear ---

    # --- Define QueryRoot inside the test, so it registers post-clear ---
    @fraiseql.type
    class QueryRoot:
        ping: Annotated[str, fraise_field(description="Ping-pong healthcheck", purpose="output")]

        @staticmethod
        def resolve_ping(root: Any, info: Any) -> str:
            return "pong"

    # Debug 1: Check the __fraiseql_definition__ metadata for QueryRoot
    logger.debug(
        "QueryRoot __fraiseql_definition__: %s", getattr(QueryRoot, "__fraiseql_definition__", None)
    )
    logger.debug("QueryRoot defined and decorated.")
    logger.debug(
        "Registry types after defining QueryRoot: %s", [t.__name__ for t in registry._types]
    )

    @fraiseql.type
    class BaseResult:
        id_: Annotated[uuid.UUID | None, fraise_field(default=None)]
        updated_fields: Annotated[list[str] | None, fraise_field(default=None)]
        status: Annotated[str, fraise_field()]
        message: Annotated[str | None, fraise_field(default=None)]
        metadata: Annotated[JSON | None, fraise_field(default=None)]
        errors: Annotated[list[Error] | None, fraise_field(default=None)]
        code: Annotated[int | None, fraise_field(default=None)]

    @fraiseql.type(sql_source="tb_user")
    class GQLUser:
        id: str
        email: str

    @fraiseql.input
    class CreateUserInputTestGQLConversion:
        email: str
        metadata: Annotated[JSON | None, fraise_field(default=None)]

    @fraiseql.success
    class CreateUserSuccess(BaseResult):
        user: Annotated[GQLUser, fraise_field()]

    @fraiseql.error
    class CreateUserError(BaseResult):
        duplicate_user: GQLUser | None = None

    async def createUser(
        info: Any, input: CreateUserInputTestGQLConversion
    ) -> CreateUserSuccess | CreateUserError:
        return CreateUserSuccess(status="ok", user=GQLUser(id=str(uuid.uuid4()), email=input.email))

    # Proceed with your schema building
    schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[createUser])

    logger.debug("Schema successfully built.")

    mutation = """
    mutation CreateUser($input: CreateUserInputTestGQLConversion!) {
        createUser(input: $input) {
            ... on CreateUserSuccess {
                status
                message
                metadata
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
    # --- Test: JSON field coercion ---
    variables = {"input": {"email": "test@example.com", "metadata": {"key": "value"}}}
    result = await graphql(
        schema=schema,
        source=mutation,
        variable_values=variables,
        context_value={"tenant_id": "demo", "contact_id": "test"},
    )

    if result.errors:
        logger.debug("GraphQL mutation errors: %s", result.errors)
    else:
        logger.debug("Mutation result: %s", result.data)

    assert not result.errors, f"GraphQL errors: {result.errors}"
    assert result.data is not None
    data = result.data["createUser"]  # Changed to camelCase
    # The test passes if we can execute the mutation without errors
    assert data is not None

    # --- Test: QueryRoot.ping is visible and works ---
    ping_query = "{ ping }"
    ping_result = await graphql(
        schema=schema, source=ping_query, context_value={"tenant_id": "demo", "contact_id": "test"}
    )

    if ping_result.errors:
        logger.debug("Ping query errors: %s", ping_result.errors)
    else:
        logger.debug("Ping query result: %s", ping_result.data)

    assert not ping_result.errors
    assert ping_result.data == {"ping": "pong"}

    # --- Test: Introspection includes ping ---
    introspection_query = """
    {
      __schema {
        queryType {
          fields {
            name
          }
        }
      }
    }
    """
    introspect = await graphql(schema=schema, source=introspection_query)

    if introspect.errors:
        logger.debug("Introspection query errors: %s", introspect.errors)
    else:
        logger.debug("Introspection result: %s", introspect.data)

    assert not introspect.errors
    field_names = [f["name"] for f in introspect.data["__schema"]["queryType"]["fields"]]
    logger.debug("QueryRoot fields seen in introspection: %s", field_names)
    assert "ping" in field_names
