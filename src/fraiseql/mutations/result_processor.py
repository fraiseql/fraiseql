"""Clean, immutable mutation result processor."""

from dataclasses import dataclass
from typing import Any, Optional, Type, TypeVar

from .types import MutationResult

T = TypeVar("T")


@dataclass(frozen=True)  # Immutable
class ErrorDetail:
    """Structured error detail."""

    code: int
    identifier: str
    message: str
    details: dict[str, Any]


@dataclass(frozen=True)  # Immutable
class ProcessedResult:
    """Immutable processed mutation result."""

    typename: str
    status: str
    message: str
    errors: list[ErrorDetail]
    data: Optional[dict[str, Any]] = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to JSON-serializable dictionary."""
        result = {
            "__typename": self.typename,
            "status": self.status,
            "message": self.message,
            "errors": [
                {
                    "code": error.code,
                    "identifier": error.identifier,
                    "message": error.message,
                    "details": error.details,
                }
                for error in self.errors
            ],
        }

        if self.data:
            result.update(self.data)

        return result


class MutationResultProcessor:
    """Clean, predictable mutation result processor."""

    def process_error(self, db_result: MutationResult, error_class: Type[T]) -> ProcessedResult:
        """Process database result into error response."""
        error_detail = self._create_error_detail(db_result)

        return ProcessedResult(
            typename=error_class.__name__,
            status=db_result.status,
            message=db_result.message,
            errors=[error_detail],
            data=self._extract_error_data(db_result, error_class),
        )

    def process_success(self, db_result: MutationResult, success_class: Type[T]) -> ProcessedResult:
        """Process database result into success response."""
        return ProcessedResult(
            typename=success_class.__name__,
            status=db_result.status,
            message=db_result.message,
            errors=[],  # Empty list, never None
            data=self._extract_success_data(db_result, success_class),
        )

    def _create_error_detail(self, db_result: MutationResult) -> ErrorDetail:
        """Create structured error detail from database result."""
        status = db_result.status or "unknown"

        if ":" in status:
            prefix, identifier = status.split(":", 1)
            if prefix in ("noop", "blocked"):
                code = 422  # Unprocessable Entity
            elif prefix == "failed":
                code = 500  # Internal Server Error
            else:
                code = 500  # Default to server error
        else:
            code = 500
            identifier = "general_error"

        details = {}
        if db_result.extra_metadata:
            details.update(db_result.extra_metadata)

        return ErrorDetail(
            code=code,
            identifier=identifier,
            message=db_result.message or f"Operation failed: {status}",
            details=details,
        )

    def _extract_error_data(
        self, db_result: MutationResult, error_class: Type[T]
    ) -> Optional[dict[str, Any]]:
        """Extract error-specific data from database result."""
        # Start with base error class fields
        data = {}

        # Add message if error class has message field
        if hasattr(error_class, "__annotations__") and "message" in error_class.__annotations__:
            data["message"] = db_result.message

        # Add error_code if error class has error_code field
        if hasattr(error_class, "__annotations__") and "error_code" in error_class.__annotations__:
            # Extract error code from status
            if db_result.status and ":" in db_result.status:
                _, identifier = db_result.status.split(":", 1)
                data["error_code"] = identifier.upper()
            else:
                data["error_code"] = "GENERAL_ERROR"

        # Add any object_data if it exists
        if db_result.object_data:
            data.update(db_result.object_data)

        return data if data else None

    def _extract_success_data(
        self, db_result: MutationResult, success_class: Type[T]
    ) -> Optional[dict[str, Any]]:
        """Extract success-specific data from database result."""
        data = {}

        # Add message if success class has message field
        if hasattr(success_class, "__annotations__") and "message" in success_class.__annotations__:
            data["message"] = db_result.message

        # Add any object_data if it exists
        if db_result.object_data:
            data.update(db_result.object_data)

        return data if data else None
