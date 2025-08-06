"""Special GraphQL type for passthrough results."""

import json
from typing import Any

from fraiseql.core.raw_json_executor import RawJSONResult


class PassthroughResult:
    """A result that should bypass GraphQL type validation.

    This class acts as a placeholder that GraphQL will accept
    but signals that the actual data should be passed through raw.
    """

    def __init__(self, data: Any, field_name: str):
        """Initialize with raw data.

        Args:
            data: The raw data (dict, list, etc.)
            field_name: The GraphQL field name
        """
        self.data = data
        self.field_name = field_name
        self._raw_json = None

    def to_raw_json(self) -> RawJSONResult:
        """Convert to RawJSONResult."""
        if self._raw_json is None:
            if isinstance(self.data, str):
                # Already JSON string
                self._raw_json = RawJSONResult(self.data)
            else:
                # Convert to JSON
                json_str = json.dumps(self.data)
                self._raw_json = RawJSONResult(json_str)
        return self._raw_json

    def __getattr__(self, name):
        """Allow GraphQL to access any attribute.

        This makes the object appear to have any field GraphQL expects.
        """
        # Return self for any attribute access
        # This allows GraphQL to traverse the object
        return self

    def __getitem__(self, key):
        """Allow dictionary-style access."""
        return self

    def __iter__(self):
        """Make it iterable for list types."""
        # Return a single item so GraphQL thinks it's a valid list
        return iter([self])

    def __bool__(self):
        """Always truthy."""
        return True

    def __len__(self):
        """Return 1 for list types."""
        return 1


def wrap_for_passthrough(data: Any, field_name: str) -> Any:
    """Wrap data for passthrough mode.

    This function wraps data in a way that GraphQL will accept
    but that signals to bypass type validation.

    Args:
        data: The raw data
        field_name: The GraphQL field name

    Returns:
        Wrapped data that GraphQL will accept
    """
    # If already wrapped, return as-is
    if isinstance(data, (PassthroughResult, RawJSONResult)):
        return data

    # For None, return None (GraphQL handles this fine)
    if data is None:
        return None

    # For lists, wrap each item
    if isinstance(data, list):
        # Return a list with a single PassthroughResult
        # This satisfies GraphQL's list validation
        return [PassthroughResult(data, field_name)]

    # For dicts and other types, wrap directly
    return PassthroughResult(data, field_name)
