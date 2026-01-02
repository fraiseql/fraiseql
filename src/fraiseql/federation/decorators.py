"""Federation decorators for entity definitions.

Provides @entity, @extend_entity, and external() for marking types as federated entities.

Examples:
    Simple entity with auto-detected key:
    >>> from fraiseql.federation import entity
    >>>
    >>> @entity
    ... class User:
    ...     id: str
    ...     name: str

    Entity with explicit key:
    >>> @entity(key="user_id")
    ... class User:
    ...     user_id: str
    ...     name: str

    Composite key:
    >>> @entity(key=["org_id", "user_id"])
    ... class OrgUser:
    ...     org_id: str
    ...     user_id: str

    Type extension with external fields:
    >>> from fraiseql.federation import extend_entity, external
    >>>
    >>> @extend_entity(key="id")
    ... class Product:
    ...     id: str = external()
    ...     reviews: list["Review"]
"""

from typing import List, Optional, Type, TypeVar, Union, overload

from .auto_detect import auto_detect_key_python, validate_key_field

T = TypeVar("T")

# Global registry of federation entities
_ENTITY_REGISTRY: dict[str, "EntityMetadata"] = {}


class EntityMetadata:
    """Metadata for a federated entity.

    Stores information about an entity including its type name, key field(s),
    and other federation-related metadata.

    Attributes:
        cls: The Python class this metadata describes
        type_name: GraphQL type name
        key: Key field name or list of key fields
        resolved_key: The resolved key (after auto-detection)
        fields: Field annotations from the class
        is_extension: Whether this is an extended entity (from another subgraph)
        external_fields: Set of fields marked as external
    """

    def __init__(
        self,
        cls: Type,
        key: Optional[Union[str, List[str]]] = None,
        is_extension: bool = False,
    ):
        self.cls = cls
        self.type_name = cls.__name__
        self.key = key
        self.is_extension = is_extension
        self.external_fields: set[str] = set()

        # Extract field annotations
        self.fields = getattr(cls, "__annotations__", {}).copy()

        # Resolve key
        self.resolved_key = self._resolve_key()

    def _resolve_key(self) -> Union[str, List[str]]:
        """Resolve key: explicit > auto-detected > error.

        Returns:
            Key field name or list of key fields

        Raises:
            ValueError: If key cannot be determined and no explicit key provided
            ValueError: If explicit key field does not exist on the class
        """
        # Explicit key provided
        if self.key is not None:
            if isinstance(self.key, str):
                # Validate single key field exists
                return validate_key_field(self.cls, self.key)
            # Validate composite key fields all exist
            for field in self.key:
                if field not in self.fields:
                    raise ValueError(
                        f"Key field '{field}' not found in {self.type_name}. "
                        f"Available fields: {list(self.fields.keys())}"
                    )
            return self.key

        # Auto-detect key
        detected = auto_detect_key_python(self.cls)

        if detected is None:
            raise ValueError(
                f"{self.type_name} has no 'id' field. "
                f"Specify key explicitly: @entity(key='field_name')"
            )

        return detected

    def mark_field_external(self, field_name: str) -> None:
        """Mark a field as external (defined in another subgraph)."""
        self.external_fields.add(field_name)

    def __repr__(self) -> str:
        return (
            f"EntityMetadata("
            f"type_name={self.type_name!r}, "
            f"key={self.resolved_key!r}, "
            f"is_extension={self.is_extension}"
            f")"
        )


class _External:
    """Marker for fields that are external (defined in another subgraph).

    Used with @extend_entity to mark fields that come from other subgraphs.

    Example:
        >>> @extend_entity(key="id")
        ... class Product:
        ...     id: str = external()  # From another subgraph
        ...     reviews: list["Review"]  # New field in this subgraph
    """

    def __repr__(self) -> str:
        return "<external>"

    def __init__(self):
        pass


def external() -> _External:
    """Mark field as external (defined in another subgraph).

    Used with @extend_entity to indicate that a field comes from
    another subgraph and should be marked with @external in SDL.

    Returns:
        Marker object (type: _External)

    Example:
        >>> @extend_entity(key="id")
        ... class Product:
        ...     id: str = external()
        ...     name: str = external()
        ...     reviews: list["Review"]
    """
    return _External()


# Overload signatures for @entity decorator
@overload
def entity(cls: Type[T]) -> Type[T]: ...


@overload
def entity(
    *,
    key: Optional[Union[str, List[str]]] = None,
) -> callable: ...


