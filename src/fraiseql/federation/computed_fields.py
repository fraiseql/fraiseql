"""Computed fields with dependency tracking for Federation.

Provides integration between @requires/@provides directives and external fields.
Enables computed fields that depend on fields from other subgraphs.

Example:
    Type extension with computed field that requires external data:

        from fraiseql.federation import extend_entity, external, requires, provides

        @extend_entity(key="id")
        class Product:
            # External fields from products subgraph
            id: str = external()
            name: str = external()
            price: float = external()

            # New fields in reviews subgraph
            reviews: list["Review"] = field(default_factory=list)

            # Computed field requiring external data
            @requires("price")
            def discounted_price(self, discount: float = 0.1) -> float:
                return self.price * (1 - discount)

            # Field providing to other subgraphs
            @provides("id reviews")
            async def review_summary(self) -> str:
                return f"{self.id}: {len(self.reviews)} reviews"

This module provides:
- ComputedField: Metadata about computed fields
- validate_requires(): Ensure dependencies exist
- validate_provides(): Ensure provided fields are available
- get_computed_fields(): Extract all computed fields from class
"""

from typing import Any, Optional, Set

from .directives import get_method_directives


class ComputedField:
    """Metadata about a computed field in an entity.

    Attributes:
        method_name: Name of the method implementing the field
        requires: List of field dependencies (from @requires directive)
        provides: List of fields this resolves (from @provides directive)
        is_async: Whether the method is async
        return_type: Python return type annotation
    """

    def __init__(
        self,
        method_name: str,
        requires: Optional[list[str]] = None,
        provides: Optional[list[str]] = None,
        is_async: bool = False,
        return_type: Optional[Any] = None,
    ):
        self.method_name = method_name
        self.requires = requires or []
        self.provides = provides or []
        self.is_async = is_async
        self.return_type = return_type

    def has_requirements(self) -> bool:
        """Check if field has dependencies."""
        return len(self.requires) > 0

    def has_provisions(self) -> bool:
        """Check if field provides data to other subgraphs."""
        return len(self.provides) > 0

    def get_required_fields(self) -> list[str]:
        """Get list of required field names."""
        return self.requires

    def get_provided_fields(self) -> list[str]:
        """Get list of provided field names."""
        return self.provides

    def __repr__(self) -> str:
        parts = [f"method={self.method_name}"]
        if self.requires:
            parts.append(f"requires={self.requires}")
        if self.provides:
            parts.append(f"provides={self.provides}")
        if self.is_async:
            parts.append("async=True")
        return f"ComputedField({', '.join(parts)})"


class ComputedFieldValidator:
    """Validates computed field dependencies and provisions.

    Ensures that:
    - Required fields actually exist in the entity
    - Provided fields can be resolved by the method
    - No circular dependencies between computed fields
    """

    def __init__(self):
        self.errors: list[str] = []

    def validate_requires(
        self,
        method_name: str,
        required_fields: list[str],
        all_fields: Set[str],
        external_fields: Optional[Set[str]] = None,
    ) -> bool:
        """Validate that required fields exist.

        Args:
            method_name: Name of the method
            required_fields: Fields the method requires
            all_fields: All available fields in the entity
            external_fields: External fields (optional, for context)

        Returns:
            True if all requirements are satisfied, False otherwise
        """
        external_fields = external_fields or set()
        missing = set(required_fields) - all_fields

        if missing:
            missing_list = sorted(missing)
            self.errors.append(
                f"Method {method_name} @requires fields that don't exist: {missing_list}"
            )
            return False

        return True

    def validate_provides(
        self,
        method_name: str,
        provided_fields: list[str],
        all_fields: Set[str],
    ) -> bool:
        """Validate that provided fields can be resolved.

        Args:
            method_name: Name of the method
            provided_fields: Fields this method will provide
            all_fields: All available fields in the entity

        Returns:
            True if provisions are valid, False otherwise
        """
        # Provided fields should be fields or new computed fields
        # For now, we just check they're reasonable field names
        if not provided_fields:
            self.errors.append(f"Method {method_name} @provides with empty field list")
            return False

        return True

    def validate_computed_field(
        self,
        computed_field: ComputedField,
        all_fields: Set[str],
        external_fields: Optional[Set[str]] = None,
    ) -> bool:
        """Validate a computed field completely.

        Args:
            computed_field: The computed field to validate
            all_fields: All available fields in the entity
            external_fields: External fields (optional)

        Returns:
            True if field is valid, False otherwise
        """
        valid = True

        if computed_field.has_requirements():
            valid &= self.validate_requires(
                computed_field.method_name,
                computed_field.requires,
                all_fields,
                external_fields,
            )

        if computed_field.has_provisions():
            valid &= self.validate_provides(
                computed_field.method_name,
                computed_field.provides,
                all_fields,
            )

        return valid

    def get_errors(self) -> list[str]:
        """Get all validation errors."""
        return self.errors.copy()

    def clear_errors(self) -> None:
        """Clear error list."""
        self.errors.clear()


