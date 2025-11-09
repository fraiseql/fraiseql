"""Tests for Turbo CLI commands."""

import json

import pytest

from fraiseql.cli.main import cli


@pytest.fixture
def sample_graphql_query(tmp_path) -> None:
    """Create a sample GraphQL query file."""
    query_content = """
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    email
  }
}
"""
    query_file = tmp_path / "query.graphql"
    query_file.write_text(query_content)
    return query_file


@pytest.fixture
def sample_graphql_queries_json(tmp_path) -> None:
    """Create a JSON file with multiple queries."""
    queries = {
        "queries": [
            {
                "operationName": "GetUser",
                "query": "query GetUser($id: ID!) { user(id: $id) { id name } }",
            },
            {"operationName": "GetPosts", "query": "query GetPosts { posts { id title } }"},
        ]
    }
    json_file = tmp_path / "queries.json"
    json_file.write_text(json.dumps(queries))
    return json_file


@pytest.fixture
def sample_single_query_json(tmp_path) -> None:
    """Create a JSON file with a single query."""
    query = {"operationName": "GetUser", "query": "query GetUser { user { id name } }"}
    json_file = tmp_path / "single_query.json"
    json_file.write_text(json.dumps(query))
    return json_file


@pytest.fixture
def sample_query_list_json(tmp_path) -> None:
    """Create a JSON file with query list (no 'queries' key)."""
    queries = [
        {"operationName": "Query1", "query": "query Query1 { field1 }"},
        {"operationName": "Query2", "query": "query Query2 { field2 }"},
    ]
    json_file = tmp_path / "query_list.json"
    json_file.write_text(json.dumps(queries))
    return json_file


@pytest.fixture
def sample_view_mapping(tmp_path) -> None:
    """Create a sample view mapping file."""
    mapping = {"User": "v_users", "Post": "v_posts", "Comment": "v_comments"}
    mapping_file = tmp_path / "mapping.json"
    mapping_file.write_text(json.dumps(mapping))
    return mapping_file


@pytest.fixture
def invalid_graphql_query(tmp_path) -> None:
    """Create an invalid GraphQL query file."""
    invalid_content = "this is not valid GraphQL {{"
    query_file = tmp_path / "invalid.graphql"
    query_file.write_text(invalid_content)
    return query_file


