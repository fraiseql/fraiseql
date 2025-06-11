"""Database setup and migration management for TestFoundry."""

from pathlib import Path

from fraiseql.db import FraiseQLRepository


class FoundrySetup:
    """Manages TestFoundry database setup and migrations."""

    def __init__(
        self, repository: FraiseQLRepository, schema_name: str = "testfoundry"
    ):
        self.repository = repository
        self.schema_name = schema_name
        self.extension_path = Path(__file__).parent

    async def install(self) -> None:
        """Install TestFoundry schema and all SQL objects."""
        print(f"Installing TestFoundry in schema: {self.schema_name}")

        # Create schema
        await self._create_schema()

        # Install in order:
        # 1. Tables
        await self._install_tables()

        # 2. Functions
        await self._install_functions()

        # 3. Test generators
        await self._install_generators()

        print("TestFoundry installation complete!")

    async def _create_schema(self) -> None:
        """Create the TestFoundry schema if it doesn't exist."""
        async with self.repository.get_pool().connection() as conn:
            await conn.execute(
                f"""
                CREATE SCHEMA IF NOT EXISTS {self.schema_name};
                SET search_path TO {self.schema_name}, public;
            """
            )

    async def _install_tables(self) -> None:
        """Install TestFoundry table definitions."""
        table_files = [
            "1_tables/5001_testfoundry_tb_field_mapping.sql",
            "1_tables/5002_test_entity_dependents.sql",
            "1_tables/5003_test_manual_scenarios.sql",
            "1_tables/5003_testfoundry_tb_entity_period_fields.sql",
            # Install the mapping tables from rootextension
            "rootextension/testfoundry/testfoundry_tb_fk_mapping.sql",
            "rootextension/testfoundry/testfoundry_tb_input_field_mapping.sql",
        ]

        for file_path in table_files:
            await self._execute_sql_file(file_path)

    async def _install_functions(self) -> None:
        """Install TestFoundry function definitions."""
        # Core functions
        function_files = [
            "3_functions/5030_get_entity_structure.sql",
            "3_functions/5034_testfoundry_generate_invalid_fk_input.sql",
            "3_functions/5035_test_foundry_generate_authorization_vars.sql",
            "3_functions/5036_testfoundry_insert_entity.sql",
            "3_functions/5037_testfoundry_generate_duplicate_fieldset.sql",
            "3_functions/5037_testfoundry_insert_duplicate_record_set.sql",
            "3_functions/testfoundry_select_pk_value.sql",
        ]

        # Randomizer functions
        randomizer_files = [
            "3_functions/randomize/5031_testfoundry_cast_random_value.sql",
            "3_functions/randomize/5031_testfoundry_random_value.sql",
            "3_functions/randomize/5033_testfoundry_random_entity_generator.sql",
            "3_functions/randomize/testfoundry_generate_field_value.sql",
            "3_functions/randomize/testfoundry_generate_random_input.sql",
            "3_functions/randomize/testfoundry_random_latitude_longitude.sql",
            "3_functions/randomize/testfoundry_random_text_from_mapping.sql",
            "3_functions/randomize/testfoundry_random_value_from_mapping.sql",
        ]

        # Type introspection functions
        introspection_files = [
            "3_functions/type_introspection/testfoundry_build_conditions.sql",
            "3_functions/type_introspection/testfoundry_infer_dependency_field_type.sql",
            "3_functions/type_introspection/testfoundry_list_input_fields.sql",
            "3_functions/type_introspection/testfoundry_populate_dependency_field_types.sql",
        ]

        for file_path in function_files + randomizer_files + introspection_files:
            await self._execute_sql_file(file_path)

    async def _install_generators(self) -> None:
        """Install TestFoundry test generation functions."""
        generator_files = [
            # Create generators
            "590_test_generation/51_create/501_generate_happy_create.sql",
            "590_test_generation/51_create/502_generate_duplicate_create.sql",
            "590_test_generation/51_create/503_generate_fk_violation_create.sql",
            "590_test_generation/51_create/504_test_foundry_generate_constraint_create_test.sql",
            # Delete generators
            "590_test_generation/52_delete/521_generate_happy_delete.sql",
            "590_test_generation/52_delete/522_generate_soft_delete.sql",
            "590_test_generation/52_delete/523_generate_blocked_delete.sql",
        ]

        for file_path in generator_files:
            await self._execute_sql_file(file_path)

    async def _execute_sql_file(self, relative_path: str) -> None:
        """Execute a SQL file with proper schema context."""
        file_path = self.extension_path / relative_path
        if not file_path.exists():
            print(f"Warning: SQL file not found: {file_path}")
            return

        sql_content = file_path.read_text()

        # Replace CREATE TABLE with CREATE TABLE IF NOT EXISTS (only if not already there)
        if "CREATE TABLE IF NOT EXISTS" not in sql_content:
            sql_content = sql_content.replace(
                "CREATE TABLE", "CREATE TABLE IF NOT EXISTS"
            )

        # Prepend search_path to ensure objects are created in the right schema
        sql_with_schema = (
            f"SET search_path TO {self.schema_name}, public;\n\n{sql_content}"
        )

        try:
            async with self.repository.get_pool().connection() as conn:
                await conn.execute(sql_with_schema)
            print(f"Executed: {relative_path}")
        except Exception as e:
            print(f"Error executing {relative_path}: {e}")
            # Don't raise if it's a duplicate error - just log it
            if "already exists" not in str(e):
                raise

    async def uninstall(self) -> None:
        """Remove TestFoundry schema and all objects."""
        async with self.repository.get_pool().connection() as conn:
            await conn.execute(f"DROP SCHEMA IF EXISTS {self.schema_name} CASCADE;")
        print(f"TestFoundry schema '{self.schema_name}' removed.")

    async def populate_fraiseql_metadata(
        self, entity_name: str, input_type_name: str
    ) -> None:
        """Populate TestFoundry metadata for a FraiseQL entity.

        This is where you would analyze FraiseQL types and create the necessary
        INSERT statements for testfoundry_tb_input_field_mapping and
        testfoundry_tb_fk_mapping tables.

        Args:
            entity_name: The entity/table name (e.g., 'users', 'posts')
            input_type_name: The GraphQL input type name (e.g., 'UserInput', 'PostInput')
        """
        # This would be implemented based on analyzing FraiseQL types
        # For now, it's a placeholder showing the pattern
        pass