def entity(
    cls: Optional[Type[T]] = None,
    *,
    key: Optional[Union[str, List[str]]] = None,
) -> Union[Type[T], callable]:
    """Mark a type as a federated entity.

    Decorator for GraphQL types that should participate in Apollo Federation.
    Auto-detects the entity key from the 'id' field if not explicitly provided.

    Args:
        cls: The class to decorate (set automatically when used as @entity)
        key: Entity key field(s). Auto-detected from 'id' if not provided.
             Can be a single field name or list of field names for composite keys.

    Returns:
        Decorated class with federation metadata attached

    Raises:
        ValueError: If no key field found and key not explicitly provided

    Examples:
        Simple entity with auto-detected 'id' key:
        >>> @entity
        ... class User:
        ...     id: str
        ...     name: str
        ...     email: str

        Entity with explicit single key:
        >>> @entity(key="user_id")
        ... class User:
        ...     user_id: str
        ...     name: str

        Entity with composite key:
        >>> @entity(key=["org_id", "user_id"])
        ... class OrgUser:
        ...     org_id: str
        ...     user_id: str
        ...     permissions: list[str]
    """

    def decorator(cls_to_decorate: Type[T]) -> Type[T]:
        # Create metadata
        metadata = EntityMetadata(cls_to_decorate, key=key, is_extension=False)

        # Register entity
        _ENTITY_REGISTRY[metadata.type_name] = metadata

        # Store metadata on class for introspection
        cls_to_decorate.__fraiseql_entity__ = metadata  # type: ignore[attr-defined]

        return cls_to_decorate

    if cls is None:
        # Called with arguments: @entity(key="...")
        return decorator
    # Called without arguments: @entity
    return decorator(cls)


def extend_entity(
    cls: Optional[Type[T]] = None,
    *,
    key: Union[str, List[str]],
) -> Union[Type[T], callable]:
    """Mark a type as an extended federated entity.

    Used for entities defined in other subgraphs that this subgraph
    wants to add fields to. Fields marked with external() are
    defined in the other subgraph. New fields (without external())
    can be added to compute data or reference related entities.

    Args:
        cls: The class to decorate (set automatically when used as decorator)
        key: Reference key to parent entity (required). Single field name
             or list of field names for composite keys. Must match the key
             from the entity's originating subgraph.

    Returns:
        Decorated class with federation extension metadata

    Raises:
        ValueError: If key is not provided
        ValueError: If any key field doesn't exist on the class

    Examples:
        Extend product with reviews (single key):
        >>> from fraiseql.federation import extend_entity, external
        >>>
        >>> @extend_entity(key="id")
        ... class Product:
        ...     id: str = external()  # From products subgraph
        ...     name: str = external()  # From products subgraph
        ...     reviews: list["Review"]  # New field added by reviews subgraph

        Extend with computed field using @requires:
        >>> from fraiseql.federation import extend_entity, external, requires
        >>>
        >>> @extend_entity(key="id")
        ... class Product:
        ...     id: str = external()
        ...     price: float = external()
        ...     currency: str = external()
        ...
        ...     @requires("price currency")
        ...     async def price_in_cents(self) -> int:
        ...         '''Computed field using required external fields'''
        ...         return int(self.price * 100)
    """

    def decorator(cls_to_decorate: Type[T]) -> Type[T]:
        metadata = EntityMetadata(cls_to_decorate, key=key, is_extension=True)

        # Mark external fields
        for field_name, field_default in cls_to_decorate.__dict__.items():
            if isinstance(field_default, _External):
                metadata.mark_field_external(field_name)

        # Register entity
        _ENTITY_REGISTRY[metadata.type_name] = metadata

        # Store metadata on class
        cls_to_decorate.__fraiseql_entity__ = metadata  # type: ignore[attr-defined]

        return cls_to_decorate

    if cls is None:
        return decorator
    return decorator(cls)


def get_entity_registry() -> dict[str, EntityMetadata]:
    """Get all registered entities.

    Returns:
        Dictionary mapping type names to EntityMetadata

    Example:
        >>> from fraiseql.federation import get_entity_registry
        >>>
        >>> registry = get_entity_registry()
        >>> for type_name, metadata in registry.items():
        ...     print(f"{type_name}: key={metadata.resolved_key}")
    """
    return _ENTITY_REGISTRY.copy()


def get_entity_metadata(type_name: str) -> Optional[EntityMetadata]:
    """Get metadata for a specific entity.

    Args:
        type_name: GraphQL type name

    Returns:
        EntityMetadata if entity registered, None otherwise

    Example:
        >>> from fraiseql.federation import get_entity_metadata
        >>>
        >>> user_metadata = get_entity_metadata("User")
        >>> if user_metadata:
        ...     print(f"User key: {user_metadata.resolved_key}")
    """
    return _ENTITY_REGISTRY.get(type_name)


def clear_entity_registry() -> None:
    """Clear the entity registry (mainly for testing).

    Warning:
        This is primarily for test cleanup. Do not use in production code.
    """
    _ENTITY_REGISTRY.clear()
