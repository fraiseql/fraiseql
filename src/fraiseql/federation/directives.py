"""Federation directives for computed fields and field dependencies.

Provides decorators and markers for Federation Standard directives:
- @requires: Specifies that a field requires other fields to be resolved
- @provides: Marks eager field loading for related types
- @directive: Generic directive marker for extensibility

Examples:
    Computed field with dependencies:

        from fraiseql.federation import entity, requires, provides

        @entity
        class Product:
            id: str
            price: float
            currency: str

            @requires("price currency")
            def formatted_price(self) -> str:
                return f"{self.currency} {self.price}"

    Eager field loading:

        @entity
        class User:
            id: str
            name: str

            @provides("id name")
            async def posts(self, db):
                return await db.fetch("SELECT * FROM posts WHERE user_id = $1", self.id)
"""

from typing import Any, Callable, Optional, Set, TypeVar

T = TypeVar("T")


class _RequiresMarker:
    """Marker for fields that require other fields to be resolved.

    Used with @requires to indicate field dependencies.

    Example:
        @requires("price currency")
        def formatted_price(self) -> str:
            return f"{self.currency} {self.price}"
    """

    def __init__(self, fields: str | list[str]):
        """Initialize requires marker.

        Args:
            fields: Space/comma-separated field names or list of field names
        """
        self.fields = self._parse_fields(fields)
        self.field_set: Set[str] = set(self.fields)

    @staticmethod
    def _parse_fields(fields: str | list[str]) -> list[str]:
        """Parse fields from string or list.

        Args:
            fields: Space-separated "field1 field2", comma-separated "field1, field2",
                   or list ["field1", "field2"]

        Returns:
            List of field names
        """
        if isinstance(fields, list):
            return fields

        # Parse space/comma separated string
        parsed = []
        for field in fields.split():
            # Remove trailing comma if present
            cleaned_field = field.rstrip(",").strip()
            if cleaned_field:
                parsed.append(cleaned_field)
        return parsed

    def __repr__(self) -> str:
        return f"<requires: {', '.join(self.fields)}>"


class _ProvidesMarker:
    """Marker for fields that eagerly load related data.

    Used with @provides to indicate which fields are eagerly loaded.

    Example:
        @provides("id name")
        async def posts(self, db):
            return await db.fetch(...)
    """

    def __init__(self, fields: str | list[str]):
        """Initialize provides marker.

        Args:
            fields: Space/comma-separated field names or list of field names
        """
        self.fields = self._parse_fields(fields)
        self.field_set: Set[str] = set(self.fields)

    @staticmethod
    def _parse_fields(fields: str | list[str]) -> list[str]:
        """Parse fields from string or list.

        Args:
            fields: Space-separated "field1 field2", comma-separated "field1, field2",
                   or list ["field1", "field2"]

        Returns:
            List of field names
        """
        if isinstance(fields, list):
            return fields

        # Parse space/comma separated string
        parsed = []
        for field in fields.split():
            # Remove trailing comma if present
            cleaned_field = field.rstrip(",").strip()
            if cleaned_field:
                parsed.append(cleaned_field)
        return parsed

    def __repr__(self) -> str:
        return f"<provides: {', '.join(self.fields)}>"


def requires(fields: str | list[str]) -> Callable[[T], T]:
    """Mark a method as requiring specific fields to be resolved.

    Used in Federation Standard to declare field dependencies for computed fields.
    Enables the Apollo Gateway to fetch required fields before resolving the method.

    Args:
        fields: Fields required by this method, space/comma-separated or list

    Returns:
        Decorator function

    Example:
        @requires("price currency")
        def formatted_price(self) -> str:
            return f"{self.currency} {self.price}"

        @requires("latitude longitude")
        def distance_from_origin(self) -> float:
            import math
            return math.sqrt(self.latitude**2 + self.longitude**2)
    """

    def decorator(func: T) -> T:
        # Attach marker to function
        marker = _RequiresMarker(fields)
        func.__fraiseql_requires__ = marker  # type: ignore[attr-defined]
        return func

    return decorator


def provides(fields: str | list[str]) -> Callable[[T], T]:
    """Mark a method as providing specific fields in responses.

    Used in Federation Standard to declare fields that are eagerly loaded
    by this resolver. Helps the Apollo Gateway optimize entity queries.

    Args:
        fields: Fields provided by this resolver, space/comma-separated or list

    Returns:
        Decorator function

    Example:
        @provides("id name")
        async def posts(self, db):
            return await db.fetch(
                "SELECT id, title, user_id FROM posts WHERE user_id = $1",
                self.id
            )

        @provides("user_id created_at")
        async def recent_comments(self, db):
            return await db.fetch(
                "SELECT * FROM comments WHERE user_id = $1 ORDER BY created_at DESC LIMIT 10",
                self.id
            )
    """

    def decorator(func: T) -> T:
        # Attach marker to function
        marker = _ProvidesMarker(fields)
        func.__fraiseql_provides__ = marker  # type: ignore[attr-defined]
        return func

    return decorator


class DirectiveMetadata:
    """Metadata about directives on a field or method.

    Tracks which directives are applied and their parameters.

    Attributes:
        requires: _RequiresMarker if @requires applied, None otherwise
        provides: _ProvidesMarker if @provides applied, None otherwise
    """

    def __init__(
        self,
        requires: Optional[_RequiresMarker] = None,
        provides: Optional[_ProvidesMarker] = None,
    ):
        self.requires = requires
        self.provides = provides

    def has_requires(self) -> bool:
        """Check if this field has @requires directive."""
        return self.requires is not None

    def has_provides(self) -> bool:
        """Check if this field has @provides directive."""
        return self.provides is not None

    def get_required_fields(self) -> list[str]:
        """Get list of required fields."""
        return self.requires.fields if self.requires else []

    def get_provided_fields(self) -> list[str]:
        """Get list of provided fields."""
        return self.provides.fields if self.provides else []

    def __repr__(self) -> str:
        parts = []
        if self.requires:
            parts.append(f"requires={self.requires.fields}")
        if self.provides:
            parts.append(f"provides={self.provides.fields}")
        return f"DirectiveMetadata({', '.join(parts)})"


def get_directives(func: Any) -> DirectiveMetadata:
    """Extract directive metadata from a method.

    Args:
        func: Function to extract directives from

    Returns:
        DirectiveMetadata with applied directives

    Example:
        @requires("price")
        def formatted_price(self) -> str:
            return f"${self.price}"

        metadata = get_directives(formatted_price)
        assert metadata.get_required_fields() == ["price"]
    """
    requires_marker = getattr(func, "__fraiseql_requires__", None)
    provides_marker = getattr(func, "__fraiseql_provides__", None)

    return DirectiveMetadata(requires=requires_marker, provides=provides_marker)


def get_method_directives(cls: type) -> dict[str, DirectiveMetadata]:
    """Extract all directive metadata from a class's methods.

    Args:
        cls: Class to extract directives from

    Returns:
        Dict mapping method name to DirectiveMetadata

    Example:
        @entity
        class Product:
            id: str
            price: float

            @requires("price")
            def formatted_price(self) -> str:
                return f"${self.price}"

        directives = get_method_directives(Product)
        assert directives["formatted_price"].get_required_fields() == ["price"]
    """
    directives = {}

    for name, obj in cls.__dict__.items():
        if callable(obj) and not name.startswith("_"):
            directive_meta = get_directives(obj)
            # Only include if has directives
            if directive_meta.has_requires() or directive_meta.has_provides():
                directives[name] = directive_meta

    return directives
