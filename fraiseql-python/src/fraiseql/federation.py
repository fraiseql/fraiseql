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

from typing import TYPE_CHECKING, Any, Callable, TypeVar

from fraiseql.errors import FederationValidationError

if TYPE_CHECKING:
    from typing import Union

T = TypeVar("T")


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
    if isinstance(field_names, str):
        field_names = [field_names]

    def decorator(cls: type[T]) -> type[T]:
        # Initialize federation metadata if not present
        if not hasattr(cls, "__fraiseql_federation__"):
            cls.__fraiseql_federation__ = {
                "keys": [],
                "extend": False,
                "external_fields": [],
                "requires": {},
                "provides_data": [],
            }

        # Get field annotations
        annotations = getattr(cls, "__annotations__", {})

        # Validate that all key fields exist
        for field_name in field_names:
            if field_name not in annotations:
                raise FederationValidationError(f"Field '{field_name}' not found in {cls.__name__}")

        # Check for duplicate key
        existing_keys = cls.__fraiseql_federation__["keys"]
        new_key = {"fields": field_names}
        if new_key in existing_keys:
            raise FederationValidationError(f"Duplicate key field in {cls.__name__}")

        # Add key to federation metadata
        cls.__fraiseql_federation__["keys"].append(new_key)

        # Scan fields for @requires() and @provides() markers on non-extended types
        # (Extended types will be scanned in @extends decorator)
        if not cls.__fraiseql_federation__["extend"]:
            for field_name in annotations:
                field_default = getattr(cls, field_name, None)

                if isinstance(field_default, FieldDefault):
                    # Handle @requires() on non-extended type
                    if field_default.requires:
                        required_field = field_default.requires
                        if required_field not in annotations:
                            raise FederationValidationError(
                                f"Field '{required_field}' not found in {cls.__name__}"
                            )
                        cls.__fraiseql_federation__["requires"][field_name] = (
                            required_field
                        )

                    # Handle @provides() on non-extended type
                    if field_default.provides:
                        cls.__fraiseql_federation__["provides_data"].extend(
                            field_default.provides
                        )

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
        # Initialize federation metadata if not present
        if not hasattr(c, "__fraiseql_federation__"):
            c.__fraiseql_federation__ = {
                "keys": [],
                "extend": False,
                "external_fields": [],
                "requires": {},
                "provides_data": [],
            }

        # Check that @key decorator was used
        if not c.__fraiseql_federation__["keys"]:
            raise FederationValidationError(f"@extends requires @key decorator on {c.__name__}")

        # Mark type as extended
        c.__fraiseql_federation__["extend"] = True

        # Scan fields for @external(), @requires(), @provides() markers
        annotations = getattr(c, "__annotations__", {})
        for field_name, field_type in annotations.items():
            # Get field default if present
            field_default = getattr(c, field_name, None)

            if isinstance(field_default, FieldDefault):
                # Handle @external()
                if field_default.external:
                    if field_name not in annotations:
                        raise FederationValidationError(
                            f"Field '{field_name}' not found in {c.__name__}"
                        )
                    c.__fraiseql_federation__["external_fields"].append(field_name)

                # Handle @requires()
                if field_default.requires:
                    required_field = field_default.requires
                    if required_field not in annotations:
                        raise FederationValidationError(
                            f"Field '{required_field}' not found in {c.__name__}"
                        )
                    c.__fraiseql_federation__["requires"][field_name] = required_field

                # Handle @provides()
                if field_default.provides:
                    c.__fraiseql_federation__["provides_data"].extend(
                        field_default.provides
                    )

        return c

    # Support both @extends and @extends()
    if cls is None:
        # Called with parentheses: @extends()
        return decorator
    # Called without parentheses: @extends
    return decorator(cls)
