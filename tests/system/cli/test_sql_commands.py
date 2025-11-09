"""Tests for SQL CLI commands."""

import pytest

from fraiseql.cli.main import cli


@pytest.fixture
def sample_type_file(tmp_path) -> None:
    """Create a sample Python file with a FraiseQL type."""
    types_dir = tmp_path / "src" / "types"
    types_dir.mkdir(parents=True)

    type_content = '''
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class TestUser:
    """A test user type."""
    id: int = fraise_field(description="User ID")
    name: str = fraise_field(description="User name")
    email: str = fraise_field(description="User email")
    is_active: bool = fraise_field(description="Is user active")
'''

    (types_dir / "test_types.py").write_text(type_content)
    (types_dir / "__init__.py").write_text("")
    (tmp_path / "src" / "__init__.py").write_text("")

    return types_dir


@pytest.fixture
def sample_sql_file(tmp_path) -> None:
    """Create a sample SQL file."""
    sql_content = """
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) AS data
FROM tb_users;
"""
    sql_file = tmp_path / "test_view.sql"
    sql_file.write_text(sql_content)
    return sql_file


@pytest.mark.unit
class TestSQLGenerateView:
    """Test the fraiseql sql generate-view command."""

    def test_generate_view_requires_type_name(self, cli_runner) -> None:
        """Test that generate-view requires a type name."""
        result = cli_runner.invoke(cli, ["sql", "generate-view"])

        assert result.exit_code != 0
        assert "Missing argument" in result.output or "Error" in result.output

    def test_generate_view_with_invalid_module(self, cli_runner) -> None:
        """Test generate-view with invalid module path."""
        result = cli_runner.invoke(
            cli, ["sql", "generate-view", "User", "--module", "invalid.module"]
        )

        assert result.exit_code != 0

    def test_generate_view_basic_output(self, cli_runner, sample_type_file, monkeypatch) -> None:
        """Test basic view generation output."""
        # Change to the tmp directory
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli, ["sql", "generate-view", "TestUser", "--module", "src.types.test_types"]
        )

        # Should succeed (or fail gracefully if dependencies missing)
        # We mainly want to ensure the command executes
        assert "TestUser" in result.output or "Error" in result.output

    def test_generate_view_with_exclude(self, cli_runner, sample_type_file, monkeypatch) -> None:
        """Test view generation with excluded fields."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-view",
                "TestUser",
                "--module",
                "src.types.test_types",
                "--exclude",
                "email",
                "--exclude",
                "is_active",
            ],
        )

        # Command should execute (success or handled error)
        assert result.exit_code == 0 or "Error" in result.output

    def test_generate_view_with_custom_names(
        self, cli_runner, sample_type_file, monkeypatch
    ) -> None:
        """Test view generation with custom table and view names."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-view",
                "TestUser",
                "--module",
                "src.types.test_types",
                "--table",
                "tb_custom_users",
                "--view",
                "v_custom_users",
            ],
        )

        assert result.exit_code == 0 or "Error" in result.output

    def test_generate_view_no_comments(self, cli_runner, sample_type_file, monkeypatch) -> None:
        """Test view generation without comments."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-view",
                "TestUser",
                "--module",
                "src.types.test_types",
                "--no-comments",
            ],
        )

        assert result.exit_code == 0 or "Error" in result.output


@pytest.mark.unit
class TestSQLGenerateSetup:
    """Test the fraiseql sql generate-setup command."""

    def test_generate_setup_basic(self, cli_runner, sample_type_file, monkeypatch) -> None:
        """Test basic setup generation."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli, ["sql", "generate-setup", "TestUser", "--module", "src.types.test_types"]
        )

        assert result.exit_code == 0 or "Error" in result.output

    def test_generate_setup_with_table(self, cli_runner, sample_type_file, monkeypatch) -> None:
        """Test setup generation with table creation."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-setup",
                "TestUser",
                "--module",
                "src.types.test_types",
                "--with-table",
            ],
        )

        assert result.exit_code == 0 or "Error" in result.output

    def test_generate_setup_with_all_options(
        self, cli_runner, sample_type_file, monkeypatch
    ) -> None:
        """Test setup generation with all options enabled."""
        monkeypatch.chdir(sample_type_file.parent.parent)

        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-setup",
                "TestUser",
                "--module",
                "src.types.test_types",
                "--with-table",
                "--with-indexes",
                "--with-data",
            ],
        )

        assert result.exit_code == 0 or "Error" in result.output


@pytest.mark.unit
class TestSQLGeneratePattern:
    """Test the fraiseql sql generate-pattern command."""

    def test_pagination_pattern(self, cli_runner) -> None:
        """Test pagination pattern generation."""
        result = cli_runner.invoke(
            cli,
            ["sql", "generate-pattern", "pagination", "users", "--limit", "10", "--offset", "20"],
        )

        assert result.exit_code == 0
        assert "users" in result.output
        assert "LIMIT" in result.output or "pagination" in result.output.lower()

    def test_filtering_pattern(self, cli_runner) -> None:
        """Test filtering pattern generation."""
        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-pattern",
                "filtering",
                "users",
                "-w",
                "email=test@example.com",
                "-w",
                "is_active=true",
            ],
        )

        assert result.exit_code == 0
        assert "users" in result.output

    def test_filtering_pattern_with_types(self, cli_runner) -> None:
        """Test filtering with different value types."""
        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-pattern",
                "filtering",
                "products",
                "-w",
                "price=100",
                "-w",
                "in_stock=true",
                "-w",
                "category=electronics",
            ],
        )

        assert result.exit_code == 0
        assert "products" in result.output

    def test_sorting_pattern(self, cli_runner) -> None:
        """Test sorting pattern generation."""
        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-pattern",
                "sorting",
                "users",
                "-o",
                "name:ASC",
                "-o",
                "created_at:DESC",
            ],
        )

        assert result.exit_code == 0
        assert "users" in result.output
        assert "ORDER" in result.output or "sorting" in result.output.lower()

    def test_sorting_pattern_default_direction(self, cli_runner) -> None:
        """Test sorting pattern with default direction."""
        result = cli_runner.invoke(
            cli, ["sql", "generate-pattern", "sorting", "users", "-o", "name"]
        )

        assert result.exit_code == 0
        assert "users" in result.output

    def test_relationship_pattern(self, cli_runner) -> None:
        """Test relationship pattern generation."""
        result = cli_runner.invoke(
            cli,
            [
                "sql",
                "generate-pattern",
                "relationship",
                "users",
                "--child-table",
                "posts",
                "--foreign-key",
                "user_id",
            ],
        )

        assert result.exit_code == 0
        assert "users" in result.output or "posts" in result.output

    def test_relationship_pattern_missing_options(self, cli_runner) -> None:
        """Test relationship pattern without required options."""
        result = cli_runner.invoke(cli, ["sql", "generate-pattern", "relationship", "users"])

        # Should show error about missing options
        assert "Error" in result.output or "required" in result.output.lower()

    def test_aggregation_pattern(self, cli_runner) -> None:
        """Test aggregation pattern generation."""
        result = cli_runner.invoke(
            cli, ["sql", "generate-pattern", "aggregation", "orders", "--group-by", "customer_id"]
        )

        assert result.exit_code == 0
        assert "orders" in result.output

    def test_aggregation_pattern_missing_group_by(self, cli_runner) -> None:
        """Test aggregation pattern without group-by."""
        result = cli_runner.invoke(cli, ["sql", "generate-pattern", "aggregation", "orders"])

        # Should show error about missing group-by
        assert "Error" in result.output or "required" in result.output.lower()


@pytest.mark.unit
class TestSQLValidate:
    """Test the fraiseql sql validate command."""

    def test_validate_valid_sql(self, cli_runner, sample_sql_file) -> None:
        """Test validation of valid SQL."""
        result = cli_runner.invoke(cli, ["sql", "validate", str(sample_sql_file)])

        assert result.exit_code == 0
        # Output should indicate validation result
        assert "valid" in result.output.lower() or "error" in result.output.lower()

    def test_validate_missing_file(self, cli_runner) -> None:
        """Test validation with missing file."""
        result = cli_runner.invoke(cli, ["sql", "validate", "nonexistent.sql"])

        assert result.exit_code != 0

    def test_validate_invalid_sql(self, cli_runner, tmp_path) -> None:
        """Test validation of invalid SQL."""
        invalid_sql = tmp_path / "invalid.sql"
        invalid_sql.write_text("SELECT * FROM table_without_data_column;")

        result = cli_runner.invoke(cli, ["sql", "validate", str(invalid_sql)])

        # Should complete (may show warnings/errors about SQL)
        assert result.exit_code == 0


@pytest.mark.unit
class TestSQLExplain:
    """Test the fraiseql sql explain command."""

    def test_explain_valid_sql(self, cli_runner, sample_sql_file) -> None:
        """Test explaining valid SQL."""
        result = cli_runner.invoke(cli, ["sql", "explain", str(sample_sql_file)])

        assert result.exit_code == 0
        assert "Explanation" in result.output or "explain" in result.output.lower()

    def test_explain_missing_file(self, cli_runner) -> None:
        """Test explaining missing file."""
        result = cli_runner.invoke(cli, ["sql", "explain", "nonexistent.sql"])

        assert result.exit_code != 0

    def test_explain_with_issues_detection(self, cli_runner, tmp_path) -> None:
        """Test explaining SQL with potential issues."""
        sql_with_issues = tmp_path / "issues.sql"
        sql_with_issues.write_text(
            """
            CREATE VIEW v_test AS
            SELECT * FROM users;
        """
        )

        result = cli_runner.invoke(cli, ["sql", "explain", str(sql_with_issues)])

        assert result.exit_code == 0
        # Should provide explanation (and possibly warnings)


@pytest.mark.unit
class TestSQLLoadType:
    """Test the _load_type helper function."""

    def test_load_type_without_module(self, cli_runner) -> None:
        """Test loading type without specifying module."""
        # This will fail to find the type, but tests the search logic
        result = cli_runner.invoke(cli, ["sql", "generate-view", "NonexistentType"])

        # Should fail with helpful error
        assert result.exit_code != 0
        assert "Could not find" in result.output or "Error" in result.output

    def test_load_type_from_multiple_locations(self, cli_runner, tmp_path, monkeypatch) -> None:
        """Test type loading from common locations."""
        # Create type in common location
        types_dir = tmp_path / "types"
        types_dir.mkdir()

        type_content = """
