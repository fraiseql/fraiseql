import pytest

pytestmark = pytest.mark.integration

"""Tests for the main CLI entry point."""

from unittest.mock import patch

from fraiseql.cli.main import cli, main


@pytest.mark.unit
class TestCLIMain:
    """Test the main CLI functionality."""

    def test_cli_version(self, cli_runner) -> None:
        """Test --version flag shows version."""
        result = cli_runner.invoke(cli, ["--version"])

        assert result.exit_code == 0
        assert "fraiseql, version" in result.output
        # Verify version format matches expected pattern (semantic versioning)
        import re

        version_pattern = r"fraiseql, version \d+\.\d+\.\d+"
        assert re.search(version_pattern, result.output)

    def test_cli_help(self, cli_runner) -> None:
        """Test --help shows help text."""
        result = cli_runner.invoke(cli, ["--help"])

        assert result.exit_code == 0
        assert "FraiseQL - Production-ready GraphQL API framework for PostgreSQL" in result.output
        assert "Commands:" in result.output
        assert "init" in result.output
        assert "dev" in result.output
        assert "generate" in result.output
        assert "check" in result.output

    def test_cli_no_command(self, cli_runner) -> None:
        """Test CLI without command shows help."""
        result = cli_runner.invoke(cli, [])

        # Click returns exit code 2 when no command is provided
        assert result.exit_code == 2
        assert "Usage:" in result.output

    def test_cli_invalid_command(self, cli_runner) -> None:
        """Test invalid command shows error."""
        result = cli_runner.invoke(cli, ["invalid-command"])

        assert result.exit_code != 0
        assert "Error: No such command 'invalid-command'" in result.output

    def test_main_function_handles_exceptions(self) -> None:
        """Test main() function handles exceptions properly."""
        with (
            patch("fraiseql.cli.main.cli", side_effect=Exception("Test error")),
            patch("sys.exit") as mock_exit,
            patch("click.echo") as mock_echo,
        ):
            main()

            mock_echo.assert_called_with("Error: Test error", err=True)
            mock_exit.assert_called_with(1)

    def test_command_groups(self, cli_runner) -> None:
        """Test command groups have proper help."""
        # Test generate group
        result = cli_runner.invoke(cli, ["generate", "--help"])
        assert result.exit_code == 0
        assert "Generate code from your FraiseQL schema" in result.output
        assert "schema" in result.output
        assert "migration" in result.output
        assert "crud" in result.output
