"""Shared FraiseQL runtime type definition model and related helpers."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from fraiseql.fields import FraiseQLField


class FraiseQLTypeDefinition:
    """Internal marker for FraiseQL-annotated types.

    This class is attached to any class decorated with `@fraise_type`, `@fraise_input`,
    `@success`, or `@failure`, and stores runtime metadata needed for schema generation,
    SQL modeling, and execution.

    Attributes:
        python_type (type): The actual user-defined Python class.
        is_input (bool): True if this type is meant for input (e.g., arguments).
        kind (str): 'input', 'type', 'success', or 'failure'.
        sql_source (str | None): Optional SQL table/view this type is bound to.
        fields (dict[str, FraiseQLField]): Ordered field name → metadata.
        type_hints (dict[str, type]): Field name → resolved Python type hints.
        is_frozen (bool): Whether the type is immutable.
        kw_only (bool): Whether the generated __init__ is keyword-only.
        field_map (dict[str, FraiseQLField]): Fast lookup for fields by name.
        type (type): Reference to the original user-defined class.
    """

    __slots__ = (
        "field_map",
        "fields",
        "is_frozen",
        "is_input",
        "kind",
        "kw_only",
        "python_type",
        "sql_source",
        "type",
        "type_hints",
    )

    def __init__(
        self,
        *,
        python_type: type,
        is_input: bool,
        kind: str,
        sql_source: str | None,
        fields: dict[str, FraiseQLField],
        type_hints: dict[str, type],
        is_frozen: bool = False,
        kw_only: bool = False,
    ) -> None:
        self.python_type = python_type
        self.is_input = is_input
        self.kind = kind
        self.sql_source = sql_source
        self.fields = fields
        self.type_hints = type_hints
        self.is_frozen = is_frozen
        self.kw_only = kw_only

        # Additional introspection metadata
        self.field_map: dict[str, FraiseQLField] = dict(fields)
        self.type: type = python_type

    @property
    def is_output(self) -> bool:
        """Returns True if this is an output type (i.e., not an input type)."""
        return not self.is_input

    def __repr__(self) -> str:
        """Returns a string representation of the FraiseQLTypeDefinition instance."""
        return (
            f"<FraiseQLTypeDefinition("
            f"type={self.python_type.__name__}, "
            f"is_input={self.is_input}, "
            f"kind={self.kind}, "
            f"is_frozen={self.is_frozen}, "
            f"kw_only={self.kw_only}, "
            f"sql_source={self.sql_source}, "
            f"fields={list(self.fields.keys())})>"
        )

    def describe(self) -> dict[str, object]:
        """Returns a structured description of the type definition."""
        return {
            "typename": self.python_type.__name__,
            "is_input": self.is_input,
            "kind": self.kind,
            "sql_source": self.sql_source,
            "is_frozen": self.is_frozen,
            "kw_only": self.kw_only,
            "fields": {
                name: {
                    "type": self.type_hints.get(name),
                    "purpose": field.purpose,
                    "default": field.default,
                    "default_factory": field.default_factory,
                    "description": field.description,
                }
                for name, field in self.fields.items()
            },
        }


class Unset:
    """Sentinel value representing a missing or undefined input.

    This is used to distinguish between unset and explicitly-null values.
    """

    __slots__ = ()

    def __bool__(self) -> bool:
        """UNSET is always falsy."""
        return False

    def __str__(self) -> str:
        """String representation of UNSET."""
        return "UNSET"

    def __repr__(self) -> str:
        """Repr of UNSET sentinel value."""
        return "UNSET"


UNSET = Unset()


class ScalarMarker:
    """Base class for all FraiseQL scalar marker types."""

    __slots__ = ()
