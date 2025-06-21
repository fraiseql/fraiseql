#!/usr/bin/env python3
"""Script to help migrate existing mock-based tests to use real PostgreSQL.

This script analyzes test files and provides guidance on converting them
to use the new database fixtures.
"""

import ast
import sys
from pathlib import Path


class MockDetector(ast.NodeVisitor):
    """AST visitor to detect mock usage in test files."""

    def __init__(self):
        self.mock_imports = []
        self.mock_usages = []
        self.async_functions = []

    def visit_ImportFrom(self, node):
        """Detect imports from unittest.mock."""
        if node.module and "mock" in node.module:
            for alias in node.names:
                self.mock_imports.append((node.lineno, alias.name))
        self.generic_visit(node)

    def visit_Import(self, node):
        """Detect mock imports."""
        for alias in node.names:
            if "mock" in alias.name.lower():
                self.mock_imports.append((node.lineno, alias.name))
        self.generic_visit(node)

    def visit_Call(self, node):
        """Detect Mock/AsyncMock usage."""
        if isinstance(node.func, ast.Name):
            if node.func.id in ["Mock", "AsyncMock", "MagicMock"]:
                self.mock_usages.append((node.lineno, node.func.id))
        elif isinstance(node.func, ast.Attribute):
            if node.func.attr in ["Mock", "AsyncMock", "MagicMock", "patch"]:
                self.mock_usages.append((node.lineno, node.func.attr))
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node):
        """Track async test functions."""
        if node.name.startswith("test_"):
            self.async_functions.append((node.lineno, node.name))
        self.generic_visit(node)


def analyze_test_file(filepath: Path) -> tuple[list, list, list]:
    """Analyze a test file for mock usage."""
    with filepath.open() as f:
        content = f.read()

    try:
        tree = ast.parse(content)
        detector = MockDetector()
        detector.visit(tree)
        return detector.mock_imports, detector.mock_usages, detector.async_functions  # noqa: TRY300
    except SyntaxError:
        return [], [], []


def generate_migration_suggestions(filepath: Path, mock_info: tuple[list, list, list]):
    """Generate suggestions for migrating a test file."""
    mock_imports, mock_usages, async_functions = mock_info

    if not mock_imports and not mock_usages:
        return None

    suggestions = [f"\n## Migration suggestions for {filepath.name}:\n"]

    # Check what's being mocked
    with filepath.open() as f:
        content = f.read()

    if "AsyncConnectionPool" in content or "psycopg" in content:
        suggestions.append("### Database-related mocks detected")
        suggestions.append("1. Replace mock fixtures with real database fixtures:")
        suggestions.append("   - Use `@pytest.mark.database` to mark database tests")
        suggestions.append("   - Replace `mock_pool` with `db_pool` fixture")
        suggestions.append(
            "   - Replace `mock_connection` with `db_connection` fixture",
        )
        suggestions.append("")
        suggestions.append("2. Update test setup:")
        suggestions.append("   ```python")
        suggestions.append("   @pytest.mark.database")
        suggestions.append("   class TestYourClass:")
        suggestions.append("       @pytest.fixture")
        suggestions.append(
            "       async def test_schema(self, db_connection, create_test_table):",
        )
        suggestions.append("           # Create your test tables here")
        suggestions.append(
            "           await create_test_table(db_connection, 'table_name', 'CREATE TABLE ...')",
        )
        suggestions.append("   ```")
        suggestions.append("")
        suggestions.append("3. Replace mock assertions with real queries:")
        suggestions.append(
            "   - Instead of `mock.assert_called_with()`, verify actual database state",
        )
        suggestions.append("   - Use real SQL queries to check results")
        suggestions.append("")

    if async_functions:
        suggestions.append("### Async test functions detected")
        suggestions.append("- These will work seamlessly with the new fixtures")
        suggestions.append("- Ensure you're using `@pytest.mark.asyncio` decorator")
        suggestions.append("")

    suggestions.append("### Example migration:")
    suggestions.append("```python")
    suggestions.append("# Old mock-based test")
    suggestions.append("def test_query(self, mock_pool):")
    suggestions.append("    mock_cursor = AsyncMock()")
    suggestions.append("    mock_cursor.fetchall.return_value = [{'id': 1}]")
    suggestions.append("    # ... setup mocks ...")
    suggestions.append("")
    suggestions.append("# New integration test")
    suggestions.append("@pytest.mark.database")
    suggestions.append("async def test_query(self, repository, test_schema):")
    suggestions.append("    result = await repository.run(query)")
    suggestions.append("    assert result[0]['id'] == 1")
    suggestions.append("```")

    return "\n".join(suggestions)


def main():
    """Main function to analyze test files."""
    # Find all test files
    test_dir = Path("tests")
    if not test_dir.exists():
        sys.exit(1)

    test_files = list(test_dir.rglob("test_*.py"))

    files_with_mocks = []

    for test_file in sorted(test_files):
        mock_info = analyze_test_file(test_file)
        if mock_info[0] or mock_info[1]:  # Has mock imports or usages
            files_with_mocks.append((test_file, mock_info))

    if not files_with_mocks:
        return

    for test_file, _ in files_with_mocks:
        pass

    # Generate detailed suggestions
    for test_file, mock_info in files_with_mocks:
        suggestions = generate_migration_suggestions(test_file, mock_info)
        if suggestions:
            pass

    # General guidance


if __name__ == "__main__":
    main()
