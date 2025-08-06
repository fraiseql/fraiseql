"""Field definitions and metadata for FraiseQL type system."""

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
    graphql_name: str | None = None
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
        graphql_name: str | None = None,
    ) -> None:
        """Initialize a FraiseQL field with metadata.

        Args:
            field_type: The type of the field (can be inferred from annotations)
            default: Default value for the field
            default_factory: Factory function to generate default values
            init: Whether to include in __init__ method
            repr: Whether to include in __repr__ method
            compare: Whether to include in comparison operations
            purpose: Field usage - "input", "output", or "both"
            description: Human-readable field description
            graphql_name: Custom GraphQL field name (defaults to Python name)
        """
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
        self.graphql_name = graphql_name

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
    graphql_name: str | None = None,
    inferred_type: type | None = None,  # Added this for automatic annotation inference
) -> FraiseQLField:
    """Create a new FraiseQLField with metadata for schema building and codegen.

    This function creates field definitions for FraiseQL types with rich metadata
    that controls GraphQL schema generation, serialization, and runtime behavior.

    Args:
        field_type: Explicit type annotation for the field. If None, will be inferred
            from the class annotation where the field is defined.
        default: Default value for the field. Cannot be used with default_factory.
        default_factory: Callable that returns a default value. Useful for mutable
            defaults like lists or dicts. Cannot be used with default.
        init: Whether this field should be included in the generated __init__ method.
            Set to False for computed or derived fields.
        repr: Whether this field should be included in the __repr__ string.
        compare: Whether this field should be used in equality comparisons.
        purpose: Controls where this field appears:
            - "input": Only in GraphQL input types
            - "output": Only in GraphQL output types
            - "both": In both input and output types (default)
        description: Human-readable description that appears in GraphQL schema
            documentation and introspection.
        graphql_name: Override the GraphQL field name. By default uses the Python
            attribute name, but this allows customization for API compatibility.
        inferred_type: Internal parameter for automatic type inference. Users should
            not set this directly.

    Returns:
        FraiseQLField: A field descriptor with the specified metadata.

    Examples:
        Basic field with default::

            @fraise_type
            class User:
                id: UUID
                name: str
                status: str = fraise_field(default="active")

        Field with factory for mutable default::

            @fraise_type
            class Post:
                id: UUID
                tags: list[str] = fraise_field(default_factory=list)
                metadata: dict[str, Any] = fraise_field(default_factory=dict)

        Output-only computed field::

            @fraise_type
            class Product:
                id: UUID
                price: Decimal
                tax_rate: Decimal
                total_price: Decimal = fraise_field(
                    purpose="output",
                    init=False,
                    description="Price including tax"
                )

        Field with GraphQL customization::

            @fraise_type
            class Article:
                id: UUID
                internal_ref: str = fraise_field(
                    graphql_name="reference",
                    description="Public-facing reference number"
                )

        Input-only field for mutations::

            @fraise_input
            class CreateUserInput:
                name: str
                email: str
                password: str = fraise_field(
                    purpose="input",
                    repr=False,  # Don't include in repr for security
                    description="User password (will be hashed)"
                )

    Notes:
        - default and default_factory are mutually exclusive
        - Fields without defaults are required in GraphQL schema
        - The purpose parameter helps separate concerns between API input/output
        - Use description for API documentation that appears in GraphQL playground
    """
    logger.debug("Creating FraiseQLField for type: %s with purpose: %s", field_type, purpose)
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
        graphql_name=graphql_name,
    )
