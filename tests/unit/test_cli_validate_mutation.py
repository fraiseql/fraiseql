"""Unit tests for the validate-mutation-return CLI command."""

import json
from pathlib import Path

import pytest
from click.testing import CliRunner

from fraiseql.cli.main import cli

pytestmark = pytest.mark.unit

SCHEMA_SDL = """\
type Query {
    _dummy: String
}

type Mutation {
    createUser(name: String!, email: String!): CreateUserResult!
}

type CreateUserSuccess {
    status: String!
    message: String
    id: String
}

type CreateUserError {
    status: String!
    message: String
    code: Int!
}

union CreateUserResult = CreateUserSuccess | CreateUserError
"""


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture
def schema_file(tmp_path: Path) -> Path:
    p = tmp_path / "schema.graphql"
    p.write_text(SCHEMA_SDL)
    return p


@pytest.fixture
def valid_response(tmp_path: Path) -> Path:
    p = tmp_path / "valid.json"
    p.write_text(json.dumps({"status": "success", "message": "Created", "id": "42"}))
    return p


@pytest.fixture
def invalid_response(tmp_path: Path) -> Path:
    p = tmp_path / "invalid.json"
    p.write_text(json.dumps({"message": "oops"}))
    return p


class TestValidateMutationReturnCLI:
    """Tests for the validate-mutation-return CLI command."""

    def test_valid_response_exits_0(
        self, runner: CliRunner, schema_file: Path, valid_response: Path
    ) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--response-file",
                str(valid_response),
            ],
        )
        assert result.exit_code == 0
        assert "PASS" in result.output

    def test_invalid_response_exits_1(
        self, runner: CliRunner, schema_file: Path, invalid_response: Path
    ) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--response-file",
                str(invalid_response),
            ],
        )
        assert result.exit_code == 1
        assert "FAIL" in result.output

    def test_multiple_files_via_args(
        self,
        runner: CliRunner,
        schema_file: Path,
        valid_response: Path,
        invalid_response: Path,
    ) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                str(valid_response),
                str(invalid_response),
            ],
        )
        assert result.exit_code == 1
        assert "PASS" in result.output
        assert "FAIL" in result.output
        assert "1/2 passed" in result.output

    def test_json_format(self, runner: CliRunner, schema_file: Path, valid_response: Path) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--format",
                "json",
                "--response-file",
                str(valid_response),
            ],
        )
        assert result.exit_code == 0
        data = json.loads(result.output)
        assert len(data) == 1
        assert data[0]["valid"] is True

    def test_junit_format(self, runner: CliRunner, schema_file: Path, valid_response: Path) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--format",
                "junit",
                "--response-file",
                str(valid_response),
            ],
        )
        assert result.exit_code == 0
        assert "<testsuite" in result.output
        assert 'failures="0"' in result.output

    def test_junit_format_with_failure(
        self, runner: CliRunner, schema_file: Path, invalid_response: Path
    ) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--format",
                "junit",
                "--response-file",
                str(invalid_response),
            ],
        )
        assert result.exit_code == 1
        assert "<failure" in result.output

    def test_no_files_exits_1(self, runner: CliRunner, schema_file: Path) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
            ],
        )
        assert result.exit_code == 1
        assert "No response files" in result.output

    def test_invalid_json_file(self, runner: CliRunner, schema_file: Path, tmp_path: Path) -> None:
        bad = tmp_path / "bad.json"
        bad.write_text("not json {{{")
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "createUser",
                "--response-file",
                str(bad),
            ],
        )
        assert result.exit_code == 1
        assert "Invalid JSON" in result.output

    def test_nonexistent_mutation(
        self, runner: CliRunner, schema_file: Path, valid_response: Path
    ) -> None:
        result = runner.invoke(
            cli,
            [
                "validate-mutation-return",
                "--schema",
                str(schema_file),
                "--mutation",
                "nonExistent",
                "--response-file",
                str(valid_response),
            ],
        )
        assert result.exit_code == 1
        assert "not found" in result.output


class TestLibraryImport:
    """Test that validate_mutation_return is importable from the public API."""

    def test_importable_from_fraiseql(self) -> None:
        from fraiseql import validate_mutation_return as vmr

        assert callable(vmr)
