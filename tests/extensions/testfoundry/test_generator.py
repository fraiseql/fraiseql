"""Tests for FoundryGenerator."""

import pytest
import pytest_asyncio

from fraiseql.extensions.testfoundry import FoundryGenerator


@pytest.mark.database
class TestFoundryGenerator:
    """Test the FoundryGenerator class."""

    @pytest_asyncio.fixture
    async def generator(self, populated_metadata, test_config):
        """Create a generator instance with populated metadata."""
        return FoundryGenerator(populated_metadata, test_config)

    @pytest.mark.asyncio
    async def test_generator_initialization(self, testfoundry_repository, test_config):
        """Test generator initialization."""
        generator = FoundryGenerator(testfoundry_repository, test_config)

        assert generator.repository == testfoundry_repository
        assert generator.config == test_config
        assert generator.analyzer is not None

    @pytest.mark.asyncio
    async def test_generate_happy_path_test(self, generator):
        """Test generation of happy path pgTAP test."""
        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        assert "happy_create" in tests
        pgtap_content = tests["happy_create"]

        # Verify pgTAP structure
        assert "-- Happy path CREATE test for entity users" in pgtap_content
        assert "SELECT plan(" in pgtap_content
        assert "\\gset" in pgtap_content  # Authentication variables use \\gset
        assert "v_org" in pgtap_content
        assert "v_user" in pgtap_content
        assert "create_users_with_log" in pgtap_content
        assert "SELECT is(" in pgtap_content
        assert "SELECT ok(" in pgtap_content
        assert "SELECT * FROM finish();" in pgtap_content

    @pytest.mark.asyncio
    async def test_generate_all_test_types(self, generator):
        """Test generation of all configured test types."""
        # Configure to generate all test types
        generator.config.test_options = {
            "happy_path": True,
            "constraint_violations": True,
            "fk_violations": True,
            "soft_delete": True,
        }

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Should have all test types
        assert "happy_create" in tests
        assert "duplicate_create" in tests
        assert "fk_violation_create" in tests
        assert "soft_delete" in tests

        # Each test should have content
        for _test_type, content in tests.items():
            assert content is not None
            assert len(content) > 0
            assert "-- " in content  # Should have comments

    @pytest.mark.asyncio
    async def test_selective_test_generation(self, generator):
        """Test that only configured test types are generated."""
        # Configure to generate only specific tests
        generator.config.test_options = {
            "happy_path": True,
            "constraint_violations": False,
            "fk_violations": False,
            "soft_delete": True,
        }

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Should only have enabled test types
        assert "happy_create" in tests
        assert "soft_delete" in tests
        assert "duplicate_create" not in tests
        assert "fk_violation_create" not in tests

    @pytest.mark.asyncio
    async def test_write_sql_tests_to_files(self, generator, temp_test_dir):
        """Test writing SQL tests to files."""
        # Generate tests
        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Write to files
        paths = await generator.write_tests_to_files(tests, "users")

        # Verify files were created
        assert len(paths) > 0

        for path in paths:
            assert path.exists()
            assert path.suffix == ".sql"
            assert path.parent.name == "users"

            # Read and verify content
            content = path.read_text()
            assert len(content) > 0
            assert "SELECT plan(" in content or "-- " in content

    @pytest.mark.asyncio
    async def test_write_pytest_wrapped_tests(self, generator, temp_test_dir):
        """Test writing pytest-wrapped tests."""
        # Configure for pytest generation
        generator.config.generate_pytest = True

        # Generate tests
        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Write to files
        paths = await generator.write_tests_to_files(tests, "users")

        # Verify files were created
        assert len(paths) > 0

        for path in paths:
            assert path.exists()
            assert path.suffix == ".py"
            assert path.parent.name == "users"

            # Read and verify content
            content = path.read_text()
            assert "@pytest.mark.database" in content
            assert "@pytest.mark.asyncio" in content
            assert "async def test_" in content
            assert 'sql = """' in content
            assert "await db_connection.fetchval(sql)" in content

    @pytest.mark.asyncio
    async def test_error_handling_missing_function(self, generator):
        """Test error handling when SQL function is missing."""
        # Drop one of the test generation functions
        async with generator.repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(
                    "DROP FUNCTION IF EXISTS testfoundry._testfoundry_generate_happy_create CASCADE"
                )
            await conn.commit()

        # Should handle the error gracefully
        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Should have an error message for happy_create
        assert "happy_create" in tests
        assert (
            "Error generating" in tests["happy_create"]
            or "No test generated" in tests["happy_create"]
        )

    @pytest.mark.asyncio
    async def test_generate_all_tests_batch(self, generator):
        """Test batch generation for multiple entities."""
        entities = [
            {
                "entity_name": "users",
                "table_name": "tb_users",
                "input_type_name": "user_input",
            },
            {
                "entity_name": "posts",
                "table_name": "tb_posts",
                "input_type_name": "post_input",
            },
        ]

        # Should generate tests for all entities
        await generator.generate_all_tests(entities)

        # Verify files were created for each entity
        users_dir = generator.config.test_output_dir / "users"
        posts_dir = generator.config.test_output_dir / "posts"

        assert users_dir.exists()
        assert posts_dir.exists()

        # Should have at least one test file per entity
        assert len(list(users_dir.glob("*.sql"))) > 0
        assert len(list(posts_dir.glob("*.sql"))) > 0

    @pytest.mark.asyncio
    async def test_complex_entity_with_fk(self, generator):
        """Test generation for entity with foreign key relationships."""
        tests = await generator.generate_tests_for_entity(
            entity_name="posts", table_name="tb_posts", input_type_name="post_input"
        )

        # Posts should have FK violation test since it references users
        if "fk_violation_create" in tests:
            content = tests["fk_violation_create"]
            # Should attempt to create with invalid user_id
            assert "fk_violation" in content.lower() or "foreign key" in content.lower()

    @pytest.mark.asyncio
    async def test_pytest_wrapper_format(self, generator):
        """Test the pytest wrapper format in detail."""
        generator.config.generate_pytest = True

        # Create a simple test content
        sql_content = """-- Test SQL
SELECT plan(1);
SELECT ok(true, 'Simple test');
SELECT * FROM finish();"""

        wrapped = generator._wrap_in_pytest(sql_content, "users", "happy_create")

        # Verify pytest wrapper structure
        assert '"""Generated test for users happy_create."""' in wrapped
        assert "import pytest" in wrapped
        assert "import asyncpg" in wrapped
        assert "@pytest.mark.database" in wrapped
        assert "@pytest.mark.asyncio" in wrapped
        assert "async def test_users_happy_create(db_connection):" in wrapped
        assert 'sql = """' in wrapped
        assert sql_content in wrapped
        assert "result = await db_connection.fetchval(sql)" in wrapped
        assert "assert 'ok' in result.lower()" in wrapped
        assert "asyncpg.exceptions.PostgresError" in wrapped