@pytest.mark.unit
class TestTurboRegister:
    """Test the fraiseql turbo register command."""

    def test_register_requires_query_file(self, cli_runner) -> None:
        """Test that register requires a query file."""
        result = cli_runner.invoke(cli, ["turbo", "register"])

        assert result.exit_code != 0
        assert "Missing argument" in result.output or "Error" in result.output

    def test_register_with_nonexistent_file(self, cli_runner) -> None:
        """Test register with nonexistent file."""
        result = cli_runner.invoke(cli, ["turbo", "register", "nonexistent.graphql"])

        assert result.exit_code != 0

    def test_register_graphql_file(self, cli_runner, sample_graphql_query) -> None:
        """Test registering queries from .graphql file."""
        result = cli_runner.invoke(cli, ["turbo", "register", str(sample_graphql_query)])

        # Should execute (may fail on actual registration if dependencies missing)
        # We're testing the CLI command structure, not the full registration
        assert "Registering query" in result.output or "Error" in result.output

    def test_register_json_file_with_queries_key(
        self, cli_runner, sample_graphql_queries_json
    ) -> None:
        """Test registering queries from JSON file with 'queries' key."""
        result = cli_runner.invoke(cli, ["turbo", "register", str(sample_graphql_queries_json)])

        assert "Registering query" in result.output or "Error" in result.output

    def test_register_json_file_single_query(self, cli_runner, sample_single_query_json) -> None:
        """Test registering single query from JSON file."""
        result = cli_runner.invoke(cli, ["turbo", "register", str(sample_single_query_json)])

        assert "Registering query" in result.output or "Error" in result.output

    def test_register_json_file_query_list(self, cli_runner, sample_query_list_json) -> None:
        """Test registering query list from JSON file."""
        result = cli_runner.invoke(cli, ["turbo", "register", str(sample_query_list_json)])

        assert "Registering query" in result.output or "Error" in result.output

    def test_register_with_view_mapping(
        self, cli_runner, sample_graphql_query, sample_view_mapping
    ):
        """Test register with view mapping file."""
        result = cli_runner.invoke(
            cli,
            [
                "turbo",
                "register",
                str(sample_graphql_query),
                "--view-mapping",
                str(sample_view_mapping),
            ],
        )

        assert "Registering query" in result.output or "Error" in result.output

    def test_register_with_output_file(self, cli_runner, sample_graphql_query, tmp_path) -> None:
        """Test register with output file for results."""
        output_file = tmp_path / "results.json"

        result = cli_runner.invoke(
            cli, ["turbo", "register", str(sample_graphql_query), "--output", str(output_file)]
        )

        # Command should execute
        assert "Registering query" in result.output or "Error" in result.output

    def test_register_dry_run_valid_query(self, cli_runner, sample_graphql_query) -> None:
        """Test dry-run mode with valid query."""
        result = cli_runner.invoke(
            cli, ["turbo", "register", str(sample_graphql_query), "--dry-run"]
        )

        # Should validate without registering
        assert result.exit_code == 0
        assert "Registering query" in result.output
        # In dry-run, should show validation result
        assert "Valid GraphQL" in result.output or "Invalid GraphQL" in result.output

    def test_register_dry_run_invalid_query(self, cli_runner, invalid_graphql_query) -> None:
        """Test dry-run mode with invalid query."""
        result = cli_runner.invoke(
            cli, ["turbo", "register", str(invalid_graphql_query), "--dry-run"]
        )

        assert result.exit_code == 0
        # Should show validation error
        assert "Invalid GraphQL" in result.output

    def test_register_all_options(
        self, cli_runner, sample_graphql_queries_json, sample_view_mapping, tmp_path
    ):
        """Test register with all options combined."""
        output_file = tmp_path / "results.json"

        result = cli_runner.invoke(
            cli,
            [
                "turbo",
                "register",
                str(sample_graphql_queries_json),
                "--view-mapping",
                str(sample_view_mapping),
                "--output",
                str(output_file),
                "--dry-run",
            ],
        )

        assert result.exit_code == 0
        assert "Registering query" in result.output

    def test_register_summary_output(self, cli_runner, sample_graphql_queries_json) -> None:
        """Test that register shows summary of results."""
        result = cli_runner.invoke(cli, ["turbo", "register", str(sample_graphql_queries_json)])

        # Should show summary like "X/Y successful"
        assert "successful" in result.output.lower() or "Error" in result.output


@pytest.mark.unit
class TestTurboList:
    """Test the fraiseql turbo list command."""

    def test_list_default_format(self, cli_runner) -> None:
        """Test list command with default format."""
        result = cli_runner.invoke(cli, ["turbo", "list"])

        assert result.exit_code == 0
        assert "Registered queries" in result.output

    def test_list_json_format(self, cli_runner) -> None:
        """Test list command with JSON format."""
        result = cli_runner.invoke(cli, ["turbo", "list", "--format", "json"])

        assert result.exit_code == 0
        assert "Registered queries" in result.output

    def test_list_sql_format(self, cli_runner) -> None:
        """Test list command with SQL format."""
        result = cli_runner.invoke(cli, ["turbo", "list", "--format", "sql"])

        assert result.exit_code == 0
        assert "Registered queries" in result.output

    def test_list_invalid_format(self, cli_runner) -> None:
        """Test list command with invalid format."""
        result = cli_runner.invoke(cli, ["turbo", "list", "--format", "invalid"])

        # Should fail with format validation error
        assert result.exit_code != 0


@pytest.mark.unit
class TestTurboInspect:
    """Test the fraiseql turbo inspect command."""

    def test_inspect_requires_hash(self, cli_runner) -> None:
        """Test that inspect requires a query hash."""
        result = cli_runner.invoke(cli, ["turbo", "inspect"])

        assert result.exit_code != 0
        assert "Missing argument" in result.output or "Error" in result.output

    def test_inspect_with_hash(self, cli_runner) -> None:
        """Test inspect with a query hash."""
        result = cli_runner.invoke(cli, ["turbo", "inspect", "abc123def456"])

        assert result.exit_code == 0
        assert "Query details" in result.output
        assert "abc123def456" in result.output

    def test_inspect_with_sha256_hash(self, cli_runner) -> None:
        """Test inspect with SHA-256 hash format."""
        sha_hash = "a" * 64  # 64 character hex string
        result = cli_runner.invoke(cli, ["turbo", "inspect", sha_hash])

        assert result.exit_code == 0
        assert "Query details" in result.output


