"""Common base types for mutations and other GraphQL operations."""

from .errors import Error
from .fraise_type import fraise_type


@fraise_type
class MutationResultBase:
    """Base type for GraphQL mutation results.

    This type provides a standardized structure for mutation responses, including
    common fields that most mutations need. It can be inherited by both success
    and error response types to ensure consistency.

    This base type is designed to work with FraiseQL's automatic error population
    feature, making it plug-and-play for projects that follow common patterns.

    Fields:
        status: The status of the mutation (e.g., "success", "noop:already_exists")
        message: Human-readable description of the result
        errors: List of structured errors (auto-populated by FraiseQL when applicable)

    Example Usage:
        @fraiseql.success
        class CreateUserSuccess(MutationResultBase):
            user: User | None = None

        @fraiseql.failure
        class CreateUserError(MutationResultBase):
            conflict_user: User | None = None
    """

    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None
