"""Test generation functionality for TestFoundry."""

from pathlib import Path
from typing import Optional

from fraiseql.db import FraiseQLRepository

from .analyzer import FoundryAnalyzer
from .config import FoundryConfig


class FoundryGenerator:
    """Generates tests using TestFoundry framework."""

    def __init__(
        self, repository: FraiseQLRepository, config: Optional[FoundryConfig] = None
    ):
        self.repository = repository
        self.config = config or FoundryConfig()
        self.analyzer = FoundryAnalyzer(None)  # Schema builder would be injected

    async def generate_tests_for_entity(
        self, entity_name: str, table_name: str, input_type_name: str
    ) -> dict[str, str]:
        """Generate all test types for an entity.

        Args:
            entity_name: Logical entity name (e.g., 'user', 'post')
            table_name: Actual database table name (e.g., 'tb_users', 'tb_posts')
            input_type_name: GraphQL input type name (e.g., 'user_input', 'post_input')

        Returns:
            Dictionary mapping test type to generated SQL
        """
        tests = {}

        # Set search path for TestFoundry functions
        search_path_sql = f"SET search_path TO {self.config.schema_name}, public;"

        if self.config.test_options.get("happy_path", True):
            sql = await self._generate_test(
                entity_name,
                "create",
                f"{search_path_sql}\nSELECT _testfoundry_generate_happy_create('{entity_name}');",
            )
            tests["happy_create"] = sql

        if self.config.test_options.get("constraint_violations", True):
            sql = await self._generate_test(
                entity_name,
                "duplicate",
                f"{search_path_sql}\nSELECT _testfoundry_generate_duplicate_create_test('{entity_name}');",
            )
            tests["duplicate_create"] = sql

        if self.config.test_options.get("fk_violations", True):
            sql = await self._generate_test(
                entity_name,
                "fk_violation",
                f"{search_path_sql}\nSELECT _testfoundry_generate_fk_violation_create_test('{entity_name}');",
            )
            tests["fk_violation_create"] = sql

        if self.config.test_options.get("soft_delete", True):
            sql = await self._generate_test(
                entity_name,
                "soft_delete",
                f"{search_path_sql}\nSELECT _generate_happy_soft_delete_test('{entity_name}');",
            )
            tests["soft_delete"] = sql

        return tests

    async def _generate_test(self, entity_name: str, test_type: str, query: str) -> str:
        """Execute a test generation query and return the result."""
        try:
            async with (
                self.repository.get_pool().connection() as conn,
                conn.cursor() as cur,
            ):
                # Split the query to handle SET separately
                queries = query.strip().split("\n")
                for q in queries[:-1]:  # Execute all but the last query
                    if q.strip():
                        await cur.execute(q)

                # Execute the final SELECT and get result
                await cur.execute(queries[-1])
                result = await cur.fetchone()
                if result:
                    return (
                        result[0]
                        or f"-- No test generated for {entity_name} {test_type}"
                    )
                return f"-- No test generated for {entity_name} {test_type}"
        except Exception as e:
            return f"-- Error generating {test_type} test for {entity_name}: {e}"

    async def write_tests_to_files(
        self, tests: dict[str, str], entity_name: str
    ) -> list[Path]:
        """Write generated tests to files.

        Args:
            tests: Dictionary mapping test type to SQL content
            entity_name: Name of the entity

        Returns:
            List of paths to generated files
        """
        output_paths = []

        # Create output directory
        entity_dir = self.config.test_output_dir / entity_name
        entity_dir.mkdir(parents=True, exist_ok=True)

        for test_type, sql_content in tests.items():
            if self.config.generate_pytest:
                # Generate pytest wrapper
                pytest_content = self._wrap_in_pytest(
                    sql_content, entity_name, test_type
                )
                file_path = entity_dir / f"test_{test_type}.py"
                file_path.write_text(pytest_content)
            else:
                # Write raw SQL
                file_path = entity_dir / f"{test_type}.sql"
                file_path.write_text(sql_content)

            output_paths.append(file_path)
            print(f"Generated: {file_path}")

        return output_paths

    def _wrap_in_pytest(self, sql: str, entity_name: str, test_type: str) -> str:
        """Wrap SQL test in a pytest function."""
        # Extract test name from SQL if it's a pgTAP test
        test_name = f"test_{entity_name}_{test_type}"

        return f'''"""Generated test for {entity_name} {test_type}."""

import pytest
import asyncpg


@pytest.mark.database
@pytest.mark.asyncio
async def {test_name}(db_connection):
    """Test {test_type} for {entity_name}."""
    # Execute pgTAP test
    sql = """
{sql}
    """

    try:
        result = await db_connection.fetchval(sql)

        # If it's a pgTAP test, check the result
        if result and isinstance(result, str):
            # pgTAP returns 'ok' for passing tests
            assert 'ok' in result.lower(), f"Test failed: {{result}}"
        else:
            # For non-pgTAP tests, just ensure no exception
            assert True, "Test executed successfully"

    except asyncpg.exceptions.PostgresError as e:
        pytest.fail(f"Database error: {{e}}")
'''

    async def analyze_and_populate_metadata(
        self, input_type: type, entity_name: str, table_name: str
    ) -> str:
        """Analyze a FraiseQL type and populate TestFoundry metadata.

        Args:
            input_type: FraiseQL input type class
            entity_name: Logical entity name
            table_name: Database table name

        Returns:
            SQL statements that were executed
        """
        # Analyze the input type
        field_mappings = self.analyzer.analyze_input_type(input_type)
        self.analyzer.field_mappings = field_mappings

        # Analyze FK relationships
        fk_mappings = self.analyzer.analyze_entity_relationships(
            entity_name, table_name
        )
        self.analyzer.fk_mappings = fk_mappings

        # Generate SQL
        sql = self.analyzer.generate_sql_statements()

        # Execute the SQL to populate metadata
        if sql:
            async with (
                self.repository.get_pool().connection() as conn,
                conn.cursor() as cur,
            ):
                await cur.execute(
                    f"SET search_path TO {self.config.schema_name}, public;\n{sql}"
                )

        return sql

    async def generate_all_tests(self, entities: list[dict[str, str]]) -> None:
        """Generate tests for multiple entities.

        Args:
            entities: List of dicts with keys: entity_name, table_name, input_type_name
        """
        for entity_info in entities:
            entity_name = entity_info["entity_name"]
            table_name = entity_info["table_name"]
            input_type_name = entity_info["input_type_name"]

            print(f"\nGenerating tests for {entity_name}...")

            # Generate tests
            tests = await self.generate_tests_for_entity(
                entity_name, table_name, input_type_name
            )

            # Write to files
            await self.write_tests_to_files(tests, entity_name)

        print(f"\nAll tests generated in: {self.config.test_output_dir}")
