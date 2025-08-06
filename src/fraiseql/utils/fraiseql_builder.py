"""FraiseQL class builder: collects fields, builds init/repr/eq, and applies frozen behavior."""

import inspect
import logging
from collections.abc import Callable
from typing import Any, Literal, Never, TypeVar, get_type_hints

logger = logging.getLogger(__name__)


from fraiseql.fields import FRAISE_MISSING, FraiseQLField, fraise_field
from fraiseql.utils.annotations import unwrap_annotated
from fraiseql.utils.field_counter import next_field_index

T = TypeVar("T")


def collect_annotations(cls: type) -> dict[str, Any]:
    """Collect type annotations across MRO with full support for Annotated/Extras."""
    annotations: dict[str, Any] = {}
    for base in reversed(cls.__mro__):
        if base is object:
            continue
        base_hints = get_type_hints(base, include_extras=True)
        annotations.update({k: v for k, v in base_hints.items() if not k.startswith("__")})
    return annotations


def _get_field_for_annotation(
    name: str,
    hint: Any,
    cls: type,
) -> FraiseQLField:
    """Extract or create a FraiseQLField for a given annotation.

    Args:
        name: The field name
        hint: The type annotation
        cls: The class containing the field

    Returns:
        A FraiseQLField instance
    """
    base_type, extras = unwrap_annotated(hint)

    # Check for explicitly declared FraiseQLField in Annotated metadata
    explicit_field = next((x for x in extras if isinstance(x, FraiseQLField)), None)

    # Get default value from class attribute
    default_value = getattr(cls, name, FRAISE_MISSING)

    if isinstance(default_value, FraiseQLField):
        # Class attribute is already a FraiseQLField
        return default_value
    if explicit_field:
        # Use field from Annotated[T, fraise_field(...)]
        return explicit_field
    # Create new field with discovered default value
    if default_value is not FRAISE_MISSING:
        return fraise_field(field_type=base_type, default=default_value)
    return fraise_field(field_type=base_type)


def _should_include_field(
    field: FraiseQLField,
    kind: Literal["input", "output", "type", "interface"],
) -> bool:
    """Determine if a field should be included based on its purpose and the type kind.

    Args:
        field: The field to check
        kind: The kind of type being created

    Returns:
        True if the field should be included
    """
    purpose_map = {
        "input": ("input", "both"),
        "output": ("output", "both"),
        "type": ("type", "both", "output"),
        "interface": (
            "type",
            "both",
            "output",
        ),  # Interface follows same pattern as type
    }
    return field.purpose in purpose_map.get(kind, ())


def _configure_field_properties(
    field: FraiseQLField,
    name: str,
    base_type: Any,
    kind: Literal["input", "output", "type", "interface"],
) -> None:
    """Configure field properties like type, name, index, and init status.

    Args:
        field: The field to configure
        name: The field name
        base_type: The base type from annotation
        kind: The kind of type being created
    """
    # Ensure field has a type
    if field.field_type is None:
        field.field_type = base_type

    # Set field name
    field.name = name

    # Assign stable index for field ordering
    if not hasattr(field, "index") or field.index is None:
        field.index = next_field_index()

    # Set default purpose if not specified
    if not field.purpose:
        field.purpose = "both"

    # Configure init behavior
    # Output-only fields in @fraise_type decorated classes should not be in __init__
    # But mutation result types (kind="output") need all fields in __init__
    # Interface fields also follow the same pattern as types
    if field.purpose == "output" and kind in ("type", "interface"):
        field.init = False