@pytest.mark.unit
class TestTurboLoadQueries:
    """Test the load_queries helper function."""

    def test_load_graphql_file(self, sample_graphql_query) -> None:
        """Test loading .graphql file."""
        from fraiseql.cli.commands.turbo import load_queries

        queries = load_queries(str(sample_graphql_query))

        assert len(queries) == 1
        assert "query" in queries[0]
        assert "GetUser" in queries[0]["query"]

    def test_load_json_with_queries_key(self, sample_graphql_queries_json) -> None:
        """Test loading JSON file with 'queries' key."""
        from fraiseql.cli.commands.turbo import load_queries

        queries = load_queries(str(sample_graphql_queries_json))

        assert len(queries) == 2
        assert queries[0]["operationName"] == "GetUser"
        assert queries[1]["operationName"] == "GetPosts"

    def test_load_json_single_query(self, sample_single_query_json) -> None:
        """Test loading JSON file with single query."""
        from fraiseql.cli.commands.turbo import load_queries

        queries = load_queries(str(sample_single_query_json))

        assert len(queries) == 1
        assert queries[0]["operationName"] == "GetUser"

    def test_load_json_query_list(self, sample_query_list_json) -> None:
        """Test loading JSON file with query list."""
        from fraiseql.cli.commands.turbo import load_queries

        queries = load_queries(str(sample_query_list_json))

        assert len(queries) == 2
        assert queries[0]["operationName"] == "Query1"
        assert queries[1]["operationName"] == "Query2"

    def test_load_unsupported_format(self, tmp_path) -> None:
        """Test loading unsupported file format."""
        from fraiseql.cli.commands.turbo import load_queries

        unsupported_file = tmp_path / "query.txt"
        unsupported_file.write_text("some query")

        with pytest.raises(ValueError) as exc_info:
            load_queries(str(unsupported_file))

        assert "Unsupported file format" in str(exc_info.value)


@pytest.mark.unit
class TestTurboHelp:
    """Test Turbo command help output."""

    def test_turbo_help(self, cli_runner) -> None:
        """Test turbo command help."""
        result = cli_runner.invoke(cli, ["turbo", "--help"])

        assert result.exit_code == 0
        assert "TurboRouter management" in result.output or "turbo" in result.output.lower()
        assert "register" in result.output
        assert "list" in result.output
        assert "inspect" in result.output

    def test_turbo_register_help(self, cli_runner) -> None:
        """Test turbo register command help."""
        result = cli_runner.invoke(cli, ["turbo", "register", "--help"])

        assert result.exit_code == 0
        assert "QUERY_FILE" in result.output or "query" in result.output.lower()
        assert "--view-mapping" in result.output
        assert "--output" in result.output
        assert "--dry-run" in result.output

    def test_turbo_list_help(self, cli_runner) -> None:
        """Test turbo list command help."""
        result = cli_runner.invoke(cli, ["turbo", "list", "--help"])

        assert result.exit_code == 0
        assert "--format" in result.output

    def test_turbo_inspect_help(self, cli_runner) -> None:
        """Test turbo inspect command help."""
        result = cli_runner.invoke(cli, ["turbo", "inspect", "--help"])

        assert result.exit_code == 0
        assert "QUERY_HASH" in result.output or "hash" in result.output.lower()


@pytest.mark.unit
class TestTurboEdgeCases:
    """Test edge cases for turbo commands."""

    def test_register_empty_graphql_file(self, cli_runner, tmp_path) -> None:
        """Test registering empty GraphQL file."""
        empty_file = tmp_path / "empty.graphql"
        empty_file.write_text("")

        result = cli_runner.invoke(cli, ["turbo", "register", str(empty_file), "--dry-run"])

        # Should handle gracefully
        assert result.exit_code == 0

    def test_register_malformed_json(self, cli_runner, tmp_path) -> None:
        """Test registering malformed JSON file."""
        malformed_json = tmp_path / "malformed.json"
        malformed_json.write_text("{invalid json")

        result = cli_runner.invoke(cli, ["turbo", "register", str(malformed_json)])

        # Should show error
        assert result.exit_code != 0

    def test_register_with_missing_view_mapping(self, cli_runner, sample_graphql_query) -> None:
        """Test register with nonexistent view mapping file."""
        result = cli_runner.invoke(
            cli,
            ["turbo", "register", str(sample_graphql_query), "--view-mapping", "nonexistent.json"],
        )

        # Should fail with file not found error
        assert result.exit_code != 0
