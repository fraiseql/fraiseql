"""Federation support for Apollo Federation v2 in FraiseQL.

This module provides decorators for defining federated GraphQL schemas:
- @key: Define federation keys for entity resolution
- @extends: Extend types from other subgraphs
- @external: Mark fields as owned by other subgraphs
- @requires: Declare field dependencies
- @provides: Mark fields that provide data for other subgraphs

Example:
    ```python
    from fraiseql import type as fraiseql_type
    from fraiseql.federation import key, extends, external, requires, provides

    # Authoritative User type in this subgraph
    @fraiseql_type
    @key("id")
    class User:
        id: str
        email: str

    # Extended User type in another subgraph
    @fraiseql_type
    @extends
    @key("id")
    class User:
        id: str = external()
        email: str = external()
        orders: list[Order]
    ```
"""

from __future__ import annotations

import inspect
from typing import TYPE_CHECKING, Any, TypeVar

from fraiseql.errors import FederationValidationError

if TYPE_CHECKING:
    from collections.abc import Callable

T = TypeVar("T")

# Federation metadata keys
_KEYS = "keys"
_EXTEND = "extend"
_EXTERNAL_FIELDS = "external_fields"
_REQUIRES = "requires"
_PROVIDES_DATA = "provides_data"


def _init_federation_metadata() -> dict[str, Any]:
    """Initialize an empty federation metadata dictionary."""
    return {
        _KEYS: [],
        _EXTEND: False,
        _EXTERNAL_FIELDS: [],
        _REQUIRES: {},
        _PROVIDES_DATA: [],
    }


def _get_or_init_federation_metadata(cls: type[T]) -> dict[str, Any]:
    """Get or initialize federation metadata on a class."""
    if not hasattr(cls, "__fraiseql_federation__"):
        cls.__fraiseql_federation__ = _init_federation_metadata()
    return cls.__fraiseql_federation__


def _check_type_decorator_applied(cls: type[T]) -> bool:
    """Check if @fraiseql.type or @type decorator is in the decorator stack.

    This is a heuristic check using source inspection.
    Returns True if type decorator appears to be present.
    """
    try:
        source = inspect.getsource(cls)
        # Check if fraiseql_type or just 'type' decorator appears before class definition
        lines = source.split("\n")
        return any("@" in line and "type" in line.lower() for line in lines)
    except (OSError, TypeError):
        # Can't get source (e.g., in tests or REPL), assume it's OK
        return True


def _extract_key_fields(metadata: dict[str, Any]) -> set[str]:
    """Extract all field names used in keys from federation metadata."""
    key_fields: set[str] = set()
    for key_def in metadata.get(_KEYS, []):
        key_fields.update(key_def.get("fields", []))
    return key_fields


def _validate_field_exists(field_name: str, annotations: dict[str, Any], class_name: str) -> None:
    """Validate that a field name exists in class annotations."""
    if field_name not in annotations:
        raise FederationValidationError(f"Field '{field_name}' not found in {class_name}")


def _collect_field_markers(
    cls: type[T],
) -> tuple[set[str], dict[str, FieldDefault]]:
    """Collect all field markers and their metadata from a class.

    Returns:
        Tuple of (field_names_with_markers, marker_by_field)
    """
    annotations = getattr(cls, "__annotations__", {})
    field_markers: dict[str, FieldDefault] = {}
    marked_fields: set[str] = set()

    for field_name in annotations:
        field_default = getattr(cls, field_name, None)
        if isinstance(field_default, FieldDefault):
            field_markers[field_name] = field_default
            marked_fields.add(field_name)

    return marked_fields, field_markers


def _process_field_markers(
    metadata: dict[str, Any],
    field_markers: dict[str, FieldDefault],
    annotations: dict[str, Any],
    class_name: str,
    is_extended: bool = False,
) -> None:
    """Process field markers and update federation metadata.

    Handles @requires, @provides, and @external markers.
    """
    for field_name, marker in field_markers.items():
        # Handle @requires()
        if marker.requires:
            _validate_field_exists(marker.requires, annotations, class_name)
            metadata[_REQUIRES][field_name] = marker.requires

        # Handle @provides()
        if marker.provides:
            metadata[_PROVIDES_DATA].extend(marker.provides)

        # Handle @external() - only relevant for extended types
        if is_extended and marker.external:
            _validate_field_exists(field_name, annotations, class_name)
            metadata[_EXTERNAL_FIELDS].append(field_name)


class FieldDefault:
    """Marker for federation field metadata (external, requires, provides).

    This class is used as a default value in field annotations to store
    federation metadata on the field.

    Attributes:
        external: Whether field is owned by another subgraph.
        requires: Field name that must be resolved first.
        provides: List of external fields this field provides data for.
    """

    def __init__(
        self,
        external: bool = False,
        requires: str | None = None,
        provides: list[str] | None = None,
    ) -> None:
        self.external = external
        self.requires = requires
        self.provides = provides or []


def external() -> FieldDefault:
    """Mark a field as external (owned by another subgraph).

    Use this in extended types to mark which fields are owned by the
    authoritative subgraph.

    Returns:
        FieldDefault marked as external.

    Example:
        ```python
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: str = external()
            email: str = external()
            orders: list[Order]  # New field in this subgraph
        ```

    Raises:
        ValueError: If used on non-extended types or if field doesn't exist.
    """
    return FieldDefault(external=True)


