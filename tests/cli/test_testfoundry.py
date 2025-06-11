"""Tests for the testfoundry commands."""

from pathlib import Path
from unittest.mock import AsyncMock, patch

from fraiseql.cli.main import cli


class TestTestFoundryInstall:
    """Test the testfoundry install command."""

    def test_install_success(self, cli_runner, monkeypatch):
        """Test successful TestFoundry installation."""
        # Set DATABASE_URL for the test
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        # We need to mock the actual database operations
        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundrySetup") as mock_setup_class,
        ):
            # Mock repository
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            # Mock setup - needs to be a regular Mock, not AsyncMock
            mock_setup = type("MockSetup", (), {"install": AsyncMock()})()
            mock_setup_class.return_value = mock_setup

            result = cli_runner.invoke(cli, ["testfoundry", "install"])

            assert result.exit_code == 0
            assert "Installing TestFoundry" in result.output
            assert "TestFoundry installed successfully!" in result.output

            mock_setup_class.assert_called_once_with(
                mock_repo, schema_name="testfoundry"
            )
            mock_setup.install.assert_called_once()
            mock_repo.close.assert_called_once()

    def test_install_no_database_url(self, cli_runner, monkeypatch):
        """Test error when DATABASE_URL is not set."""
        # Ensure DATABASE_URL is not set
        monkeypatch.delenv("DATABASE_URL", raising=False)

        result = cli_runner.invoke(cli, ["testfoundry", "install"])

        assert result.exit_code != 0
        assert "DATABASE_URL not set" in result.output

    def test_install_custom_schema(self, cli_runner, monkeypatch):
        """Test installation with custom schema name."""
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundrySetup") as mock_setup_class,
        ):
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            mock_setup = type("MockSetup", (), {"install": AsyncMock()})()
            mock_setup_class.return_value = mock_setup

            result = cli_runner.invoke(
                cli, ["testfoundry", "install", "--schema", "custom_tf"]
            )

            assert result.exit_code == 0
            mock_setup_class.assert_called_once_with(mock_repo, schema_name="custom_tf")

    def test_install_handles_error(self, cli_runner, monkeypatch):
        """Test error handling during installation."""
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundrySetup") as mock_setup_class,
        ):
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            mock_setup = type(
                "MockSetup",
                (),
                {"install": AsyncMock(side_effect=Exception("Connection failed"))},
            )()
            mock_setup_class.return_value = mock_setup

            result = cli_runner.invoke(cli, ["testfoundry", "install"])

            assert result.exit_code != 0
            assert "Installation failed: Connection failed" in result.output
            mock_repo.close.assert_called_once()


class TestTestFoundryGenerate:
    """Test the testfoundry generate command."""

    def test_generate_success(self, cli_runner, temp_project_dir, monkeypatch):
        """Test successful test generation."""
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        # We still need to mock database operations
        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundryGenerator") as mock_gen_class,
        ):
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            test_dict = {
                "happy_create": "-- Happy path test",
                "duplicate_create": "-- Duplicate test",
            }
            mock_generator = type(
                "MockGenerator",
                (),
                {
                    "generate_tests_for_entity": AsyncMock(return_value=test_dict),
                    "write_tests_to_files": AsyncMock(),
                },
            )()
            mock_gen_class.return_value = mock_generator

            result = cli_runner.invoke(cli, ["testfoundry", "generate", "User"])

            assert result.exit_code == 0
            assert "Generating tests for User" in result.output
            assert "Tests generated in tests/generated" in result.output
            assert "- happy_create" in result.output
            assert "- duplicate_create" in result.output

            mock_generator.generate_tests_for_entity.assert_called_once_with(
                "User", "users"
            )

    def test_generate_custom_output(self, cli_runner, monkeypatch):
        """Test generation with custom output directory."""
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundryGenerator") as mock_gen_class,
        ):
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            mock_generator = type(
                "MockGenerator",
                (),
                {
                    "generate_tests_for_entity": AsyncMock(return_value={}),
                    "write_tests_to_files": AsyncMock(),
                },
            )()
            mock_gen_class.return_value = mock_generator

            result = cli_runner.invoke(
                cli, ["testfoundry", "generate", "Post", "-o", "custom/tests"]
            )

            assert result.exit_code == 0
            assert "Tests generated in custom/tests" in result.output

            # Check write_tests_to_files was called with custom path
            call_args = mock_generator.write_tests_to_files.call_args
            assert call_args[0][1] == Path("custom/tests")


