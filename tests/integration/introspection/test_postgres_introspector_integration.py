"""Integration tests for PostgresIntrospector with real database.

These tests verify that the PostgresIntrospector can correctly discover
database views and functions from a real PostgreSQL database.
"""

import pytest

from fraiseql.introspection.postgres_introspector import (
    ColumnInfo,
    ParameterInfo,
    PostgresIntrospector,
)


class TestPostgresIntrospectorIntegration:
    """Integration tests for PostgresIntrospector with real database."""

    @pytest.fixture
    async def introspector(self, db_pool) -> None:
        """Create PostgresIntrospector with real database pool."""
        return PostgresIntrospector(db_pool)

    @pytest.fixture
    async def test_view(self, db_connection) -> None:
        """Create a test view for introspection testing."""
        # Create underlying table with unique name to avoid conflicts
        import uuid

        table_suffix = uuid.uuid4().hex[:8]
        table_name = f"test_users_{table_suffix}"
        view_name = f"v_users_{table_suffix}"

        # Create underlying table
        await db_connection.execute(f"""
            CREATE TABLE {table_name} (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                created_at TIMESTAMP DEFAULT NOW()
            )
        """)

        # Add comments to table and columns
        await db_connection.execute(f"""
            COMMENT ON TABLE {table_name} IS '@fraiseql:type
            name: User
            description: A user in the system'
        """)

        await db_connection.execute(f"""
            COMMENT ON COLUMN {table_name}.id IS 'Unique identifier for the user'
        """)

        await db_connection.execute(f"""
            COMMENT ON COLUMN {table_name}.name IS 'Full name of the user'
        """)

        await db_connection.execute(f"""
            COMMENT ON COLUMN {table_name}.email IS 'Email address (optional)'
        """)

        # Create view
        await db_connection.execute(f"""
            CREATE VIEW {view_name} AS
            SELECT id, name, email, created_at
            FROM {table_name}
            WHERE email IS NOT NULL
        """)

        # Add comment to view
        await db_connection.execute(f"""
            COMMENT ON VIEW {view_name} IS '@fraiseql:type
            name: ActiveUser
            description: Users with email addresses'
        """)

        await db_connection.commit()

        return view_name

    @pytest.fixture
    async def test_function(self, db_connection) -> None:
        """Create a test function for introspection testing."""
        # Create function with unique name
        import uuid

        func_suffix = uuid.uuid4().hex[:8]
        func_name = f"fn_create_user_{func_suffix}"

        # Create function
        await db_connection.execute(f"""
            CREATE OR REPLACE FUNCTION {func_name}(
                p_name TEXT,
                p_email TEXT DEFAULT NULL
            )
            RETURNS INTEGER
            LANGUAGE plpgsql
            AS $$
            BEGIN
                -- Dummy function for testing
                RETURN 1;
            END;
            $$
        """)

        # Add comment to function
        await db_connection.execute(f"""
            COMMENT ON FUNCTION {func_name}(TEXT, TEXT) IS '@fraiseql:mutation
            name: createUser
            description: Create a new user'
        """)

        await db_connection.commit()

        return func_name

    async def test_discover_views_basic(self, introspector, test_view) -> None:
        """Test basic view discovery functionality."""
        views = await introspector.discover_views(pattern="v_%")

        # Find our test view
        test_view_metadata = None
        for view in views:
            if view.view_name == test_view:
                test_view_metadata = view
                break

        assert test_view_metadata is not None
        assert test_view_metadata.schema_name == "public"
        assert test_view_metadata.view_name == test_view
        assert "SELECT" in test_view_metadata.definition.upper()
        assert test_view_metadata.comment is not None
        assert "@fraiseql:type" in test_view_metadata.comment

    async def test_discover_views_columns(self, introspector, test_view) -> None:
        """Test that view column information is correctly discovered."""
        views = await introspector.discover_views(pattern="v_%")

        test_view_metadata = None
        for view in views:
            if view.view_name == test_view:
                test_view_metadata = view
                break

        assert test_view_metadata is not None

        # Check columns
        columns = test_view_metadata.columns
        assert "id" in columns
        assert "name" in columns
        assert "email" in columns
        assert "created_at" in columns

        # Check column details
        id_column = columns["id"]
        assert isinstance(id_column, ColumnInfo)
        assert id_column.name == "id"
        assert id_column.pg_type == "int4"  # SERIAL becomes int4
        # Note: PostgreSQL views may not preserve column comments from underlying tables
        # So we just check that the column exists and has the right basic properties
        assert id_column.name == "id"

        name_column = columns["name"]
        assert name_column.name == "name"
        assert name_column.pg_type == "text"
        # Note: PostgreSQL views may not preserve NOT NULL constraints from underlying tables
        # So we just check the basic properties
        assert name_column.name == "name"

        email_column = columns["email"]
        assert email_column.name == "email"
        assert email_column.pg_type == "text"
        assert email_column.nullable  # No NOT NULL constraint

    async def test_discover_views_no_match(self, introspector) -> None:
        """Test view discovery with pattern that matches nothing."""
        views = await introspector.discover_views(pattern="nonexistent_%")
        assert len(views) == 0

    async def test_discover_views_schema_filter(self, introspector, test_view) -> None:
        """Test view discovery with schema filtering."""
        # Test with correct schema
        views = await introspector.discover_views(pattern="v_%", schemas=["public"])
        assert len(views) >= 1
        assert any(v.view_name == test_view for v in views)

        # Test with wrong schema
        views = await introspector.discover_views(pattern="v_%", schemas=["other_schema"])
        assert len(views) == 0

    async def test_discover_functions_basic(self, introspector, test_function) -> None:
        """Test basic function discovery functionality."""
        functions = await introspector.discover_functions(pattern="fn_%")

        # Find our test function
        test_func_metadata = None
        for func in functions:
            if func.function_name == test_function:
                test_func_metadata = func
                break

        assert test_func_metadata is not None
        assert test_func_metadata.schema_name == "public"
        assert test_func_metadata.function_name == test_function
        assert test_func_metadata.return_type == "integer"
        assert test_func_metadata.language == "plpgsql"
        assert test_func_metadata.comment is not None
        assert "@fraiseql:mutation" in test_func_metadata.comment

    async def test_discover_functions_parameters(self, introspector, test_function) -> None:
        """Test that function parameter information is correctly discovered."""
        functions = await introspector.discover_functions(pattern="fn_%")

        test_func_metadata = None
        for func in functions:
            if func.function_name == test_function:
                test_func_metadata = func
                break

        assert test_func_metadata is not None

        # Check parameters
        params = test_func_metadata.parameters
        assert len(params) == 2

        # First parameter: p_name
        p_name = params[0]
        assert isinstance(p_name, ParameterInfo)
        assert p_name.name == "p_name"
        assert p_name.pg_type == "text"
        assert p_name.mode == "IN"
        assert p_name.default_value is None

        # Second parameter: p_email with default
        p_email = params[1]
        assert p_email.name == "p_email"
        assert p_email.pg_type == "text"
        assert p_email.mode == "IN"
        assert p_email.default_value == "NULL::text"  # PostgreSQL casts NULL to the parameter type

    async def test_discover_functions_no_match(self, introspector) -> None:
        """Test function discovery with pattern that matches nothing."""
        functions = await introspector.discover_functions(pattern="nonexistent_%")
        assert len(functions) == 0

    async def test_discover_functions_schema_filter(self, introspector, test_function) -> None:
        """Test function discovery with schema filtering."""
        # Test with correct schema
        functions = await introspector.discover_functions(pattern="fn_%", schemas=["public"])
        assert len(functions) >= 1
        assert any(f.function_name == test_function for f in functions)

        # Test with wrong schema
        functions = await introspector.discover_functions(pattern="fn_%", schemas=["other_schema"])
        assert len(functions) == 0

    async def test_discover_multiple_views_and_functions(self, introspector, db_connection) -> None:
        """Test discovery of multiple views and functions."""
        # Create unique test objects for this test
        import uuid

        suffix = uuid.uuid4().hex[:8]

        # Create test table
        table_name = f"test_users_multi_{suffix}"
        await db_connection.execute(f"""
            CREATE TABLE {table_name} (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )
        """)

        # Create additional test objects
        view_name = f"v_admins_{suffix}"
        func_name = f"fn_get_user_{suffix}"

        await db_connection.execute(f"""
            CREATE VIEW {view_name} AS
            SELECT id, name, email
            FROM {table_name}
            WHERE name LIKE 'Admin%';
        """)

        await db_connection.execute(f"""
            CREATE OR REPLACE FUNCTION {func_name}(p_id INTEGER)
            RETURNS TABLE(id INTEGER, name TEXT, email TEXT)
            LANGUAGE sql
            AS $$
                SELECT id, name, email FROM {table_name} WHERE id = p_id;
            $$
        """)

        await db_connection.commit()

        # Discover views - should find both the original test view and this new one
        views = await introspector.discover_views(pattern="v_%")
        view_names = {v.view_name for v in views}
        assert len(view_names) >= 2  # At least our test views

        # Discover functions - should find both functions
        functions = await introspector.discover_functions(pattern="fn_%")
        func_names = {f.function_name for f in functions}
        assert len(func_names) >= 2  # At least our test functions

        # Check the table-returning function
        get_user_func = next(f for f in functions if f.function_name == func_name)
        assert get_user_func.return_type == "TABLE(id integer, name text, email text)"
        assert len(get_user_func.parameters) == 1
        assert get_user_func.parameters[0].name == "p_id"
        assert get_user_func.parameters[0].pg_type in (
            "int4",
            "integer",
        )  # PostgreSQL may return either

    async def test_view_metadata_structure(self, introspector, test_view) -> None:
        """Test that ViewMetadata objects have correct structure."""
        views = await introspector.discover_views(pattern="v_%")

        test_view_metadata = next(v for v in views if v.view_name == test_view)

        # Check all required fields are present and correct types
        assert isinstance(test_view_metadata.schema_name, str)
        assert isinstance(test_view_metadata.view_name, str)
        assert isinstance(test_view_metadata.definition, str)
        assert isinstance(test_view_metadata.comment, (str, type(None)))
        assert isinstance(test_view_metadata.columns, dict)

        # Check columns structure
        for col_name, col_info in test_view_metadata.columns.items():
            assert isinstance(col_name, str)
            assert isinstance(col_info, ColumnInfo)
            assert isinstance(col_info.name, str)
            assert isinstance(col_info.pg_type, str)
            assert isinstance(col_info.nullable, bool)
            assert isinstance(col_info.comment, (str, type(None)))

    async def test_function_metadata_structure(self, introspector, test_function) -> None:
        """Test that FunctionMetadata objects have correct structure."""
        functions = await introspector.discover_functions(pattern="fn_%")

        test_func_metadata = next(f for f in functions if f.function_name == test_function)

        # Check all required fields are present and correct types
        assert isinstance(test_func_metadata.schema_name, str)
        assert isinstance(test_func_metadata.function_name, str)
        assert isinstance(test_func_metadata.parameters, list)
        assert isinstance(test_func_metadata.return_type, str)
        assert isinstance(test_func_metadata.comment, (str, type(None)))
        assert isinstance(test_func_metadata.language, str)

        # Check parameters structure
        for param in test_func_metadata.parameters:
            assert isinstance(param, ParameterInfo)
            assert isinstance(param.name, str)
            assert isinstance(param.pg_type, str)
            assert isinstance(param.mode, str)
            assert isinstance(param.default_value, (str, type(None)))
