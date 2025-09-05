"""Error Module.

This module defines the Error class, which is used to represent error information
in a structured format. The Error class includes attributes for error messages,
codes, identifiers, and additional details, and it implements several magic methods
for comparison, hashing, and string representation.
"""

from typing import Any

from fraiseql.types.fraise_type import fraise_type


@fraise_type
class Error:
    """Represents an error with a message, code, identifier, and optional details.

    Attributes:
        message (str): A human-readable error message.
        code (int): A numeric error code.
        identifier (str): A unique identifier for the error.
        details (JSON | None): Additional details about the error in JSON format.
    """

    message: str
    code: int
    identifier: str
    details: Any | None = None

    def __json__(self) -> dict[str, Any]:
        """Enable JSON serialization for GraphQL responses.

        This method ensures Error objects can be properly serialized in GraphQL
        responses and other JSON contexts, resolving the serialization issue
        described in the error report.

        Returns:
            dict[str, Any]: Dictionary with all error fields for JSON serialization.
        """
        return {
            "message": self.message,
            "code": self.code,
            "identifier": self.identifier,
            "details": self.details,
        }

    def to_dict(self) -> dict[str, Any]:
        """Convert Error to dictionary representation.

        Public API for converting Error objects to dictionaries, primarily
        used for testing and manual serialization scenarios.

        Returns:
            dict[str, Any]: Dictionary representation of the Error.
        """
        return self.__json__()
