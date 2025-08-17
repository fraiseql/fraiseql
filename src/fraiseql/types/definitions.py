"""Shared FraiseQL runtime type definition model and related helpers."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional, Self

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
        jsonb_column (str | None): Optional JSONB column name for data extraction.
        resolve_nested (bool): If True, resolve nested instances via separate queries.
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
        "jsonb_column",
        "kind",
        "kw_only",
        "python_type",
        "resolve_nested",
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
        jsonb_column: str | None = None,
        resolve_nested: bool = False,
        fields: dict[str, FraiseQLField],
        type_hints: dict[str, type],
        is_frozen: bool = False,
        kw_only: bool = False,
    ) -> None:
        self.python_type = python_type
        self.is_input = is_input
        self.kind = kind
        self.sql_source = sql_source
        self.jsonb_column = jsonb_column
        self.resolve_nested = resolve_nested
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
            f"jsonb_column={self.jsonb_column}, "
            f"fields={list(self.fields.keys())})>"
        )

    def describe(self) -> dict[str, object]:
        """Returns a structured description of the type definition."""
        return {
            "typename": self.python_type.__name__,
            "is_input": self.is_input,
            "kind": self.kind,
            "sql_source": self.sql_source,
            "jsonb_column": self.jsonb_column,
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


class UnsetType:
    """Sentinel value representing a missing or undefined input.

    This is used to distinguish between unset and explicitly-null values.
    Implements singleton pattern to ensure only one instance exists.
    """

    __instance: Optional[UnsetType] = None

    def __new__(cls) -> Self:
        """Ensure only one instance of UnsetType exists."""
        if cls.__instance is None:
            ret = super().__new__(cls)
            cls.__instance = ret
            return ret
        return cls.__instance  # type: ignore[return-value]

    def __bool__(self) -> bool:
        """UNSET is always falsy."""
        return False

    def __str__(self) -> str:
        """String representation of UNSET."""
        return ""

    def __repr__(self) -> str:
        """Repr of UNSET sentinel value."""
        return "UNSET"


# Type UNSET as Any to make it compatible with any type annotation
UNSET: Any = UnsetType()


# Keep the old Unset class name as an alias for backward compatibility
Unset = UnsetType


class ScalarMarker:
    """Base class for all FraiseQL scalar marker types."""

    __slots__ = ()
