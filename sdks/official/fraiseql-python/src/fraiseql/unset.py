"""UNSET sentinel for three-state update mutation inputs.

In update mutations, fields can be in one of three states:

- **UNSET** — field was not provided; omit from generated SQL.
- **None** — explicitly set to NULL in the database.
- **<value>** — set to the given value.

This module provides the ``UNSET`` sentinel object, which is a singleton
that can be distinguished from ``None`` using ``is`` comparison.

Example::

    @fraiseql.input
    class UpdateUserInput:
        name: str | None | UNSET_TYPE = UNSET
        email: str | None | UNSET_TYPE = UNSET

    # In the generated SQL:
    #   name=UNSET  → field omitted entirely
    #   name=None   → SET name = NULL
    #   name="Alice" → SET name = 'Alice'
"""

from __future__ import annotations

from typing import Any


class _UnsetType:
    """Singleton sentinel type representing 'not provided'."""

    _instance: _UnsetType | None = None

    def __new__(cls) -> _UnsetType:
        if cls._instance is None:
            cls._instance = super().__new__(cls)
        return cls._instance

    def __repr__(self) -> str:
        return "UNSET"

    def __bool__(self) -> bool:
        return False

    def __eq__(self, other: Any) -> bool:
        return other is UNSET

    def __hash__(self) -> int:
        return hash("UNSET")


#: Sentinel value meaning "field not provided" in update mutations.
#: Compare with ``is``: ``if value is UNSET: ...``
UNSET: _UnsetType = _UnsetType()

# Re-export the type so callers can annotate: ``field: str | UNSET_TYPE``
UNSET_TYPE = _UnsetType
