"""User-friendly error messages for FraiseQL.

This module provides clear, actionable error messages that help developers
quickly understand and resolve issues.
"""

from typing import Any


class FraiseQLError(Exception):
    """Base exception class for all FraiseQL errors.

    Provides consistent error formatting with helpful suggestions and documentation links.
    """

    def __init__(
        self,
        message: str,
        code: str = "FRAISEQL_ERROR",
        suggestion: str | None = None,
        doc_link: str | None = None,
        context: dict[str, Any] | None = None,
        cause: Exception | None = None,
    ) -> None:
        """Initialize FraiseQL error.

        Args:
            message: The error message
            code: Error code for programmatic handling
            suggestion: Helpful suggestion to resolve the issue
            doc_link: Link to relevant documentation
            context: Additional context information
            cause: The underlying exception that caused this error
        """
        super().__init__(message)
        self.message = message
        self.code = code
        self.suggestion = suggestion
        self.doc_link = doc_link
        self.context = context or {}

        if cause:
            self.__cause__ = cause

    def __str__(self) -> str:
        """Format error message with suggestions and documentation."""
        parts = [self.message]

        if self.suggestion:
            parts.append(f"Suggestion: {self.suggestion}")

        if self.doc_link:
            parts.append(f"See: {self.doc_link}")

        return "\n".join(parts)


class MissingTypeHintError(FraiseQLError):
    """Raised when a field is missing type hints."""

    def __init__(
        self,
        class_name: str,
        field_name: str,
        suggested_type: str | None = None,
    ) -> None:
        """Initialize missing type hint error.

        Args:
            class_name: Name of the class with missing type hint
            field_name: Name of the field missing type hint
            suggested_type: Suggested type to use
        """
        type_hint = suggested_type or "str"
        super().__init__(
            message=f"Field '{field_name}' in class '{class_name}' is missing a type hint.",
            code="MISSING_TYPE_HINT",
            suggestion=f"Add a type annotation like: {field_name}: {type_hint}",
            doc_link="https://docs.fraiseql.com/errors/missing-type-hint",
        )


class MissingDatabaseViewError(FraiseQLError):
    """Raised when a required database view is missing."""

    def __init__(
        self,
        type_name: str,
        expected_view: str,
        custom_view_name: bool = False,
    ) -> None:
        """Initialize missing database view error.

        Args:
            type_name: GraphQL type name
            expected_view: Expected view name
            custom_view_name: Whether a custom view name was specified
        """
        suggestion = f"""Create a view named '{expected_view}' that returns a 'data' JSONB column:

CREATE VIEW {expected_view} AS
SELECT jsonb_build_object(
    'id', id,
    'email', email,
    'name', name
) as data
FROM {type_name.lower()}s;
"""

        super().__init__(
            message=f"Database view '{expected_view}' for type '{type_name}' not found.",
            code="MISSING_DATABASE_VIEW",
            suggestion=suggestion,
            doc_link="https://docs.fraiseql.com/errors/missing-view",
        )


class InvalidFieldTypeError(FraiseQLError):
    """Raised when a field has an unsupported type."""

    def __init__(
        self,
        class_name: str,
        field_name: str,
        field_type: str,
        supported_types: list[str] | None = None,
        conversion_hint: str | None = None,
    ) -> None:
        """Initialize invalid field type error.

        Args:
            class_name: Name of the class
            field_name: Name of the field
            field_type: The invalid type
            supported_types: List of supported types
            conversion_hint: Hint for converting the type
        """
        if conversion_hint:
            suggestion = conversion_hint
        elif supported_types:
            suggestion = f"Use one of the supported types: {', '.join(supported_types)}"
        else:
            suggestion = "Check the documentation for supported field types"

        super().__init__(
            message=(
                f"Field '{field_name}' in class '{class_name}' has unsupported type '{field_type}'."
            ),
            code="INVALID_FIELD_TYPE",
            suggestion=suggestion,
            doc_link="https://docs.fraiseql.com/errors/invalid-field-type",
        )


class SQLGenerationError(FraiseQLError):
    """Raised when SQL generation fails."""

    def __init__(
        self,
        operation: str,
        reason: str,
        query_info: dict[str, Any] | None = None,
        custom_suggestion: str | None = None,
    ) -> None:
        """Initialize SQL generation error.

        Args:
            operation: The operation that failed (e.g., "WHERE clause generation")
            reason: Reason for failure
            query_info: Information about the query
            custom_suggestion: Custom suggestion to resolve the issue
        """
        suggestion = custom_suggestion or "Check the GraphQL query syntax and supported operators"

        context = {}
        if query_info:
            context["query_info"] = query_info

        super().__init__(
            message=f"Failed to generate SQL for {operation}: {reason}",
            code="SQL_GENERATION_ERROR",
            suggestion=suggestion,
            doc_link="https://docs.fraiseql.com/errors/sql-generation",
            context=context,
        )


class MutationNotFoundError(FraiseQLError):
    """Raised when a mutation's PostgreSQL function is not found."""

    def __init__(
        self,
        mutation_name: str,
        function_name: str,
        available_functions: list[str] | None = None,
    ) -> None:
        """Initialize mutation not found error.

        Args:
            mutation_name: GraphQL mutation name
            function_name: Expected PostgreSQL function name
            available_functions: List of available functions in the schema
        """
        suggestion = f"""Create the function in your database:

CREATE FUNCTION {function_name}(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Your mutation logic here
    RETURN jsonb_build_object(
        'type', 'success',
        'message', 'Created successfully'
    );
END;
$$ LANGUAGE plpgsql;
"""

        if available_functions:
            suggestion += f"\n\nAvailable mutations: {', '.join(available_functions)}"

        super().__init__(
            message=(
                f"PostgreSQL function '{function_name}' for mutation '{mutation_name}' not found."
            ),
            code="MUTATION_NOT_FOUND",
            suggestion=suggestion,
            doc_link="https://docs.fraiseql.com/errors/missing-mutation",
        )
