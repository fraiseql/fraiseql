"""Mutation return type validation against GraphQL schema.

Validates that mutation return values match the schema's expected return type,
catching type mismatches, missing required fields, and structural errors
at build time or in CI rather than at runtime.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from graphql import (
    GraphQLEnumType,
    GraphQLField,
    GraphQLList,
    GraphQLNonNull,
    GraphQLObjectType,
    GraphQLScalarType,
    GraphQLSchema,
    GraphQLUnionType,
)


@dataclass
class ValidationError:
    """A single validation error for a mutation return value."""

    field_path: str
    message: str
    expected_type: str


@dataclass
class ValidationResult:
    """Result of validating a mutation return value against the schema."""

    is_valid: bool
    errors: list[ValidationError] = field(default_factory=list)
    matched_type: str | None = None


# Mapping from GraphQL scalar names to Python types
_SCALAR_TYPE_MAP: dict[str, tuple[type, ...]] = {
    "String": (str,),
    "Int": (int,),
    "Float": (int, float),
    "Boolean": (bool,),
    "ID": (str, int),
}


def validate_mutation_return(
    schema: GraphQLSchema,
    mutation_name: str,
    return_value: dict,
) -> ValidationResult:
    """Validate a mutation return value against the schema's expected type.

    Args:
        schema: A GraphQL schema (from graphql-core's build_schema or runtime).
        mutation_name: The name of the mutation to validate against.
        return_value: The dict return value to validate.

    Returns:
        A ValidationResult indicating whether the value is valid.
    """
    mutation_type = schema.mutation_type
    if mutation_type is None:
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path="",
                    message="Schema has no Mutation type defined",
                    expected_type="Mutation",
                )
            ],
        )

    fields = mutation_type.fields
    if mutation_name not in fields:
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path="",
                    message=f"Mutation '{mutation_name}' not found in schema",
                    expected_type="",
                )
            ],
        )

    mutation_field: GraphQLField = fields[mutation_name]
    return_type = mutation_field.type

    return _validate_type(return_value, return_type, "")


def _validate_type(
    value: object,
    graphql_type: object,
    path: str,
) -> ValidationResult:
    """Recursively validate a value against a GraphQL type."""
    # Unwrap NonNull
    if isinstance(graphql_type, GraphQLNonNull):
        if value is None:
            return ValidationResult(
                is_valid=False,
                errors=[
                    ValidationError(
                        field_path=path or "(root)",
                        message="Value is null but field is non-nullable",
                        expected_type=f"{graphql_type.of_type}!",
                    )
                ],
            )
        return _validate_type(value, graphql_type.of_type, path)

    # Nullable field with null value is always valid
    if value is None:
        return ValidationResult(is_valid=True)

    # Union type — try each member
    if isinstance(graphql_type, GraphQLUnionType):
        return _validate_union(value, graphql_type, path)

    # List type
    if isinstance(graphql_type, GraphQLList):
        return _validate_list(value, graphql_type, path)

    # Scalar type
    if isinstance(graphql_type, GraphQLScalarType):
        return _validate_scalar(value, graphql_type, path)

    # Enum type
    if isinstance(graphql_type, GraphQLEnumType):
        return _validate_enum(value, graphql_type, path)

    # Object type
    if isinstance(graphql_type, GraphQLObjectType):
        return _validate_object(value, graphql_type, path)

    return ValidationResult(is_valid=True)


def _validate_scalar(
    value: object,
    scalar_type: GraphQLScalarType,
    path: str,
) -> ValidationResult:
    """Validate a value against a scalar type."""
    expected_types = _SCALAR_TYPE_MAP.get(scalar_type.name)
    if expected_types is None:
        # Custom scalar — accept any value
        return ValidationResult(is_valid=True)

    if not isinstance(value, expected_types):
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"Expected {scalar_type.name}, got {type(value).__name__}",
                    expected_type=scalar_type.name,
                )
            ],
        )
    return ValidationResult(is_valid=True)


def _validate_enum(
    value: object,
    enum_type: GraphQLEnumType,
    path: str,
) -> ValidationResult:
    """Validate a value against an enum type."""
    valid_values = set(enum_type.values.keys())
    if value not in valid_values:
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"Value '{value}' is not a valid {enum_type.name} enum value. "
                    f"Valid values: {', '.join(sorted(valid_values))}",
                    expected_type=enum_type.name,
                )
            ],
        )
    return ValidationResult(is_valid=True)


def _validate_list(
    value: object,
    list_type: GraphQLList,
    path: str,
) -> ValidationResult:
    """Validate a value against a list type."""
    if not isinstance(value, list):
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"Expected list, got {type(value).__name__}",
                    expected_type=f"[{list_type.of_type}]",
                )
            ],
        )

    all_errors: list[ValidationError] = []
    for i, item in enumerate(value):
        item_path = f"{path}[{i}]" if path else f"[{i}]"
        result = _validate_type(item, list_type.of_type, item_path)
        if not result.is_valid:
            all_errors.extend(result.errors)

    if all_errors:
        return ValidationResult(is_valid=False, errors=all_errors)
    return ValidationResult(is_valid=True)


def _validate_object(
    value: object,
    object_type: GraphQLObjectType,
    path: str,
) -> ValidationResult:
    """Validate a value against an object type."""
    if not isinstance(value, dict):
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"Expected object, got {type(value).__name__}",
                    expected_type=object_type.name,
                )
            ],
        )

    all_errors: list[ValidationError] = []

    for field_name, field_def in object_type.fields.items():
        field_path = f"{path}.{field_name}" if path else field_name

        if field_name not in value:
            # Check if field is required (NonNull)
            if isinstance(field_def.type, GraphQLNonNull):
                all_errors.append(
                    ValidationError(
                        field_path=field_path,
                        message=f"Missing required field '{field_name}'",
                        expected_type=str(field_def.type),
                    )
                )
            continue

        field_value = value[field_name]
        result = _validate_type(field_value, field_def.type, field_path)
        if not result.is_valid:
            all_errors.extend(result.errors)

    if all_errors:
        return ValidationResult(is_valid=False, errors=all_errors)
    return ValidationResult(is_valid=True, matched_type=object_type.name)


def _validate_union(
    value: object,
    union_type: GraphQLUnionType,
    path: str,
) -> ValidationResult:
    """Validate a value against a union type by trying each member."""
    if not isinstance(value, dict):
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"Expected object for union type, got {type(value).__name__}",
                    expected_type=union_type.name,
                )
            ],
        )

    # If __typename is provided, validate against that specific type
    typename = value.get("__typename")
    if typename:
        for member in union_type.types:
            if member.name == typename:
                result = _validate_type(value, member, path)
                if result.is_valid:
                    result.matched_type = member.name
                return result
        return ValidationResult(
            is_valid=False,
            errors=[
                ValidationError(
                    field_path=path or "(root)",
                    message=f"__typename '{typename}' is not a member of union {union_type.name}. "
                    f"Valid types: {', '.join(t.name for t in union_type.types)}",
                    expected_type=union_type.name,
                )
            ],
        )

    # No __typename — try each member, accept if any matches
    best_result: ValidationResult | None = None
    for member in union_type.types:
        result = _validate_type(value, member, path)
        if result.is_valid:
            result.matched_type = member.name
            return result
        # Track the result with fewest errors as best candidate
        if best_result is None or len(result.errors) < len(best_result.errors):
            best_result = result

    # None matched — return error with details about all attempts
    member_names = ", ".join(t.name for t in union_type.types)
    errors = [
        ValidationError(
            field_path=path or "(root)",
            message=f"Value does not match any member of union {union_type.name} "
            f"(tried: {member_names}). Consider adding '__typename' to disambiguate.",
            expected_type=union_type.name,
        )
    ]
    if best_result:
        errors.extend(best_result.errors)
    return ValidationResult(is_valid=False, errors=errors)
