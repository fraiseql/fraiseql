"""GraphQL ID scalar backed by UUID, used for opaque identifier representation."""

from __future__ import annotations

import uuid
from typing import Any


class ID:
    """A GraphQL-safe identifier backed internally by UUID."""

    __slots__ = ("_value",)

    def __init__(self, value: Any) -> None:
        """Initialize an ID instance from a UUID or a valid UUID string."""
        if isinstance(value, uuid.UUID):
            self._value = value
        elif isinstance(value, str):
            try:
                self._value = uuid.UUID(value)
            except ValueError as exc:
                msg = f"Invalid UUID string: {value}"
                raise TypeError(msg) from exc
        else:
            msg = f"ID must be initialized with a UUID or str, not {type(value).__name__}"
            raise TypeError(msg)

    @classmethod
    def coerce(cls, value: object) -> ID:
        """Coerce a UUID, str, or ID into an ID instance."""
        if isinstance(value, ID):
            return value
        if isinstance(value, uuid.UUID):
            return cls(value)
        if isinstance(value, str):
            return cls(value)
        msg = f"Cannot coerce {type(value).__name__} to ID"
        raise TypeError(msg)

    def __str__(self) -> str:
        """Return the string representation of the UUID."""
        return str(self._value)

    def __repr__(self) -> str:
        """Return the debug representation of the ID."""
        return f"ID('{self._value}')"

    def __eq__(self, other: object) -> bool:
        """Check equality with another ID or UUID."""
        if isinstance(other, ID):
            return self._value == other._value
        if isinstance(other, uuid.UUID):
            return self._value == other
        return NotImplemented

    def __hash__(self) -> int:
        """Return a hash based on the underlying UUID."""
        return hash(self._value)

    @property
    def uuid(self) -> uuid.UUID:
        """Access the underlying UUID value."""
        return self._value
