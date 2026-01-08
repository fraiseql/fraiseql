"""Core type definitions for FraiseQL."""

import json
import logging
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class FieldDefinition:
    """Base field definition."""

    name: str
    resolver: Callable
    return_type: type
    args: dict[str, Any]
    description: str | None = None


@dataclass
class QueryField(FieldDefinition):
    """Query field definition."""


@dataclass
class MutationField(FieldDefinition):
    """Mutation field definition."""


@dataclass
class SubscriptionField(FieldDefinition):
    """Subscription field definition."""


class RustResponseBytes:
    """Marker for pre-serialized response bytes from Rust.

    FastAPI detects this type and sends bytes directly without any
    Python serialization or string operations.

    This class supports optional schema_type tracking for debugging and
    provides a to_json() method for testing purposes (not recommended for
    production due to performance overhead).

    WORKAROUND: Fixes known Rust bug where closing brace is missing for
    data object when query has nested objects. This is a temporary fix
    until fraiseql-rs is updated.

    Args:
        data: Pre-serialized JSON bytes from Rust
        schema_type: Optional GraphQL schema type name for debugging (e.g., "Product", "User")

    Examples:
        >>> # Basic usage (existing code - backwards compatible)
        >>> response = RustResponseBytes(b'{"data":{"hello":"world"}}')
        >>> bytes(response)
        b'{"data":{"hello":"world"}}'

        >>> # With schema type tracking (Phase 3 enhancement)
        >>> response = RustResponseBytes(b'{"data":{"products":[]}}', schema_type="Product")
        >>> response.schema_type
        'Product'

        >>> # Testing with to_json() (Phase 3 - for tests only!)
        >>> response.to_json()
        {'data': {'products': []}}
    """

    __slots__ = ("_data", "_fixed", "_schema_type", "content_type")

    def __init__(self, data: bytes, schema_type: str | None = None) -> None:
        self._data = data
        self.content_type = "application/json"
        self._fixed = False
        self._schema_type = schema_type

    @property
    def bytes(self) -> bytes:
        """Backward compatibility property for accessing the data."""
        return self._data

    @property
    def schema_type(self) -> str | None:
        """Get the GraphQL schema type name for this response.

        This property is useful for debugging and understanding what type
        the RustResponseBytes represents. For example, if this response
        contains a list of Product objects, schema_type would be "Product".

        Returns:
            The GraphQL schema type name, or None if not set

        Examples:
            >>> response = RustResponseBytes(b'{"data":{"products":[]}}', schema_type="Product")
            >>> response.schema_type
            'Product'

            >>> response = RustResponseBytes(b'{"data":{}}')
            >>> response.schema_type is None
            True
        """
        return self._schema_type

    def to_json(self) -> dict:
        """Parse the response bytes as JSON and return as dict.

        ⚠️ WARNING: This method is intended for TESTING ONLY!

        In production, RustResponseBytes should be sent directly to the client
        via __bytes__() without any parsing. This method defeats the purpose
        of the zero-copy architecture and should only be used in test code
        for assertions.

        Returns:
            Parsed JSON as a Python dict

        Raises:
            json.JSONDecodeError: If the bytes don't contain valid JSON

        Examples:
            >>> response = RustResponseBytes(b'{"data":{"hello":"world"}}')
            >>> response.to_json()
            {'data': {'hello': 'world'}}

            >>> # In tests, you can use this to verify structure
            >>> data = response.to_json()
            >>> assert data["data"]["hello"] == "world"

            >>> # But DON'T use this in production - use __bytes__() instead!
            >>> bytes(response)  # ✅ Good - zero-copy
            b'{"data":{"hello":"world"}}'
        """
        # Use __bytes__() to get the (potentially fixed) bytes
        data_bytes = self.__bytes__()
        return json.loads(data_bytes)

    def __bytes__(self) -> bytes:
        """Return the response data as bytes, fixing JSON if necessary."""
        # Workaround for Rust bug: Check if JSON is missing closing brace
        if not self._fixed:
            try:
                # Try to parse the JSON
                json_str = self._data.decode("utf-8")  # type: ignore[union-attr]
                json.loads(json_str)
                # If it parses, no fix needed
                self._fixed = True
            except json.JSONDecodeError as e:
                # Check if it's the known "missing closing brace" bug
                if "Expecting ',' delimiter" in str(e) and e.pos >= len(json_str) - 2:
                    # Count braces to confirm
                    open_braces = json_str.count("{")
                    close_braces = json_str.count("}")

                    if open_braces > close_braces:
                        # Missing closing brace(s) - add them
                        missing_braces = open_braces - close_braces
                        fixed_json = json_str + ("}" * missing_braces)  # type: ignore[operator]

                        # Verify the fix works
                        try:
                            json.loads(fixed_json)
                            logger.warning(
                                f"Applied workaround for Rust JSON bug: "
                                f"Added {missing_braces} missing closing brace(s). "
                                f"This bug affects queries with nested objects. "
                                f"Update fraiseql-rs to fix permanently.",
                            )
                            self._data = fixed_json.encode("utf-8")
                            self._fixed = True
                        except json.JSONDecodeError:
                            # Fix didn't work, return original
                            logger.exception(
                                "Rust JSON workaround failed - returning original malformed JSON",
                            )
                    else:
                        # Different JSON error, return original
                        pass
                else:
                    # Different JSON error, return original
                    pass

        return self._data
