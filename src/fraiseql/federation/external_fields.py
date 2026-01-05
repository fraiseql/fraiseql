"""Management of external fields in type extensions.

Provides utilities for tracking and validating external fields in extended types.
External fields are fields defined in other subgraphs that are referenced in this subgraph.

When using @extend_entity, mark fields as external() if they're defined elsewhere:

    from fraiseql.federation import extend_entity, external

    @extend_entity(key="id")
    class Product:
        # External fields (defined in products subgraph)
        id: str = external()
        name: str = external()
        price: float = external()

        # New fields (defined here in reviews subgraph)
        reviews: list["Review"]
        average_rating: float

        async def review_count(self) -> int:
            return len(self.reviews)

This enables the Apollo Gateway to understand which fields are local vs. external.
"""

from typing import Any


class ExternalFieldInfo:
    """Information about an external field in a type extension.

    Attributes:
        field_name: Name of the external field
        type_annotation: Python type annotation (if available)
        is_required: Whether field is required (non-null)
    """

    def __init__(
        self,
        field_name: str,
        type_annotation: Any | None = None,
        is_required: bool = True,
    ):
        self.field_name = field_name
        self.type_annotation = type_annotation
        self.is_required = is_required

    def __repr__(self) -> str:
        return f"ExternalFieldInfo(field_name={self.field_name!r}, required={self.is_required})"


class ExternalFieldManager:
    """Manages external fields in a type extension.

    Tracks which fields are external, validates field lists, and provides
    utilities for SDL generation and gateway communication.
    """

    def __init__(self):
        self.external_fields: dict[str, ExternalFieldInfo] = {}
        self.new_fields: set[str] = set()

    def mark_external(
        self,
        field_name: str,
        type_annotation: Any | None = None,
        is_required: bool = True,
    ) -> None:
        """Mark a field as external (defined in another subgraph).

        Args:
            field_name: Name of the field
            type_annotation: Python type annotation if available
            is_required: Whether field is required (non-null)
        """
        self.external_fields[field_name] = ExternalFieldInfo(
            field_name,
            type_annotation,
            is_required,
        )

    def mark_new(self, field_name: str) -> None:
        """Mark a field as new (defined in this subgraph).

        Args:
            field_name: Name of the field
        """
        self.new_fields.add(field_name)

    def get_external_fields(self) -> list[str]:
        """Get list of all external field names.

        Returns:
            List of external field names
        """
        return list(self.external_fields.keys())

    def get_new_fields(self) -> list[str]:
        """Get list of all new field names.

        Returns:
            List of new (local) field names
        """
        return sorted(self.new_fields)

    def is_external(self, field_name: str) -> bool:
        """Check if a field is external.

        Args:
            field_name: Name of the field

        Returns:
            True if field is external, False otherwise
        """
        return field_name in self.external_fields

    def is_new(self, field_name: str) -> bool:
        """Check if a field is new (local).

        Args:
            field_name: Name of the field

        Returns:
            True if field is new (local), False otherwise
        """
        return field_name in self.new_fields

    def validate_all_fields(self, all_field_names: set[str]) -> list[str]:
        """Validate that all fields are categorized.

        Checks that every field in the type is marked as either external or new.

        Args:
            all_field_names: Set of all field names in the type

        Returns:
            List of uncategorized field names (should be empty)

        Raises:
            ValueError: If there are uncategorized fields in strict mode
        """
        external = set(self.external_fields.keys())
        new = self.new_fields
        categorized = external | new

        uncategorized = all_field_names - categorized
        return sorted(uncategorized)

    def __repr__(self) -> str:
        return (
            f"ExternalFieldManager("
            f"external={len(self.external_fields)}, "
            f"new={len(self.new_fields)}"
            ")"
        )


def extract_external_fields(
    cls: type,
) -> tuple[dict[str, Any], set[str]]:
    """Extract external field information from a class.

    Examines the class to find fields marked with external() and builds
    a mapping of field names to their type annotations.

    Args:
        cls: The class to examine

    Returns:
        Tuple of (external_fields_dict, other_field_names_set)
        - external_fields_dict: Maps field name to type annotation
        - other_field_names_set: Names of fields not marked as external

    Example:
        from fraiseql.federation import extend_entity, external

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            reviews: list["Review"]

        external_map, others = extract_external_fields(Product)
        assert "id" in external_map
        assert "reviews" in others
    """
    from .decorators import _External

    external_fields_dict: dict[str, Any] = {}
    other_field_names: set[str] = set()

    # Get all annotations
    annotations = getattr(cls, "__annotations__", {})

    # Check class __dict__ for external() markers
    class_dict = cls.__dict__

    for field_name in annotations:
        if field_name in class_dict and isinstance(class_dict[field_name], _External):
            # This field is marked as external
            external_fields_dict[field_name] = annotations[field_name]
        else:
            # This field is not marked as external (it's new/local)
            other_field_names.add(field_name)

    return external_fields_dict, other_field_names