class TestTestFoundryAnalyze:
    """Test the testfoundry analyze command."""

    def test_analyze_success(self, cli_runner, temp_project_dir):
        """Test successful type analysis."""
        # Create a real test module with type
        Path("src").mkdir()
        Path("src/types").mkdir()
        (Path("src/types") / "__init__.py").write_text(
            '''
from fraiseql import fraise_input, fraise_field

@fraise_input
class CreateUserInput:
    """Input for creating a user."""
    name: str = fraise_field(description="User name")
    email: str = fraise_field(description="User email")
    user_id: int = fraise_field(description="User ID")
'''
        )

        # We still need to mock the analyzer since it requires database analysis
        with patch(
            "fraiseql.extensions.testfoundry.FoundryAnalyzer"
        ) as mock_analyzer_class:
            # Create a mock field mapping with proper attributes
            class MockFieldMapping:
                def __init__(
                    self,
                    field_name,
                    generator_type,
                    random_function=None,
                    fk_mapping_key=None,
                    required=True,
                ):
                    self.field_name = field_name
                    self.generator_type = generator_type
                    self.random_function = random_function
                    self.fk_mapping_key = fk_mapping_key
                    self.required = required

            mock_mappings = [
                MockFieldMapping("name", "random"),
                MockFieldMapping("email", "random", "testfoundry_random_email"),
                MockFieldMapping("user_id", "resolve_fk", fk_mapping_key="user_id"),
            ]
            mock_analyzer = type(
                "MockAnalyzer",
                (),
                {
                    "analyze_input_type": lambda self, *args: mock_mappings,
                    "generate_sql_statements": lambda self, *args: "INSERT INTO ...",
                },
            )()
            mock_analyzer_class.return_value = mock_analyzer

            result = cli_runner.invoke(
                cli, ["testfoundry", "analyze", "User", "CreateUserInput"]
            )

            assert result.exit_code == 0
            assert "Analyzing CreateUserInput for User" in result.output
            assert "Field: name" in result.output
            assert "Type: random" in result.output
            assert "Field: email" in result.output
            assert "Random Function: testfoundry_random_email" in result.output
            assert "Field: user_id" in result.output
            assert "FK Mapping: user_id" in result.output
            assert "Generated SQL:" in result.output

    def test_analyze_type_not_found(self, cli_runner, temp_project_dir):
        """Test error when type is not found."""
        # Create module without the requested type
        Path("src").mkdir()
        Path("src/types").mkdir()
        (Path("src/types") / "__init__.py").write_text(
            """
from fraiseql import fraise_input

@fraise_input
class SomeOtherInput:
    name: str
"""
        )

        result = cli_runner.invoke(
            cli, ["testfoundry", "analyze", "User", "CreateUserInput"]
        )

        assert result.exit_code != 0
        assert "Type 'CreateUserInput' not found" in result.output


class TestTestFoundryUninstall:
    """Test the testfoundry uninstall command."""

    def test_uninstall_confirmed(self, cli_runner, monkeypatch):
        """Test uninstall with confirmation."""
        monkeypatch.setenv("DATABASE_URL", "postgresql://test/db")

        with (
            patch("fraiseql.cqrs.CQRSRepository") as mock_repo_class,
            patch("fraiseql.extensions.testfoundry.FoundrySetup") as mock_setup_class,
        ):
            mock_repo = AsyncMock()
            mock_repo.close = AsyncMock()
            mock_repo_class.return_value = mock_repo

            mock_setup = type("MockSetup", (), {"uninstall": AsyncMock()})()
            mock_setup_class.return_value = mock_setup

            result = cli_runner.invoke(cli, ["testfoundry", "uninstall"], input="y\n")

            assert result.exit_code == 0
            assert "This will remove TestFoundry" in result.output
            assert "TestFoundry uninstalled" in result.output

            mock_setup.uninstall.assert_called_once()
            mock_repo.close.assert_called_once()

    def test_uninstall_cancelled(self, cli_runner):
        """Test uninstall cancelled by user."""
        result = cli_runner.invoke(cli, ["testfoundry", "uninstall"], input="n\n")

        assert result.exit_code == 0
        assert "This will remove TestFoundry" in result.output
        assert "TestFoundry uninstalled" not in result.output
