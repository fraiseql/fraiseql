"""Missing docstring."""

import logging
from collections.abc import Callable
from typing import Any, Literal

logger = logging.getLogger(__name__)

FRAISE_MISSING = object()
FraiseQLFieldPurpose = Literal["input", "output", "both"]


class FraiseQLField:
    """Represents a field in a FraiseQL schema with metadata for code generation.

    Attributes:
        name: The name of the field.
        index: The field's index, or None if not applicable.
        default: The default value for the field, or FRAISE_MISSING if not specified.
        default_factory: A callable used to generate a default value, or None.
        init: Whether the field should be included in the class's `__init__` method.
        repr: Whether the field should be included in the class's `__repr__` method.
        compare: Whether the field should be included in comparison operations.
        purpose: The intended purpose of the field, e.g., "input", "output", or "both".
        field_type: The type of the field (inferred from annotations or specified explicitly).
        description: A description of the field's purpose or behavior.
    """

    name: str
    index: int | None
    default: Any
    default_factory: Callable[[], Any] | None
    init: bool
    repr: bool
    compare: bool
    purpose: str
    field_type: type[Any] | None = None
    description: str | None
    __fraiseql_field__: bool = True

    def __init__(
        self,
        *,
        field_type: type | None = None,
        default: Any = FRAISE_MISSING,
        default_factory: Callable[[], Any] | None = None,
        init: bool = True,
        repr: bool = True,
        compare: bool = True,
        purpose: FraiseQLFieldPurpose = "both",
        description: str | None = None,
    ) -> None:
        """Missing docstring."""
        if default is not FRAISE_MISSING and default_factory is not None:
            msg = "Cannot specify both default and default_factory"
            raise ValueError(msg)

        self.default = default
        self.default_factory = default_factory
        self.field_type = field_type
        self.init = init
        self.repr = repr
        self.compare = compare
        self.purpose = purpose
        self.description = description

    def has_default(self) -> bool:
        """Return True if a default value or factory is present."""
        return self.default is not FRAISE_MISSING or self.default_factory is not None

    @property
    def type(self) -> type[Any] | None:
        """Alias for field_type for backward compatibility."""
        return self.field_type


def fraise_field(
    *,
    field_type: type | None = None,
    default: Any = FRAISE_MISSING,
    default_factory: Callable[[], Any] | None = None,
    init: bool = True,
    repr: bool = True,
    compare: bool = True,
    purpose: FraiseQLFieldPurpose = "both",
    description: str | None = None,
    inferred_type: type | None = None,  # Added this for automatic annotation inference
) -> FraiseQLField:
    """Create a new FraiseQLField with metadata for schema building and codegen."""
    logger.debug(
        "Creating FraiseQLField for type: %s with purpose: %s", field_type, purpose
    )
    # Validate purpose
    if purpose not in {"input", "output", "both"}:
        msg = f"Invalid purpose for FraiseQLField: {purpose}"
        raise ValueError(msg)

    # If no field_type is provided, infer it from the annotation.
    if field_type is None and inferred_type is not None:
        field_type = inferred_type

    return FraiseQLField(
        default=default,
        default_factory=default_factory,
        field_type=field_type,
        init=init,
        repr=repr,
        compare=compare,
        purpose=purpose,
        description=description,
    )
