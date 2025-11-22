import pytest

"""Tests for the check command."""

from pathlib import Path

from fraiseql.cli.main import cli


@pytest.mark.unit
class TestCheckCommand:
    """Test the fraiseql check command."""

    def test_check_requires_project(self, cli_runner, temp_project_dir) -> None:
        """Test that check requires being in a project directory."""
        result = cli_runner.invoke(cli, ["check"])

        assert result.exit_code != 0
        assert "Not in a FraiseQL project directory" in result.output

    def test_check_with_valid_project(self, cli_runner, temp_project_dir) -> None:
        """Test check command with a valid project structure."""
        # Create project structure
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()
        Path("tests").mkdir()
        Path("migrations").mkdir()

        # Create a valid main.py
        (Path("src") / "main.py").write_text(
            """
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    id: int = fraise_field(description="User ID")
    name: str = fraise_field(description="User name")

@fraiseql.type
class QueryRoot:
    users: list[User] = fraise_field(default_factory=list, description="All users")

    async def resolve_users(self, info) -> None:
        return []

app = fraiseql.create_fraiseql_app(queries=[QueryRoot])
""",
        )

        result = cli_runner.invoke(cli, ["check"])

        assert result.exit_code == 0
        assert "Checking FraiseQL project" in result.output
        assert "✅ src/" in result.output
        assert "✅ tests/" in result.output
        assert "✅ migrations/" in result.output
        assert "✅ Found FraiseQL app" in result.output
        assert "✨ All checks passed!" in result.output

    def test_check_missing_directories(self, cli_runner, temp_project_dir) -> None:
        """Test check warns about missing directories."""
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()
        (Path("src") / "main.py").write_text("app = None")

        result = cli_runner.invoke(cli, ["check"])

        assert "❌ tests/ (missing)" in result.output
        assert "❌ migrations/ (missing)" in result.output
        assert "Warning: Missing directories: tests, migrations" in result.output

    def test_check_missing_main_py(self, cli_runner, temp_project_dir) -> None:
        """Test check fails when main.py is missing."""
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()

        result = cli_runner.invoke(cli, ["check"])

        assert result.exit_code != 0
        assert "❌ src/main.py not found" in result.output

    def test_check_no_app_in_main(self, cli_runner, temp_project_dir) -> None:
        """Test check warns when no app is found in main.py."""
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()
        Path("tests").mkdir()
        Path("migrations").mkdir()
        (Path("src") / "main.py").write_text("# No app defined")

        result = cli_runner.invoke(cli, ["check"])

        assert result.exit_code == 0  # Still passes but with warning
        assert "⚠️  No 'app' found in src/main.py" in result.output

    def test_check_import_error(self, cli_runner, temp_project_dir) -> None:
        """Test check handles import errors gracefully."""
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()
        (Path("src") / "main.py").write_text("import nonexistent_module")

        result = cli_runner.invoke(cli, ["check"])

        assert result.exit_code != 0
        assert "❌ Import error:" in result.output
        assert "Make sure all dependencies are installed" in result.output

    def test_check_type_validation_error(self, cli_runner, temp_project_dir) -> None:
        """Test check handles type validation errors."""
        Path("pyproject.toml").write_text('[project]\nname = "test"')
        Path("src").mkdir()
        Path("tests").mkdir()
        Path("migrations").mkdir()

        # Create main.py with invalid type definition
        (Path("src") / "main.py").write_text(
            """
import fraiseql

# This will cause an error when building schema
@fraiseql.type
class BadType:
    # Invalid: circular reference without proper annotation
    self: "BadType"

app = fraiseql.create_fraiseql_app(types=[BadType])
""",
        )

        result = cli_runner.invoke(cli, ["check"])

        # The exact behavior depends on how FraiseQL handles invalid types
        # But it should provide some error feedback
        assert "Validating FraiseQL types" in result.output
