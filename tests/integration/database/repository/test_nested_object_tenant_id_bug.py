"""Test case to reproduce the nested object tenant_id bug."""

from typing import Optional
from uuid import UUID

import pytest
from graphql import GraphQLResolveInfo

from fraiseql import query, type


@type(sql_source="mv_organization")
class Organization:
    """Organization type with sql_source."""

    id: UUID
    name: str
    identifier: str
    status: str


@type(sql_source="v_user")
class User:
    """User type with embedded organization in JSONB data."""

    id: UUID
    first_name: str
    last_name: str
    email_address: str
    organization: Optional[Organization] = None  # This is embedded in data column


@query
async def user(info: GraphQLResolveInfo) -> Optional[User]:
    """Query to get the current user."""
    db = info.context["db"]
    # In a real app, this would get user_id from context
    # For testing, we'll use a known test user ID
    test_user_id = UUID("75736572-0000-0000-0000-000000000000")

    result = await db.find_one("v_user", id=test_user_id)

    return User(**result) if result else None


@pytest.mark.asyncio
async def test_user_with_embedded_organization_tenant_id_bug(db_connection):
    """Test that querying user with organization incorrectly requires tenant_id."""
    # Check if required tables exist
    try:
        await db_connection.execute("SELECT 1 FROM tenant.tb_organization LIMIT 1")
    except Exception:
        pytest.skip("Required tenant schema tables not available in test environment")

    # First, set up test data
    async with db_connection.transaction():
        # Create test organization
        await db_connection.execute("""
            INSERT INTO tenant.tb_organization (pk_organization, name, identifier, data)
            VALUES (
                '6f726700-0000-0000-0000-000000000000'::uuid,
                'Test Org',
                'TEST-ORG',
                jsonb_build_object(
                    'id', '6f726700-0000-0000-0000-000000000000',
                    'name', 'Test Org',
                    'identifier', 'TEST-ORG',
                    'status', 'active'
                )
            )
            ON CONFLICT (pk_organization) DO NOTHING
        """)

        # Create materialized view entry for organization
        await db_connection.execute("""
            INSERT INTO public.mv_organization (id, tenant_id, data)
            VALUES (
                '6f726700-0000-0000-0000-000000000000'::uuid,
                '6f726700-0000-0000-0000-000000000000'::uuid,  -- org is its own tenant
                jsonb_build_object(
                    'id', '6f726700-0000-0000-0000-000000000000',
                    'name', 'Test Org',
                    'identifier', 'TEST-ORG',
                    'status', 'active'
                )
            )
            ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data
        """)

        # Create test user with embedded organization
        await db_connection.execute("""
            INSERT INTO tenant.tb_contact (
                pk_contact,
                fk_customer_org,
                first_name,
                last_name,
                email_address,
                data
            )
            VALUES (
                '75736572-0000-0000-0000-000000000000'::uuid,
                '6f726700-0000-0000-0000-000000000000'::uuid,
                'Alice',
                'Cooper',
                'alice@example.com',
                jsonb_build_object(
                    'id', '75736572-0000-0000-0000-000000000000',
                    'first_name', 'Alice',
                    'last_name', 'Cooper',
                    'email_address', 'alice@example.com'
                )
            )
            ON CONFLICT (pk_contact) DO UPDATE SET
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                data = EXCLUDED.data
        """)

        # Create/update the view that includes embedded organization
        await db_connection.execute("""
            CREATE OR REPLACE VIEW public.v_user AS
            SELECT
              tb_contact.pk_contact AS id,
              tb_contact.fk_customer_org AS tenant_id,
              jsonb_build_object(
                'id', tb_contact.pk_contact,
                'first_name', tb_contact.first_name,
                'last_name', tb_contact.last_name,
                'email_address', tb_contact.email_address,
                'organization', mv_organization.data  -- Organization is EMBEDDED here
              ) AS data
            FROM tenant.tb_contact
            JOIN public.mv_organization ON tb_contact.fk_customer_org = mv_organization.id
        """)

    # Now test the GraphQL query
    from graphql import graphql

    from fraiseql import create_schema

    schema = create_schema(queries=[user], types=[User, Organization])

    query = """
    query GetUser {
      user {
        id
        firstName
        lastName
        emailAddress
        organization {
          id
          name
          identifier
          status
        }
      }
    }
    """

    context = {
        "db": db_connection,
        # Note: NOT providing tenant_id in context
    }

    result = await graphql(schema, query, context_value=context)

    # The bug: This will fail with "missing a required argument: 'tenant_id'"
    # because FraiseQL tries to query mv_organization separately
    # instead of using the embedded data from v_user

    if result.errors:
        # Check if the error is about missing tenant_id
        error_messages = [str(e) for e in result.errors]
        assert any("tenant_id" in msg for msg in error_messages), (
            "Expected tenant_id error, but got different errors"
        )

    else:
        # If no errors, the bug might be fixed
        assert result.data["user"] is not None
        assert result.data["user"]["organization"] is not None


@pytest.mark.asyncio
async def test_workaround_with_duplicate_type(db_connection):
    """Test workaround using a duplicate type without sql_source."""
    # Check if required tables exist
    try:
        await db_connection.execute("SELECT 1 FROM tenant.tb_organization LIMIT 1")
    except Exception:
        pytest.skip("Required tenant schema tables not available in test environment")

    # Define Organization without sql_source for embedded use
    @type  # No sql_source
    class EmbeddedOrganization:
        id: UUID
        name: str
        identifier: str
        status: str

    @type(sql_source="v_user")
    class UserWithEmbedded:
        id: UUID
        first_name: str
        last_name: str
        email_address: str
        organization: Optional[EmbeddedOrganization] = None

    @query
    async def userWithEmbedded(info: GraphQLResolveInfo) -> Optional[UserWithEmbedded]:
        db = info.context["db"]
        test_user_id = UUID("75736572-0000-0000-0000-000000000000")

        result = await db.find_one("v_user", id=test_user_id)

        return UserWithEmbedded(**result) if result else None

    from graphql import graphql

    from fraiseql import create_schema

    schema = create_schema(
        queries=[userWithEmbedded], types=[UserWithEmbedded, EmbeddedOrganization]
    )

    query = """
    query GetUser {
      userWithEmbedded {
        id
        firstName
        lastName
        organization {
          id
          name
          identifier
        }
      }
    }
    """

    context = {
        "db": db_connection,
    }

    result = await graphql(schema, query, context_value=context)

    # This workaround should work without tenant_id errors
    assert result.errors is None or len(result.errors) == 0
    assert result.data["userWithEmbedded"] is not None
    assert result.data["userWithEmbedded"]["organization"] is not None
