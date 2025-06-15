"""Integration tests for CLI commands working together."""

from pathlib import Path
from unittest.mock import patch

from fraiseql.cli.main import cli


class TestCLIIntegration:
    """Test CLI commands working together in realistic scenarios."""

    def test_full_project_workflow(self, cli_runner, temp_project_dir):
        """Test creating and setting up a complete project."""
        import os

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

    def test_blog_template_workflow(self, cli_runner, temp_project_dir):
        """Test blog template project setup."""
        import os

        # Create blog project
        result = cli_runner.invoke(cli, ["init", "myblog", "--template", "blog", "--no-git"])
        assert result.exit_code == 0

        os.chdir("myblog")

        # Verify blog types were created
        assert Path("src/types/user.py").exists()
        assert Path("src/types/post.py").exists()
        assert Path("src/types/comment.py").exists()

        # Generate migrations for each type
        with patch("fraiseql.cli.commands.generate.get_timestamp") as mock_timestamp:
            mock_timestamp.side_effect = ["001", "002", "003"]

            for entity in ["User", "Post", "Comment"]:
                result = cli_runner.invoke(cli, ["generate", "migration", entity])
                assert result.exit_code == 0

            assert Path("migrations/001_create_users.sql").exists()
            assert Path("migrations/002_create_posts.sql").exists()
            assert Path("migrations/003_create_comments.sql").exists()

    @pytest.mark.skip(reason="TestFoundry extension not yet implemented")
    @patch("os.getenv", return_value="postgresql://test/db")
    def test_testfoundry_workflow(self, mock_getenv, cli_runner, temp_project_dir):
        """Test TestFoundry integration workflow."""
        import os
        from unittest.mock import AsyncMock

        # Create project first
        result = cli_runner.invoke(cli, ["init", "testproject", "--no-git"])
        assert result.exit_code == 0

        os.chdir("testproject")

        # Mock TestFoundry components
        with patch("fraiseql.cqrs.CQRSRepository") as mock_repo:
            mock_repo.return_value.close = AsyncMock()

            with patch("fraiseql.extensions.testfoundry.FoundrySetup") as mock_setup:
                mock_setup.return_value.install = AsyncMock()

                # Install TestFoundry
                result = cli_runner.invoke(cli, ["testfoundry", "install"])
                assert result.exit_code == 0
                assert "TestFoundry installed successfully!" in result.output

        # Generate tests (mocked)
        with patch("fraiseql.cqrs.CQRSRepository") as mock_repo:
            mock_repo.return_value.close = AsyncMock()

            with patch("fraiseql.extensions.testfoundry.FoundryGenerator") as mock_gen:
                mock_gen.return_value.generate_tests_for_entity = AsyncMock(
                    return_value={"happy_path": "-- test"}
                )
                mock_gen.return_value.write_tests_to_files = AsyncMock()

                result = cli_runner.invoke(cli, ["testfoundry", "generate", "User"])
                assert result.exit_code == 0
                assert "Tests generated" in result.output

    def test_environment_handling(self, cli_runner, temp_project_dir):
        """Test that CLI respects environment variables."""
        import os

        # Create project with custom database URL
        custom_db = "postgresql://custom:pass@remote:5432/customdb"
        result = cli_runner.invoke(
            cli, ["init", "envtest", "--database-url", custom_db, "--no-git"]
        )
        assert result.exit_code == 0

        # Verify .env was created correctly
        env_content = (temp_project_dir / "envtest" / ".env").read_text()
        assert f"DATABASE_URL={custom_db}" in env_content
        assert "FRAISEQL_AUTO_CAMEL_CASE=true" in env_content

        os.chdir("envtest")

        # Test that dev command would load the .env file
        with patch("fraiseql.cli.commands.dev.load_dotenv") as mock_load:
            with patch("fraiseql.cli.commands.dev.uvicorn"):
                result = cli_runner.invoke(cli, ["dev"])
                mock_load.assert_called_once()
