import pytest

"""Tests for the generate commands."""

from pathlib import Path
from unittest.mock import patch

from fraiseql.cli.main import cli


@pytest.mark.unit
class TestGenerateSchema:
    """Test the generate schema command."""

    def test_generate_schema_success(self, cli_runner, temp_project_dir) -> None:
        """Test successful schema generation."""
        # Create a real project structure
        Path("src").mkdir()
        (Path("src") / "main.py").write_text(
            '''
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    """A user in the system."""
    id: int = fraise_field(description="User ID")
    name: str = fraise_field(description="User name")
    email: str = fraise_field(description="User email")

@fraiseql.type
class Post:
    """A blog post."""
    id: int = fraise_field(description="Post ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    author: User = fraise_field(description="Post author")

@fraiseql.type
class QueryRoot:
    """Root query type."""
    users: list[User] = fraise_field(default_factory=list, description="List all users")
    posts: list[Post] = fraise_field(default_factory=list, description="List all posts")

    async def resolve_users(self, info) -> None:
        return []

    async def resolve_posts(self, info) -> None:
        return []

app = fraiseql.create_fraiseql_app(queries=[QueryRoot])
''',
        )

        result = cli_runner.invoke(cli, ["generate", "schema"])

        assert result.exit_code == 0
        assert "Generating GraphQL schema" in result.output
        assert "Schema written to schema.graphql" in result.output

        # Check file was written
        assert Path("schema.graphql").exists()
        schema_content = Path("schema.graphql").read_text()

        # Verify the schema contains our types
        assert "type User" in schema_content
        assert "type Post" in schema_content
        assert "author: User" in schema_content

    def test_generate_schema_custom_output(self, cli_runner, temp_project_dir) -> None:
        """Test schema generation with custom output file."""
        # Create simple project
        Path("src").mkdir()
        (Path("src") / "main.py").write_text(
            """
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class Item:
    id: int
    name: str

@fraiseql.type
class QueryRoot:
    items: list[Item] = fraise_field(default_factory=list, description="List all items")

    async def resolve_items(self, info) -> None:
        return []

app = fraiseql.create_fraiseql_app(queries=[QueryRoot])
""",
        )

        result = cli_runner.invoke(cli, ["generate", "schema", "-o", "custom.graphql"])

        if result.exit_code != 0:
            pass
        assert result.exit_code == 0
        assert "Schema written to custom.graphql" in result.output
        assert Path("custom.graphql").exists()

        # Verify content
        assert "type Item" in Path("custom.graphql").read_text()

    def test_generate_schema_no_app(self, cli_runner, temp_project_dir) -> None:
        """Test error when no app is found."""
        Path("src").mkdir()
        (Path("src") / "main.py").write_text("# no app defined")

        result = cli_runner.invoke(cli, ["generate", "schema"])

        assert result.exit_code != 0
        assert "No 'app' found in src/main.py" in result.output

    def test_generate_schema_import_error(self, cli_runner, temp_project_dir) -> None:
        """Test error handling when import fails."""
        Path("src").mkdir()
        (Path("src") / "main.py").write_text(
            """
import nonexistent_module

app = None
""",
        )

        result = cli_runner.invoke(cli, ["generate", "schema"])

        assert result.exit_code != 0
        assert "Error generating schema" in result.output


class TestGenerateMigration:
    """Test the generate migration command."""

    def test_generate_migration_default_table(self, cli_runner, temp_project_dir) -> None:
        """Test migration generation with default table name."""
        # Mock only the timestamp to get predictable filename
        with patch(
            "fraiseql.cli.commands.generate.get_timestamp",
            return_value="20250610120000",
        ):
            result = cli_runner.invoke(cli, ["generate", "migration", "User"])

            assert result.exit_code == 0
            assert "Migration created: migrations/20250610120000_create_users.sql" in result.output

            # Check file exists and content
            migration_file = Path("migrations/20250610120000_create_users.sql")
            assert migration_file.exists()

            content = migration_file.read_text()
            assert "CREATE TABLE IF NOT EXISTS users" in content
            assert "data JSONB NOT NULL" in content
            assert "CREATE INDEX IF NOT EXISTS idx_users_data" in content
            assert "CREATE OR REPLACE VIEW v_users" in content

    def test_generate_migration_custom_table(self, cli_runner, temp_project_dir) -> None:
        """Test migration generation with custom table name."""
        with patch(
            "fraiseql.cli.commands.generate.get_timestamp",
            return_value="20250610120000",
        ):
            result = cli_runner.invoke(
                cli,
                ["generate", "migration", "Post", "--table", "blog_posts"],
            )

            assert result.exit_code == 0

            migration_file = Path("migrations/20250610120000_create_blog_posts.sql")
            content = migration_file.read_text()

            assert "CREATE TABLE IF NOT EXISTS blog_posts" in content
            assert "CREATE INDEX IF NOT EXISTS idx_blog_posts_data" in content
            assert "CREATE OR REPLACE VIEW v_blog_posts" in content

    def test_generate_migration_creates_directory(self, cli_runner, temp_project_dir) -> None:
        """Test that migrations directory is created if missing."""
        assert not Path("migrations").exists()

        with patch(
            "fraiseql.cli.commands.generate.get_timestamp",
            return_value="20250610120000",
        ):
            result = cli_runner.invoke(cli, ["generate", "migration", "User"])

            assert result.exit_code == 0
            assert Path("migrations").exists()


class TestGenerateCrud:
    """Test the generate crud command."""

    def test_generate_crud_creates_mutations(self, cli_runner, temp_project_dir) -> None:
        """Test CRUD generation creates mutation file."""
        result = cli_runner.invoke(cli, ["generate", "crud", "Product"])

        assert result.exit_code == 0
        assert "CRUD mutations created: src/mutations/product_mutations.py" in result.output

        # Check file exists
        mutations_file = Path("src/mutations/product_mutations.py")
        assert mutations_file.exists()

        content = mutations_file.read_text()
        assert "class CreateProductInput:" in content
        assert "class UpdateProductInput:" in content
        assert "class ProductResult:" in content
        assert "@fraiseql.mutation" in content
        assert "async def create_product(" in content
        assert "async def update_product(" in content
        assert "async def delete_product(" in content

    def test_generate_crud_creates_directory(self, cli_runner, temp_project_dir) -> None:
        """Test that mutations directory is created if missing."""
        assert not Path("src/mutations").exists()

        result = cli_runner.invoke(cli, ["generate", "crud", "User"])

        assert result.exit_code == 0
        assert Path("src/mutations").exists()
        assert Path("src/mutations/user_mutations.py").exists()

    def test_generate_crud_handles_case(self, cli_runner, temp_project_dir) -> None:
        """Test that CRUD generation handles different case inputs."""
        result = cli_runner.invoke(cli, ["generate", "crud", "BlogPost"])

        assert result.exit_code == 0

        content = Path("src/mutations/blogpost_mutations.py").read_text()
        assert "async def create_blogpost(" in content
        assert "BlogPostResult" in content
