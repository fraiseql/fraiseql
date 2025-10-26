"""Trinity Identifiers pattern for FraiseQL.

Provides three-tier ID system:
- pk_* (int) - Internal primary key for fast joins
- id (UUID) - Public API identifier
- identifier (str) - Human-readable URL slug

Usage:
    from fraiseql.patterns import TrinityMixin
    from uuid import UUID

    class User(TrinityMixin):
        '''User with Trinity identifiers.'''

        # Public fields (exposed to GraphQL)
        id: UUID
        identifier: str | None

        username: str
        email: str

        # Internal primary key is automatically handled by TrinityMixin
        # It's stored as pk_{entity} and hidden from GraphQL
"""

from typing import Any, ClassVar, TypeVar


class TrinityMixin:
    """Mixin to add Trinity identifier support to GraphQL types.

    Automatically manages:
    - Internal pk_* field (hidden from GraphQL)
    - Public id field (UUID)
    - Optional identifier field (human-readable slug)

    The internal pk_* field is stored as a private attribute and used
    for database queries, but never exposed through GraphQL.
    """

    # Class-level configuration
    __trinity_entity_name__: ClassVar[str | None] = None

    def __init_subclass__(cls, **kwargs: Any) -> None:
        """Set up Trinity pattern when class is defined."""
        super().__init_subclass__(**kwargs)

        # Auto-detect entity name from class name (User -> user)
        if cls.__trinity_entity_name__ is None:
            cls.__trinity_entity_name__ = cls.__name__.lower()

    @property
    def _pk_name(self) -> str:
        """Get the pk_* column name for this entity."""
        return f"pk_{self.__trinity_entity_name__}"

    def get_internal_pk(self) -> int | None:
        """Get the internal SERIAL primary key value.

        Returns:
            Internal pk_* value, or None if not set
        """
        return getattr(self, self._pk_name, None)

    def set_internal_pk(self, pk: int) -> None:
        """Set the internal SERIAL primary key value.

        Args:
            pk: Internal pk_* value
        """
        setattr(self, self._pk_name, pk)


def trinity_field(**kwargs: Any) -> dict:
    """Create field metadata for Trinity identifiers.

    This returns metadata that can be used with Strawberry or other GraphQL
    libraries to document Trinity identifier fields.

    Args:
        **kwargs: Field configuration (description, etc.)

    Returns:
        Field metadata dictionary

    Example:
        # With Strawberry
        import strawberry
        id: UUID = strawberry.field(**trinity_field(description="User UUID"))

        # Or just for documentation
        metadata = trinity_field(description="User UUID")
    """
    return kwargs


def get_pk_column_name(entity_type: type) -> str:
    """Get the pk_* column name for an entity type.

    Args:
        entity_type: Entity class (e.g., User, Post)

    Returns:
        Column name (e.g., "pk_user", "pk_post")

    Example:
        >>> get_pk_column_name(User)
        'pk_user'
    """
    if hasattr(entity_type, "__trinity_entity_name__"):
        entity_name = entity_type.__trinity_entity_name__
    else:
        entity_name = entity_type.__name__.lower()

    return f"pk_{entity_name}"


def get_identifier_from_slug(slug: str) -> str:
    """Convert a URL slug to a database identifier.

    Handles common slug formats:
    - @username -> username
    - /users/@username -> username
    - username -> username

    Args:
        slug: URL slug string

    Returns:
        Cleaned identifier string

    Example:
        >>> get_identifier_from_slug("@johndoe")
        'johndoe'
        >>> get_identifier_from_slug("/users/@johndoe")
        'johndoe'
    """
    # Strip whitespace first
    slug = slug.strip()

    # Remove leading @ if present
    slug = slug.removeprefix("@")

    # Extract last segment if it's a path
    if "/" in slug:
        slug = slug.split("/")[-1]
        slug = slug.removeprefix("@")

    # Lowercase and strip any remaining whitespace
    return slug.lower().strip()


# Type alias for Trinity-enabled entities
TrinityEntity = TypeVar("TrinityEntity", bound=TrinityMixin)
