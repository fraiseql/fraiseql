from dataclasses import dataclass
from unittest.mock import AsyncMock

import psycopg
import pytest

from fraiseql.mutations.executor import parse_mutation_result, run_fraiseql_mutation
from fraiseql.types.errors import Error

pytestmark = pytest.mark.integration


@pytest.fixture
def mock_repository() -> None:
    """Create a mock repository for testing."""
    repo = AsyncMock()
    repo.run = AsyncMock()
    return repo


@pytest.fixture
def sample_input_type() -> None:
    """Create a sample input type for testing."""

    @dataclass
    class UpdateUserInput:
        email: str | None = None
        name: str | None = None

    return UpdateUserInput


@pytest.fixture
def sample_result_types() -> None:
    """Create sample result types for testing."""

    @dataclass
    class UpdateUserSuccess:
        status: str
        message: str
        data: dict | None = None

    @dataclass
    class UpdateUserError:
        status: str
        message: str
        errors: list[Error]

    return UpdateUserSuccess, UpdateUserError


class TestParseMutationResult:
    """Test suite for parse_mutation_result function."""

    def test_parse_success_result(self, sample_result_types) -> None:
        """Test parsing a successful mutation result."""
        success_cls, error_cls = sample_result_types

        result_row = {
            "status": "ok",
            "message": "User updated successfully",
            "data": {"id": "123", "email": "test@example.com"},
        }

        custom_status_map = {"ok": ("success", 200)}

        result = parse_mutation_result(
            result_row=result_row,
            result_cls=success_cls,
            error_cls=error_cls,
            custom_status_map=custom_status_map,
        )

        assert isinstance(result, success_cls)
        assert result.status == "ok"
        assert result.message == "User updated successfully"
        assert result.data == {"id": "123", "email": "test@example.com"}

    def test_parse_error_result(self, sample_result_types) -> None:
        """Test parsing an error mutation result."""
        success_cls, error_cls = sample_result_types

        result_row = {
            "status": "failed:validation",
            "message": "Validation failed",
            "errors": [{"message": "Invalid email", "code": 422, "identifier": "invalid_email"}],
        }

        custom_status_map = {"failed:validation": ("error", 422)}

        result = parse_mutation_result(
            result_row=result_row,
            result_cls=success_cls,
            error_cls=error_cls,
            custom_status_map=custom_status_map,
        )

        assert isinstance(result, error_cls)
        assert result.status == "failed:validation"
        assert result.message == "Validation failed"
        assert len(result.errors) == 1
        assert result.errors[0].message == "Invalid email"
        assert result.errors[0].code == 422

    def test_parse_unknown_status(self, sample_result_types) -> None:
        """Test parsing result with unknown status."""
        success_cls, error_cls = sample_result_types

        result_row = {"status": "weird_status", "message": "Something happened", "errors": None}

        custom_status_map = {}

        result = parse_mutation_result(
            result_row=result_row,
            result_cls=success_cls,
            error_cls=error_cls,
            custom_status_map=custom_status_map,
        )

        assert isinstance(result, error_cls)
        assert result.status == "failed:unknown_status"
        assert result.message == "Something happened"
        assert result.errors == []

    def test_parse_result_without_message(self, sample_result_types) -> None:
        """Test parsing result without message field."""
        success_cls, error_cls = sample_result_types

        result_row = {"status": "failed:error"}

        custom_status_map = {}

        result = parse_mutation_result(
            result_row=result_row,
            result_cls=success_cls,
            error_cls=error_cls,
            custom_status_map=custom_status_map,
        )

        assert isinstance(result, error_cls)
        assert result.message == "No message provided."


class TestRunFraiseQLMutation:
    """Test suite for run_fraiseql_mutation function."""

    @pytest.mark.asyncio
    async def test_successful_mutation(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test running a successful mutation."""
        success_cls, error_cls = sample_result_types

        input_obj = sample_input_type(email="new@example.com", name="Test User")

        mock_repository.run.return_value = [
            {"status": "ok", "message": "User updated", "data": {"id": "123"}}
        ]

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={"ok": ("success", 200)},
            fallback_error_identifier="update_user_error",
            context={},
        )

        assert isinstance(result, success_cls)
        assert result.status == "ok"
        assert mock_repository.run.called

    @pytest.mark.asyncio
    async def test_mutation_with_empty_input(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test mutation with empty input (no fields to update)."""
        from fraiseql.types.definitions import UNSET

        success_cls, error_cls = sample_result_types

        # Create a custom input class that uses UNSET as default
        @dataclass
        class EmptyUserInput:
            email: str | None = UNSET
            name: str | None = UNSET

        input_obj = EmptyUserInput()  # No fields set - all default to UNSET

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={},
            fallback_error_identifier="update_user_error",
            context={},
            noop_message="Nothing to update",
        )

        assert isinstance(result, error_cls)
        assert result.status == "noop"
        assert result.message == "Nothing to update"
        assert len(result.errors) == 1
        assert result.errors[0].code == 422
        assert not mock_repository.run.called

    @pytest.mark.asyncio
    async def test_mutation_database_error(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test mutation with database error."""
        success_cls, error_cls = sample_result_types

        input_obj = sample_input_type(email="test@example.com")

        mock_repository.run.side_effect = psycopg.Error("Connection lost")

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={},
            fallback_error_identifier="db_error",
            context={},
        )

        assert isinstance(result, error_cls)
        assert result.status == "failed:exception"
        assert result.message == "Unhandled database exception occurred."
        assert len(result.errors) == 1
        assert "psycopg.Error: Connection lost" in result.errors[0].message
        assert result.errors[0].identifier == "db_error"

    @pytest.mark.asyncio
    async def test_mutation_unexpected_error(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test mutation with unexpected error."""
        success_cls, error_cls = sample_result_types

        input_obj = sample_input_type(email="test@example.com")

        mock_repository.run.side_effect = ValueError("Invalid value")

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={},
            fallback_error_identifier="unexpected_error",
            context={},
        )

        assert isinstance(result, error_cls)
        assert result.status == "failed:exception"
        assert result.message == "An unexpected error occurred."
        assert len(result.errors) == 1
        assert "ValueError: Invalid value" in result.errors[0].message

    @pytest.mark.asyncio
    async def test_mutation_no_result(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test mutation that returns no result."""
        success_cls, error_cls = sample_result_types

        input_obj = sample_input_type(email="test@example.com")

        mock_repository.run.return_value = []

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={},
            fallback_error_identifier="update_user_error",
            context={},
        )

        assert isinstance(result, error_cls)
        assert result.status == "failed:no_result"
        assert result.message == "No result returned from mutation."

    @pytest.mark.asyncio
    async def test_mutation_with_data_response(
        self, mock_repository, sample_input_type, sample_result_types
    ) -> None:
        """Test mutation that returns data in response."""
        success_cls, error_cls = sample_result_types

        input_obj = sample_input_type(email="test@example.com")

        mock_repository.run.return_value = [
            {"status": "ok", "message": "Updated", "data": {"email": "test@example.com"}}
        ]

        result = await run_fraiseql_mutation(
            input_payload=input_obj,
            sql_function_name="update_user",
            repository=mock_repository,
            result_cls=success_cls,
            error_cls=error_cls,
            status_map={"ok": ("success", 200)},
            fallback_error_identifier="update_user_error",
            context={},
        )

        assert isinstance(result, success_cls)
