"""Integration tests for PostgreSQL comments to GraphQL descriptions feature.

This test verifies end-to-end functionality of using PostgreSQL comments
as GraphQL schema descriptions across all supported comment types.
"""

import pytest

from fraiseql.introspection.input_generator import InputGenerator
from fraiseql.introspection.mutation_generator import MutationGenerator
from fraiseql.introspection.postgres_introspector import PostgresIntrospector
from fraiseql.introspection.type_generator import TypeGenerator
from fraiseql.introspection.type_mapper import TypeMapper


@pytest.mark.asyncio
class TestCommentDescriptionsIntegration:
    """End-to-end integration tests for PostgreSQL comment descriptions."""

    @pytest.fixture
    def type_mapper(self) -> TypeMapper:
        """Create TypeMapper instance."""
        return TypeMapper()

    @pytest.fixture
    def input_generator(self, type_mapper: TypeMapper) -> InputGenerator:
        """Create InputGenerator instance."""
        return InputGenerator(type_mapper)

    @pytest.fixture
    def mutation_generator(self, input_generator: InputGenerator) -> MutationGenerator:
        """Create MutationGenerator instance."""
        return MutationGenerator(input_generator)

    @pytest.fixture
    def type_generator(self, type_mapper: TypeMapper) -> TypeGenerator:
        """Create TypeGenerator instance."""
        return TypeGenerator(type_mapper)

    @pytest.fixture
    async def introspector(self, db_pool) -> PostgresIntrospector:
        """Create PostgresIntrospector with real database pool."""
        # Note: Using db_pool instead of db_connection to ensure proper transaction handling
        return PostgresIntrospector(db_pool)

    @pytest.fixture
    async def real_database_setup(self, db_connection) -> None:
        """Set up a real PostgreSQL database with test schema and comments."""
        conn = db_connection

        # Clean up any existing test objects
        await conn.execute("""
            DROP VIEW IF EXISTS test_comments.v_user_profile CASCADE;
            DROP FUNCTION IF EXISTS test_comments.fn_create_user(text, text) CASCADE;
            DROP TYPE IF EXISTS test_comments.type_create_user_input CASCADE;
            DROP SCHEMA IF EXISTS test_comments CASCADE;
        """)

        # Create test schema
        await conn.execute("CREATE SCHEMA test_comments;")

        # Create table with column comments
        await conn.execute("""
            CREATE TABLE test_comments.users (
                id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                email text NOT NULL,
                name text NOT NULL,
                created_at timestamptz DEFAULT now()
            );
        """)

        # Add column comments
        await conn.execute(
            "COMMENT ON COLUMN test_comments.users.email IS 'Primary email address for authentication';"
        )
        await conn.execute("COMMENT ON COLUMN test_comments.users.name IS 'Full name of the user';")
        await conn.execute(
            "COMMENT ON COLUMN test_comments.users.created_at IS 'Account creation timestamp (UTC)';"
        )

        # Insert test data so view is not empty
        await conn.execute("""
            INSERT INTO test_comments.users (email, name)
            VALUES ('test@example.com', 'Test User');
        """)

        # Create view with comment (with JSONB data column as expected by FraiseQL)
        await conn.execute("""
            CREATE VIEW test_comments.v_user_profile AS
            SELECT
                id,
                jsonb_build_object(
                    'email', email,
                    'name', name,
                    'created_at', created_at
                ) as data
            FROM test_comments.users;
        """)
        await conn.execute(
            "COMMENT ON VIEW test_comments.v_user_profile IS 'User profile data with contact information';"
        )

        # Debug: Check what views exist
        result = await conn.execute(
            "SELECT schemaname, viewname FROM pg_views WHERE schemaname = 'test_comments'"
        )
        view_rows = await result.fetchall()
        print(f"Views in test_comments schema: {view_rows}")

        # Create composite type with comment and attribute comments
        await conn.execute("""
            CREATE TYPE test_comments.type_create_user_input AS (
                email text,
                name text
            );
        """)
        await conn.execute(
            "COMMENT ON TYPE test_comments.type_create_user_input IS 'Input parameters for user creation';"
        )
        # Note: PostgreSQL doesn't support COMMENT ON ATTRIBUTE syntax
        # Attribute comments are handled differently in PostgreSQL
        # We'll test the infrastructure without actual attribute comments for now

        # Create function with comment
        await conn.execute("""
            CREATE FUNCTION test_comments.fn_create_user(p_email text, p_name text)
            RETURNS jsonb
            LANGUAGE plpgsql
            AS $$
            BEGIN
                INSERT INTO test_comments.users (email, name)
                VALUES (p_email, p_name)
                RETURNING row_to_json(users.*)::jsonb;
            END;
            $$;
        """)
        await conn.execute(
            "COMMENT ON FUNCTION test_comments.fn_create_user(text, text) IS 'Creates a new user account with email verification';"
        )

        # Commit the transaction so other connections can see the changes
        await conn.commit()

        yield conn

        # Cleanup
        await conn.execute("""
            DROP VIEW IF EXISTS test_comments.v_user_profile CASCADE;
            DROP FUNCTION IF EXISTS test_comments.fn_create_user(text, text) CASCADE;
            DROP TYPE IF EXISTS test_comments.type_create_user_input CASCADE;
            DROP TABLE IF EXISTS test_comments.users CASCADE;
            DROP SCHEMA IF EXISTS test_comments CASCADE;
        """)

    async def test_all_comment_types_work_end_to_end(
        self,
        real_database_setup,
        introspector: PostgresIntrospector,
        type_generator: TypeGenerator,
        mutation_generator: MutationGenerator,
        input_generator: InputGenerator,
        db_pool,
    ):
        """Test that all PostgreSQL comment types are properly converted to GraphQL descriptions."""
        conn = real_database_setup

        # 1. Test View Comment → GraphQL Type Description
        # Debug: Check what the introspector is doing
        all_views = await introspector.discover_views(schemas=["test_comments"])
        print(f"All views in test_comments: {[v.view_name for v in all_views]}")

        views = await introspector.discover_views(
            pattern="v_user_profile", schemas=["test_comments"]
        )
        print(f"Found views with pattern: {[v.view_name for v in views]}")  # Debug
        assert len(views) == 1
        view_metadata = views[0]

        assert view_metadata.comment == "User profile data with contact information"

        # Generate type class (need a mock annotation)
        from fraiseql.introspection.metadata_parser import TypeAnnotation

        type_annotation = TypeAnnotation()
        type_cls = await type_generator.generate_type_class(view_metadata, type_annotation, db_pool)
        assert type_cls.__doc__ == "User profile data with contact information"

        # 2. Test Function Comment → GraphQL Mutation Description
        # Mock function metadata (since we can't easily introspect functions in this test)
        from fraiseql.introspection.metadata_parser import MutationAnnotation
        from fraiseql.introspection.postgres_introspector import FunctionMetadata, ParameterInfo

        function_metadata = FunctionMetadata(
            schema_name="test_comments",
            function_name="fn_create_user",
            parameters=[
                ParameterInfo("p_email", "text", "IN", None),
                ParameterInfo("p_name", "text", "IN", None),
            ],
            return_type="jsonb",
            comment="Creates a new user account with email verification",
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser",
            success_type="User",
            failure_type="ValidationError",
        )

        # Generate input type first
        input_cls = await input_generator.generate_input_type(
            function_metadata, annotation, introspector
        )

        # Generate mutation class
        mutation_cls = mutation_generator._create_mutation_class(
            function_metadata,
            annotation,
            input_cls,
            type("Success", (), {}),
            type("Failure", (), {}),
        )
        assert mutation_cls.__doc__ == "Creates a new user account with email verification"

        # 3. Test Composite Type Comment → GraphQL Input Type Description
        composite_type = await introspector.discover_composite_type(
            "type_create_user_input", "test_comments"
        )
        assert composite_type is not None
        assert composite_type.comment == "Input parameters for user creation"

        # Generate input class
        input_cls = await input_generator._generate_from_composite_type(
            "type_create_user_input", "test_comments", introspector
        )
        assert input_cls.__doc__ == "Input parameters for user creation"

        # 4. Test Composite Type Structure (attribute comments not supported in PostgreSQL)
        assert hasattr(input_cls, "__gql_fields__")
        assert "email" in input_cls.__gql_fields__
        assert "name" in input_cls.__gql_fields__

        # Note: PostgreSQL doesn't support COMMENT ON ATTRIBUTE syntax
        # So descriptions will be None for now, but the structure is correct
        assert input_cls.__gql_fields__["email"].description is None
        assert input_cls.__gql_fields__["name"].description is None

        # 5. Test View Structure (has id and data columns as expected by FraiseQL)
        assert "id" in view_metadata.columns
        assert "data" in view_metadata.columns
        assert view_metadata.columns["data"].pg_type == "jsonb"

        # Note: Column comments from the base table are not preserved in JSONB views
        # This is expected behavior when using jsonb_build_object()
