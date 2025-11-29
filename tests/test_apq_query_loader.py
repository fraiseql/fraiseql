"""Tests for APQ query loader from .graphql files."""

import tempfile
from pathlib import Path

import pytest

pytestmark = pytest.mark.integration


class TestLoadQueriesFromDirectory:
    """Tests for loading queries from .graphql files."""

    def test_load_queries_from_directory_function_exists(self) -> None:
        """Test that load_queries_from_directory is importable."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        assert callable(load_queries_from_directory)

    def test_load_single_graphql_file(self) -> None:
        """Test loading a single .graphql file."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            # Create a .graphql file
            graphql_file = Path(tmpdir) / "queries.graphql"
            graphql_file.write_text("query GetUsers { users { id name } }")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 1
            assert "query GetUsers { users { id name } }" in queries

    def test_load_multiple_graphql_files(self) -> None:
        """Test loading multiple .graphql files."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            # Create multiple .graphql files
            (Path(tmpdir) / "users.graphql").write_text("query GetUsers { users { id } }")
            (Path(tmpdir) / "posts.graphql").write_text("query GetPosts { posts { title } }")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 2
            assert "query GetUsers { users { id } }" in queries
            assert "query GetPosts { posts { title } }" in queries

    def test_load_multiple_queries_in_single_file(self) -> None:
        """Test loading multiple queries from a single file."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            graphql_file = Path(tmpdir) / "operations.graphql"
            graphql_file.write_text("""
query GetUsers {
    users { id name }
}

query GetUser($id: ID!) {
    user(id: $id) { id name email }
}

mutation CreateUser($input: CreateUserInput!) {
    createUser(input: $input) { id }
}
""")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 3

    def test_load_nested_directories(self) -> None:
        """Test loading from nested directories."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            # Create nested structure
            users_dir = Path(tmpdir) / "users"
            users_dir.mkdir()
            posts_dir = Path(tmpdir) / "posts"
            posts_dir.mkdir()

            (users_dir / "queries.graphql").write_text("query GetUsers { users { id } }")
            (posts_dir / "queries.graphql").write_text("query GetPosts { posts { id } }")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 2

    def test_load_empty_directory(self) -> None:
        """Test loading from an empty directory."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            queries = load_queries_from_directory(tmpdir)

            assert queries == []

    def test_load_ignores_non_graphql_files(self) -> None:
        """Test that non-.graphql files are ignored."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            (Path(tmpdir) / "queries.graphql").write_text("query A { a }")
            (Path(tmpdir) / "readme.md").write_text("# Queries")
            (Path(tmpdir) / "schema.json").write_text("{}")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 1

    def test_load_nonexistent_directory_raises_error(self) -> None:
        """Test loading from nonexistent directory raises FileNotFoundError."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with pytest.raises(FileNotFoundError):
            load_queries_from_directory("/nonexistent/path/to/queries")

    def test_load_with_gql_extension(self) -> None:
        """Test loading .gql files as well as .graphql."""
        from fraiseql.storage.query_loader import load_queries_from_directory

        with tempfile.TemporaryDirectory() as tmpdir:
            (Path(tmpdir) / "queries.graphql").write_text("query A { a }")
            (Path(tmpdir) / "mutations.gql").write_text("mutation B { b }")

            queries = load_queries_from_directory(tmpdir)

            assert len(queries) == 2


class TestAPQQueriesDirConfig:
    """Tests for apq_queries_dir config option."""

    def test_apq_queries_dir_config_exists(self) -> None:
        """Test that apq_queries_dir config option exists."""
        from fraiseql.fastapi.config import FraiseQLConfig

        config = FraiseQLConfig(database_url="postgresql://localhost/test")

        # Should have apq_queries_dir attribute (None by default)
        assert hasattr(config, "apq_queries_dir")
        assert config.apq_queries_dir is None

    def test_apq_queries_dir_can_be_set(self) -> None:
        """Test that apq_queries_dir can be set."""
        from fraiseql.fastapi.config import FraiseQLConfig

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            apq_queries_dir="./graphql/",
        )

        assert config.apq_queries_dir == "./graphql/"

    def test_apq_queries_dir_accepts_string_path(self) -> None:
        """Test that apq_queries_dir accepts string paths."""
        from fraiseql.fastapi.config import FraiseQLConfig

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            apq_queries_dir="./graphql/queries",
        )

        assert config.apq_queries_dir == "./graphql/queries"