def collect_fraise_fields(
    cls: type,
    type_hints: dict[str, Any] | None = None,
    kind: Literal["input", "output", "type", "interface"] = "output",
) -> tuple[dict[str, FraiseQLField], dict[str, Any]]:
    """Collect and normalize all fields across MRO as FraiseQLField instances.

    This function walks through a class's annotations and creates FraiseQLField
    instances for each field, handling inheritance, default values, and field
    configuration based on the type kind.

    Args:
        cls: The class to collect fields from
        type_hints: Optional pre-computed type hints
        kind: The kind of type being created ("input", "output", or "type")

    Returns:
        A tuple of (field_dict, annotations_dict) where:
        - field_dict maps field names to FraiseQLField instances
        - annotations_dict contains the collected type annotations
    """
    if type_hints is None:
        type_hints = get_type_hints(cls, include_extras=True)

    annotations = collect_annotations(cls)
    gql_fields: dict[str, FraiseQLField] = {}

    for name, hint in annotations.items():
        # Extract or create field
        field = _get_field_for_annotation(name, hint, cls)

        # Get base type for configuration
        base_type, _ = unwrap_annotated(hint)

        # Configure field properties
        _configure_field_properties(field, name, base_type, kind)

        logger.debug("Field '%s' created with purpose: %s", name, field.purpose)

        # Check if field should be included for this type kind
        if not _should_include_field(field, kind):
            continue

        gql_fields[name] = field

    return gql_fields, annotations


def make_init(fields: dict[str, FraiseQLField], *, kw_only: bool = True) -> Callable[..., None]:
    """Create a custom __init__ method from FraiseQL fields."""
    sorted_fields = sorted(fields.values(), key=lambda f: f.index or 0)

    positional: list[inspect.Parameter] = []
    keyword: list[inspect.Parameter] = []

    for f in sorted_fields:
        if not f.init:
            continue

        if f.default is not FRAISE_MISSING:
            param_default = f.default
        elif f.default_factory is not None:
            param_default = None  # This makes the parameter optional!
        else:
            param_default = inspect.Parameter.empty

        param = inspect.Parameter(
            name=f.name,
            kind=(
                inspect.Parameter.KEYWORD_ONLY
                if kw_only
                else inspect.Parameter.POSITIONAL_OR_KEYWORD
            ),
            default=param_default,
        )
        (keyword if kw_only else positional).append(param)

    params = positional + keyword

    def _fraiseql_init(self: object, *args: object, **kwargs: object) -> None:
        # Bind the arguments
        bound = inspect.Signature(params).bind(*args, **kwargs)
        bound.apply_defaults()

        for name, value in bound.arguments.items():
            field = fields.get(name)
            if field is None:
                continue

            final_value = value
            if final_value is None and field.default_factory is not None:
                final_value = field.default_factory()
            setattr(self, name, final_value)

    _fraiseql_init.__signature__ = inspect.Signature(
        parameters=params,
    )  # pyright: ignore[reportFunctionMemberAccess]
    return _fraiseql_init


def generate_repr(cls: type, fields: dict[str, FraiseQLField]) -> None:
    """Attach a __repr__ method that includes all repr=True fields."""

    def _fraiseql_repr(self: object) -> str:
        parts = [
            f"{name}={getattr(self, name)!r}"
            for name, field in fields.items()
            if getattr(field, "repr", True)
        ]
        return f"{cls.__name__}({', '.join(parts)})"

    cls.__repr__ = _fraiseql_repr  # type: ignore[attr-defined]


def generate_eq(cls: type, fields: dict[str, FraiseQLField]) -> None:
    """Attach an __eq__ method based on all compare=True fields."""

    def _fraiseql_eq(self: object, other: object) -> bool:
        if not isinstance(other, cls):
            return False
        return all(
            getattr(self, name) == getattr(other, name)
            for name, field in fields.items()
            if getattr(field, "compare", True)
        )

    cls.__eq__ = _fraiseql_eq  # type: ignore[attr-defined]


def apply_frozen(cls: type) -> None:
    """Prevent mutation of instances by overriding __setattr__ and __delattr__."""

    def frozen_setattr(self: object, name: str, value: object) -> Never:
        _ = self, value
        msg = f"Cannot assign to field '{name}': instance is frozen"
        raise AttributeError(msg)

    def frozen_delattr(self: object, name: str) -> Never:
        _ = self
        msg = f"Cannot delete field '{name}': instance is frozen"
        raise AttributeError(msg)

    cls.__setattr__ = frozen_setattr  # type: ignore[assignment]
    cls.__delattr__ = frozen_delattr  # type: ignore[assignment]