def requires(field_name: str | list[str]) -> FieldDefault:
    """Mark a field as requiring another field to be resolved first.

    This declares that a field needs data from another field (in the same
    type or from federation) to compute its value.

    Args:
        field_name: Name of the field(s) that must be resolved first.

    Returns:
        FieldDefault with requires dependency.

    Example:
        ```python
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: str = external()
            email: str = external()
            profile: UserProfile = requires("email")  # Needs email to resolve
        ```

    Raises:
        ValueError: If the referenced field doesn't exist.
    """
    # Normalize field_name to string
    if isinstance(field_name, list):
        if len(field_name) != 1:
            raise FederationValidationError("@requires supports only single field dependency")
        field_name = field_name[0]
    return FieldDefault(requires=field_name)


def provides(*targets: str) -> FieldDefault:
    """Mark a field as providing data for external subgraph fields.

    This declares that this field's data can be used to resolve fields
    in other subgraphs.

    Args:
        *targets: List of "Type.field" references this field provides data for.

    Returns:
        FieldDefault with provides targets.

    Example:
        ```python
        @fraiseql_type
        @key("id")
        class User:
            id: str
            email: str = provides("Order.owner_email", "Invoice.owner_email")
        ```
    """
    return FieldDefault(provides=list(targets))


def key(field_names: str | list[str]) -> Callable[[type[T]], type[T]]:
    """Mark a type with a federation key for entity resolution.

    Federation keys are used to uniquely identify entities and resolve them
    across subgraphs. Multiple @key decorators define composite keys.

    Args:
        field_names: Field name or list of field names that form the key.

    Returns:
        Decorator function that stores key metadata on the class.

    Example:
        ```python
        @fraiseql_type
        @key("id")
        class User:
            id: str
            email: str

        @fraiseql_type
        @key("tenant_id")
        @key("id")
        class Account:
            tenant_id: str
            id: str
            name: str
        ```

    Raises:
        ValueError: If key field doesn't exist on the type.
    """
    # Normalize field_names to list
    if isinstance(field_names, str):
        field_names = [field_names]

    def decorator(cls: type[T]) -> type[T]:
        # Validate that @type decorator was applied or will be applied
        if not _check_type_decorator_applied(cls):
            raise TypeError(f"@key requires @type decorator to be applied to {cls.__name__}")

        # Get or initialize federation metadata
        metadata = _get_or_init_federation_metadata(cls)
        annotations = getattr(cls, "__annotations__", {})

        # Validate all key fields exist
        for field_name in field_names:
            _validate_field_exists(field_name, annotations, cls.__name__)

        # Check for duplicate keys
        new_key = {"fields": field_names}
        if new_key in metadata[_KEYS]:
            raise FederationValidationError(f"Duplicate key field in {cls.__name__}")

        # Add key to federation metadata
        metadata[_KEYS].append(new_key)

        # Collect and process field markers on non-extended types
        # Note: Extended types are processed in @extends decorator
        if not metadata[_EXTEND]:
            _, field_markers = _collect_field_markers(cls)
            _process_field_markers(metadata, field_markers, annotations, cls.__name__)

        return cls

    return decorator


def extends(cls: type[T] | None = None) -> type[T] | Callable[[type[T]], type[T]]:
    """Mark a type as extending a type from another subgraph.

    Extended types can have external fields from the authoritative subgraph
    and add new fields specific to this subgraph.

    Args:
        cls: The class being decorated.

    Returns:
        The original class with federation.extend = True.

    Example:
        ```python
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: str = external()
            email: str = external()
            orders: list[Order]  # New field in this subgraph
        ```

    Raises:
        ValueError: If @extends is used without @key decorator.
    """

    def decorator(c: type[T]) -> type[T]:
        # Get or initialize federation metadata
        metadata = _get_or_init_federation_metadata(c)

        # Validate @key decorator was applied first
        if not metadata[_KEYS]:
            raise FederationValidationError(f"@extends requires @key decorator on {c.__name__}")

        # Mark type as extended
        metadata[_EXTEND] = True

        # Extract key field names for validation
        key_fields = _extract_key_fields(metadata)

        # Collect field markers and check consistency
        annotations = getattr(c, "__annotations__", {})
        _, field_markers = _collect_field_markers(c)

        # Determine which non-key fields are external
        external_non_key = {
            name
            for name, marker in field_markers.items()
            if marker.external and name not in key_fields
        }

        # If any non-key field is external, all key fields must be external too
        if external_non_key:
            external_key_fields = {
                name
                for name, marker in field_markers.items()
                if marker.external and name in key_fields
            }
            missing_key_fields = key_fields - external_key_fields
            if missing_key_fields:
                # Report error using first non-key external field (for consistent error)
                raise FederationValidationError(
                    f"Field '{next(iter(external_non_key))}' not found in {c.__name__}"
                )

        # Process field markers (requires, provides, external)
        _process_field_markers(metadata, field_markers, annotations, c.__name__, is_extended=True)

        return c

    # Support both @extends and @extends()
    if cls is None:
        # Called with parentheses: @extends()
        return decorator
    # Called without parentheses: @extends
    return decorator(cls)