async def setup_testfoundry_for_blog_example():
    """Example setup for the blog API."""
    # This would be called after TestFoundry is installed
    # It would create the metadata records for blog entities

    # Example SQL that would be generated:
    # example_sql = """
    # -- User input type mapping
    # INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
    # (input_type, field_name, generator_type, fk_mapping_key, random_function, required)
    # VALUES
    # ('user_input', 'email', 'random', NULL, 'testfoundry_random_email', TRUE),
    # ('user_input', 'name', 'random', NULL, NULL, TRUE),
    # ('user_input', 'bio', 'random', NULL, NULL, FALSE),
    # ('user_input', 'avatar_url', 'random', NULL, 'testfoundry_random_url', FALSE);
    #
    # -- Post input type mapping
    # INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
    # (input_type, field_name, generator_type, fk_mapping_key, fk_dependency_fields, required)
    # VALUES
    # ('post_input', 'author_id', 'resolve_fk', 'user_id', NULL, TRUE),
    # ('post_input', 'title', 'random', NULL, NULL, TRUE),
    # ('post_input', 'content', 'random', NULL, NULL, TRUE),
    # ('post_input', 'excerpt', 'random', NULL, NULL, FALSE),
    # ('post_input', 'tags', 'random', NULL, 'testfoundry_random_tags', FALSE),
    # ('post_input', 'is_published', 'random', NULL, NULL, FALSE);
    #
    # -- FK mappings
    # INSERT INTO testfoundry.testfoundry_tb_fk_mapping
    # (input_type, from_expression, select_field, random_pk_field, random_value_field, random_select_where)
    # VALUES
    # ('user_id', 'tb_users', 'id', 'id', 'email', 'deleted_at IS NULL');
    # """
