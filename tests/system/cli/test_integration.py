"""Integration tests for CLI commands working together."""

import os
from pathlib import Path
from unittest.mock import patch

import pytest

from fraiseql.cli.main import cli

pytestmark = pytest.mark.integration


class TestCLIIntegration:
    """Test CLI commands working together in realistic scenarios."""

    def test_full_project_workflow(self, cli_runner, temp_project_dir) -> None:
        """Test creating and setting up a complete project."""
        # 1. Initialize project
        result = cli_runner.invoke(cli, ["init", "testapp", "--no-git"])
        assert result.exit_code == 0

        # Move into project directory
        os.chdir("testapp")

        # 2. Check the project structure is valid
        assert Path("pyproject.toml").exists()
        assert Path("src/main.py").exists()
        assert Path(".env").exists()

        # 3. Generate a migration
        with patch(
            "fraiseql.cli.commands.generate.get_timestamp",
            return_value="20250610120000",
        ):
            result = cli_runner.invoke(cli, ["generate", "migration", "User"])
            assert result.exit_code == 0
            assert Path("migrations/20250610120000_create_users.sql").exists()

        # 4. Generate CRUD mutations
        result = cli_runner.invoke(cli, ["generate", "crud", "User"])
        assert result.exit_code == 0
        assert Path("src/mutations/user_mutations.py").exists()

        # 5. Check types (should validate the project)
        result = cli_runner.invoke(cli, ["check"])
        assert result.exit_code == 0
        assert "Checking FraiseQL project" in result.output
        assert "All checks passed!" in result.output

    def test_blog_template_workflow(self, cli_runner, temp_project_dir) -> None:
        """Test blog template project setup."""
        # Create blog project
        result = cli_runner.invoke(cli, ["init", "myblog", "--template", "blog", "--no-git"])
        assert result.exit_code == 0

        # Work in the blog directory
        blog_path = temp_project_dir / "myblog"

        # Verify blog types were created
        assert (blog_path / "src/types/user.py").exists()
        assert (blog_path / "src/types/post.py").exists()
        assert (blog_path / "src/types/comment.py").exists()

        # Generate migrations for each type
        with patch("fraiseql.cli.commands.generate.get_timestamp") as mock_timestamp:
            mock_timestamp.side_effect = ["001", "002", "003"]

            for entity in ["User", "Post", "Comment"]:
                # Run CLI commands with the blog directory as cwd
                cwd_before = Path.cwd()
                try:
                    os.chdir(str(blog_path))
                    result = cli_runner.invoke(
                        cli,
                        ["generate", "migration", entity],
                    )
                    assert result.exit_code == 0
                finally:
                    os.chdir(cwd_before)

            assert (blog_path / "migrations/001_create_users.sql").exists()
            assert (blog_path / "migrations/002_create_posts.sql").exists()
            assert (blog_path / "migrations/003_create_comments.sql").exists()

    def test_environment_handling(self, cli_runner, temp_project_dir) -> None:
        """Test that CLI respects environment variables."""
        # Create project with custom database URL
        custom_db = "postgresql://custom:pass@remote:5432/customdb"
        result = cli_runner.invoke(
            cli,
            ["init", "envtest", "--database-url", custom_db, "--no-git"],
        )
        assert result.exit_code == 0

        # Verify .env was created correctly
        env_content = (temp_project_dir / "envtest" / ".env").read_text()
        assert f"DATABASE_URL={custom_db}" in env_content
        assert "FRAISEQL_AUTO_CAMEL_CASE=true" in env_content

        os.chdir("envtest")

        # Test that dev command would load the .env file
        with (
            patch("fraiseql.cli.commands.dev.load_dotenv") as mock_load,
            patch("fraiseql.cli.commands.dev.uvicorn"),
        ):
            result = cli_runner.invoke(cli, ["dev"])
            mock_load.assert_called_once()
