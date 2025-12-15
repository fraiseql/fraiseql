"""Common base types for mutations and other GraphQL operations."""

from .errors import Error
from .fraise_type import fraise_type


@fraise_type
class MutationResultBase:
    """Optional base type for GraphQL mutation results.

    This type provides a standardized structure for mutation responses, including
    common fields that most mutations need. It can be inherited by both success
    and error response types to ensure consistency.

    **NOTE: As of v1.8.1, this base class is OPTIONAL.** FraiseQL now automatically
    populates `errors` arrays on ALL error responses, so you don't need to inherit
    from this class to get structured errors. This class remains available for
    backward compatibility and convenience.

    Fields:
        status: The status of the mutation (e.g., "success", "noop:already_exists")
        message: Human-readable description of the result
        errors: List of structured errors (auto-populated by FraiseQL when applicable)

    Example Usage (Optional - for backward compatibility):
        @fraiseql.success
        class CreateUserSuccess(MutationResultBase):
            user: User | None = None

        @fraiseql.error
        class CreateUserError(MutationResultBase):
            conflict_user: User | None = None

    Alternative (Recommended for new code):
        @fraiseql.success
        class CreateUserSuccess:
            status: str = "success"
            message: str | None = None
            errors: list[Error] | None = None  # Auto-populated
            user: User | None = None

        @fraiseql.error
        class CreateUserError:
            status: str
            message: str
            errors: list[Error]  # Always populated automatically
            conflict_user: User | None = None
    """

    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None
