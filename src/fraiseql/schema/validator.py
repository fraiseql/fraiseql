"""Schema validation for FraiseQL requirements."""

import types
from typing import Any, List, Type


class SchemaValidator:
    """Validate FraiseQL schemas conform to requirements."""

    @staticmethod
    def validate_mutation_types(
        mutation_name: str,
        success_type: Type,
        error_type: Type,
    ) -> List[str]:
        """Validate mutation types conform to requirements.

        Returns:
            List of validation errors (empty if valid)
        """
        errors = []

        # Validate Success type
        errors.extend(SchemaValidator._validate_success_type(mutation_name, success_type))

        # Validate Error type
        errors.extend(SchemaValidator._validate_error_type(error_type))

        return errors

    @staticmethod
    def _validate_success_type(
        mutation_name: str,
        success_type: Type,
    ) -> List[str]:
        """Validate Success type requirements."""
        errors = []

        if not hasattr(success_type, "__annotations__"):
            errors.append(f"{success_type.__name__}: Missing annotations")
            return errors

        annotations = success_type.__annotations__

        # Create temporary schema to use _is_entity_field method
        # Import here to avoid circular imports
        from .mutation_schema_generator import MutationSchema

        temp_schema = MutationSchema(
            mutation_name=mutation_name,
            success_type=success_type,
            error_type=type("TempError", (), {}),  # Dummy error type
            union_type=None,  # Temporary
        )

        # Find entity field
        entity_field = None
        for field in annotations:
            if temp_schema._is_entity_field(field):
                entity_field = field
                break

        if not entity_field:
            errors.append(
                f"{success_type.__name__}: Missing entity field. "
                f"Expected 'entity', field derived from mutation name, or common entity name."
            )
            return errors

        # Check entity is non-nullable
        entity_type = annotations[entity_field]
        if _is_optional(entity_type):
            errors.append(
                f"{success_type.__name__}.{entity_field}: Must be non-null. "
                f"Got '{entity_type}'. Remove Optional or '| None'."
            )

        return errors

    @staticmethod
    def _validate_error_type(error_type: Type) -> List[str]:
        """Validate Error type requirements."""
        errors = []

        if not hasattr(error_type, "__annotations__"):
            errors.append(f"{error_type.__name__}: Missing annotations")
            return errors

        annotations = error_type.__annotations__

        # Required fields
        required_fields = {
            "code": int,
            "status": str,
            "message": str,
        }

        for field_name, expected_type in required_fields.items():
            if field_name not in annotations:
                errors.append(
                    f"{error_type.__name__}: Missing required field '{field_name}: {expected_type.__name__}'"  # noqa: E501
                )
            else:
                actual_type = annotations[field_name]
                # Basic type check (doesn't handle complex generics)
                if (
                    actual_type != expected_type
                    and getattr(actual_type, "__origin__", None) != expected_type
                ):
                    errors.append(
                        f"{error_type.__name__}.{field_name}: Wrong type. "
                        f"Expected '{expected_type.__name__}', got '{actual_type}'"
                    )

        return errors


def _is_optional(type_hint: Any) -> bool:
    """Check if type hint is Optional."""
    import typing

    origin = typing.get_origin(type_hint)
    if origin is typing.Union or origin is types.UnionType:
        args = typing.get_args(type_hint)
        return type(None) in args
    return False