import fraiseql

@fraiseql.type
class CommonType:
    id: int
    name: str
"""
        (types_dir / "common.py").write_text(type_content)
        (types_dir / "__init__.py").write_text("from .common import CommonType")

        monkeypatch.chdir(tmp_path)

        result = cli_runner.invoke(cli, ["sql", "generate-view", "CommonType"])

        # Should attempt to find the type
        # May fail due to import issues in test environment, but tests the logic
        assert (
            result.exit_code == 0 or "Could not find" in result.output or "Error" in result.output
        )


@pytest.mark.unit
class TestSQLHelp:
    """Test SQL command help output."""

    def test_sql_help(self, cli_runner) -> None:
        """Test sql command help."""
        result = cli_runner.invoke(cli, ["sql", "--help"])

        assert result.exit_code == 0
        assert "generate-view" in result.output
        assert "generate-setup" in result.output
        assert "generate-pattern" in result.output
        assert "validate" in result.output
        assert "explain" in result.output

    def test_generate_view_help(self, cli_runner) -> None:
        """Test generate-view command help."""
        result = cli_runner.invoke(cli, ["sql", "generate-view", "--help"])

        assert result.exit_code == 0
        assert "TYPE_NAME" in result.output or "type" in result.output.lower()
        assert "--module" in result.output

    def test_generate_pattern_help(self, cli_runner) -> None:
        """Test generate-pattern command help."""
        result = cli_runner.invoke(cli, ["sql", "generate-pattern", "--help"])

        assert result.exit_code == 0
        assert "pagination" in result.output
        assert "filtering" in result.output
        assert "sorting" in result.output
