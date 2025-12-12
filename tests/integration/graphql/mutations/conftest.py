"""
Fixtures for GraphQL mutation integration tests.

NOTE: This file contains an ATTEMPTED solution for WP-034 integration tests.
The approach FAILED due to fixture ordering constraints - see below.

BLOCKER: GraphQL schema is built during blog_simple_app initialization,
BEFORE any test-level fixtures can run. This makes it impossible to
pre-create database functions for schema discovery using fixtures.

See /tmp/fraiseql-phase1.5-blocker-analysis.md for full analysis.

SOLUTION: Tests in test_native_error_arrays.py are skipped with @pytest.mark.skip
until framework supports dynamic schema refresh (tracked for Phase 5).

This file is kept as:
1. Documentation of what was attempted
2. Template for future schema refresh solution
3. Reference for similar integration test challenges
"""

import pytest_asyncio


# ATTEMPTED FIXTURE - Does not work due to timing (see module docstring)
@pytest_asyncio.fixture(scope="function", autouse=False)
async def native_error_arrays_mutations(blog_simple_db_url):
    """
    Pre-create mutation functions for native error arrays tests (WP-034).

    These functions test the automatic error array generation feature
    implemented in v1.8.0-beta.4 (2025-12-09).

    Scope: function - Functions are created fresh for each test.
    This fixture creates the database functions BEFORE the GraphQL schema
    is built by blog_simple_client.

    Timing: This fixture must run BEFORE blog_simple_client starts,
    so the GraphQL schema can discover these mutations during initialization.

    Usage: Add this fixture as a parameter to blog_simple_graphql_client calls,
    or use pytestmark = pytest.mark.usefixtures("native_error_arrays_mutations")
    """
    import psycopg

    # Create our own connection to pre-create functions
    db = await psycopg.AsyncConnection.connect(blog_simple_db_url)

    # Create all test mutation functions in a single transaction
    await db.execute("""
        -- ================================================================
        -- WP-034: Native Error Arrays Feature Tests
        -- ================================================================

        -- Test 1: test_auto_generated_errors_from_status
        -- Tests that error arrays are auto-populated from status strings
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

        -- Test 2a: test_auto_generated_errors_multiple_status_formats
        -- Tests status format: failed:validation -> identifier="validation"
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

        -- Test 2b: test_auto_generated_errors_multiple_status_formats
        -- Tests status format: noop:not_found -> identifier="not_found"
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

        -- Test 2c: test_auto_generated_errors_multiple_status_formats
        -- Tests status format: failed:authorization -> identifier="authorization"
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

        -- Test 2d: test_auto_generated_errors_multiple_status_formats
        -- Tests status format: failed -> identifier="general_error"
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

        -- Test 3: test_explicit_errors_override_auto_generation
        -- Tests that explicit errors in metadata.errors override auto-generation
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

        -- Test 4: test_backward_compatibility_with_mutation_result_base
        -- Tests backward compatibility with MutationResultBase pattern
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

    await db.commit()  # Commit the function creations
    await db.close()  # Close our connection

    yield  # Tests run here with functions already in schema

    # Cleanup not needed - database is dropped after test class completes
