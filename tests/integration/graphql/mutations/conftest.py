"""
Fixtures for GraphQL mutation integration tests.

Provides database seeding and schema refresh for tests that require
dynamically created mutation functions.
"""

import psycopg
import pytest


@pytest.fixture
async def blog_simple_app_with_native_errors(blog_simple_app, blog_simple_db_url):
    """Blog app with native error array test mutations.

    Creates database functions for testing WP-034 native error arrays feature,
    then refreshes the schema to discover them.

    This fixture demonstrates using app.refresh_schema() to test features
    that require dynamically created database functions.
    """
    # Create test mutation functions
    async with await psycopg.AsyncConnection.connect(blog_simple_db_url) as conn:
        # Test function for test_auto_generated_errors_from_status
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_auto_error()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN mutation_validation_error(
                    'Validation failed',
                    'User',
                    NULL
                );
            END;
            $$;
        """)

        # Test functions for test_auto_generated_errors_multiple_status_formats
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_status_validation()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN (
                    'failed:validation',
                    'Test message',
                    NULL,
                    'TestType',
                    NULL,
                    NULL,
                    NULL,
                    NULL
                )::mutation_response;
            END;
            $$;

            CREATE OR REPLACE FUNCTION test_status_notfound()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN (
                    'noop:not_found',
                    'Test message',
                    NULL,
                    'TestType',
                    NULL,
                    NULL,
                    NULL,
                    NULL
                )::mutation_response;
            END;
            $$;

            CREATE OR REPLACE FUNCTION test_status_authorization()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN (
                    'failed:authorization',
                    'Test message',
                    NULL,
                    'TestType',
                    NULL,
                    NULL,
                    NULL,
                    NULL
                )::mutation_response;
            END;
            $$;

            CREATE OR REPLACE FUNCTION test_status_generalerror()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN (
                    'failed',
                    'Test message',
                    NULL,
                    'TestType',
                    NULL,
                    NULL,
                    NULL,
                    NULL
                )::mutation_response;
            END;
            $$;
        """)

        # Test function for test_explicit_errors_override_auto_generation
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_explicit_errors()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN (
                    'failed:validation',
                    'Multiple validation errors',
                    NULL,
                    'User',
                    NULL,
                    NULL,
                    NULL,
                    jsonb_build_object(
                        'errors', jsonb_build_array(
                            jsonb_build_object(
                                'code', 400,
                                'identifier', 'email_invalid',
                                'message', 'Email format is invalid',
                                'details', jsonb_build_object('field', 'email')
                            ),
                            jsonb_build_object(
                                'code', 400,
                                'identifier', 'password_weak',
                                'message', 'Password must be at least 8 characters',
                                'details', jsonb_build_object('field', 'password')
                            )
                        )
                    )
                )::mutation_response;
            END;
            $$;
        """)

        # Test function for test_backward_compatibility_with_mutation_result_base
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_with_base()
            RETURNS mutation_response
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN mutation_validation_error(
                    'Validation failed',
                    'User',
                    NULL
                );
            END;
            $$;
        """)

        await conn.commit()

    # Define test mutations manually that call the database functions
    # These mutations properly wrap the test_* database functions
    from fraiseql.db import DatabaseQuery
    from fraiseql.mutations import mutation

    @mutation
    async def test_auto_error(info) -> dict:
        """Test mutation for auto-generated errors."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_auto_error()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_status_validation(info) -> dict:
        """Test mutation for status validation."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_status_validation()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_status_notfound(info) -> dict:
        """Test mutation for status not found."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_status_notfound()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_status_authorization(info) -> dict:
        """Test mutation for status authorization."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_status_authorization()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_status_generalerror(info) -> dict:
        """Test mutation for status general error."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_status_generalerror()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_explicit_errors(info) -> dict:
        """Test mutation for explicit errors."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_explicit_errors()", [])
        result = await db.run(query)
        return result[0] if result else {}

    @mutation
    async def test_with_base(info) -> dict:
        """Test mutation for backward compatibility."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_with_base()", [])
        result = await db.run(query)
        return result[0] if result else {}

    # Add these mutations to the refresh config so they get included in the schema
    if hasattr(blog_simple_app.state, "_fraiseql_refresh_config"):
        blog_simple_app.state._fraiseql_refresh_config["original_mutations"].extend(
            [
                test_auto_error,
                test_status_validation,
                test_status_notfound,
                test_status_authorization,
                test_status_generalerror,
                test_explicit_errors,
                test_with_base,
            ]
        )

    # Refresh schema to include the new mutations
    await blog_simple_app.refresh_schema()

    yield blog_simple_app

    # Cleanup happens automatically via database fixture
