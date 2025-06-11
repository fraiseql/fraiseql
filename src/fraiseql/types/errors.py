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