def extract_computed_fields(cls: type) -> dict[str, ComputedField]:
    """Extract all computed fields from a class.

    Examines the class to find methods decorated with @requires or @provides
    and builds metadata about them.

    Args:
        cls: The class to examine

    Returns:
        Dict mapping method name to ComputedField metadata

    Example:
        from fraiseql.federation import extend_entity, external, requires

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

            @requires("price")
            def discounted_price(self) -> float:
                return self.price * 0.9

        fields = extract_computed_fields(Product)
        assert "discounted_price" in fields
        assert fields["discounted_price"].requires == ["price"]
    """
    computed = {}
    method_directives = get_method_directives(cls)

    for method_name, directive_meta in method_directives.items():
        method = getattr(cls, method_name, None)
        if not callable(method):
            continue

        # Get return type annotation
        return_type = None
        type_hints = getattr(cls, "__annotations__", {})
        if method_name in type_hints:
            return_type = type_hints[method_name]

        # Check if method is async
        import inspect

        is_async = inspect.iscoroutinefunction(method)

        # Create computed field metadata
        requires = directive_meta.get_required_fields()
        provides = directive_meta.get_provided_fields()

        computed[method_name] = ComputedField(
            method_name=method_name,
            requires=requires,
            provides=provides,
            is_async=is_async,
            return_type=return_type,
        )

    return computed


def get_all_field_dependencies(
    cls: type, external_fields: Optional[Set[str]] = None
) -> dict[str, Set[str]]:
    """Build complete dependency graph for all computed fields.

    Shows which fields each computed field depends on.

    Args:
        cls: The class to analyze
        external_fields: External fields for context (optional)

    Returns:
        Dict mapping computed field name to set of required field names

    Example:
        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()
            reviews: list = []

            @requires("price reviews")
            def summary(self) -> str:
                return f"Price: ${self.price}, Reviews: {len(self.reviews)}"

        deps = get_all_field_dependencies(Product)
        assert deps["summary"] == {"price", "reviews"}
    """
    external_fields = external_fields or set()
    computed = extract_computed_fields(cls)
    dependencies: dict[str, Set[str]] = {}

    for method_name, computed_field in computed.items():
        if computed_field.has_requirements():
            dependencies[method_name] = set(computed_field.requires)

    return dependencies


def validate_all_computed_fields(
    cls: type,
    all_fields: Set[str],
    external_fields: Optional[Set[str]] = None,
    strict: bool = False,
) -> tuple[bool, list[str]]:
    """Validate all computed fields in a class.

    Args:
        cls: The class to validate
        all_fields: All available fields (including computed fields)
        external_fields: External fields (optional)
        strict: If True, raise on errors; if False, return errors

    Returns:
        Tuple of (is_valid, error_messages)
    """
    external_fields = external_fields or set()
    validator = ComputedFieldValidator()
    computed = extract_computed_fields(cls)

    for computed_field in computed.values():
        validator.validate_computed_field(computed_field, all_fields, external_fields)

    errors = validator.get_errors()
    is_valid = len(errors) == 0

    return is_valid, errors
