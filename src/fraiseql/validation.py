"""Validation utilities for FraiseQL.

This module provides validation functions to help developers catch errors
early and ensure their queries are well-formed.
"""

from dataclasses import fields, is_dataclass
from typing import Any, Optional, Type, cast, get_args, get_origin, get_type_hints

from graphql import GraphQLResolveInfo

from .errors.exceptions import QueryValidationError, WhereClauseError


def validate_where_input(
    where_obj: Any,
    type_class: Type[Any],
    *,
    path: str = "where",
    strict: bool = False,
) -> list[str]:
    """Validate where input against type fields.

    This function checks that all fields referenced in a where clause
    exist on the target type and that operators are valid.

    Args:
        where_obj: The where input object to validate
        type_class: The FraiseQL type class to validate against
        path: Current path in the where object (for error messages)
        strict: If True, raise exception on first error; if False, collect all errors

    Returns:
        List of validation error messages (empty if valid)

    Raises:
        WhereClauseError: If strict=True and validation fails

    Example:
        >>> @fraise_type
        ... class User:
        ...     id: int
        ...     name: str
        ...     email: str
        ...
        >>> errors = validate_where_input({"name": {"_eq": "John"}}, User)
        >>> assert errors == []
        ...
        >>> errors = validate_where_input({"invalid": {"_eq": "value"}}, User)
        >>> assert "invalid" in errors[0]
    """
    errors = []

    if not where_obj:
        return errors

    if not isinstance(where_obj, dict):
        error = f"Where input at '{path}' must be a dictionary, got {type(where_obj).__name__}"
        if strict:
            raise WhereClauseError(error, where_input=where_obj)
        errors.append(error)
        return errors

    # Get available fields from the type
    available_fields = _get_type_fields(type_class)

    # Supported operators
    comparison_operators = {"_eq", "_neq", "_gt", "_gte", "_lt", "_lte"}
    string_operators = {"_like", "_ilike", "_contains", "_starts_with", "_ends_with"}
    array_operators = {"_in", "_nin"}
    null_operators = {"_is_null"}
    logical_operators = {"_and", "_or", "_not"}

    all_operators = (
        comparison_operators
        | string_operators
        | array_operators
        | null_operators
        | logical_operators
    )

    for key, value in where_obj.items():
        # Check logical operators
        if key in logical_operators:
            if key in {"_and", "_or"}:
                # These should contain arrays
                if not isinstance(value, list):
                    error = f"Operator '{key}' at '{path}.{key}' must contain an array"
                    if strict:
                        raise WhereClauseError(
                            error,
                            where_input=where_obj,
                            operator=key,
                            supported_operators=list(logical_operators),
                        )
                    errors.append(error)
                else:
                    # Recursively validate each condition
                    for i, condition in enumerate(value):
                        sub_errors = validate_where_input(
                            condition,
                            type_class,
                            path=f"{path}.{key}[{i}]",
                            strict=strict,
                        )
                        errors.extend(sub_errors)

            elif key == "_not":
                # _not contains a single condition
                sub_errors = validate_where_input(
                    value,
                    type_class,
                    path=f"{path}.{key}",
                    strict=strict,
                )
                errors.extend(sub_errors)

        # Check if it's a field name
        elif key not in all_operators:
            if key not in available_fields:
                # Check for case sensitivity issues
                lower_fields = {f.lower(): f for f in available_fields}
                if key.lower() in lower_fields:
                    suggestion = f"Did you mean '{lower_fields[key.lower()]}' instead of '{key}'?"
                    error = f"Unknown field '{key}' at '{path}'. {suggestion}"
                else:
                    fields_list = ", ".join(sorted(available_fields))
                    error = f"Unknown field '{key}' at '{path}'. Available fields: {fields_list}"

                if strict:
                    raise WhereClauseError(
                        error,
                        where_input=where_obj,
                        field_name=key,
                    )
                errors.append(error)

            # Validate operators for the field
            elif isinstance(value, dict):
                for op, op_value in value.items():
                    if op not in all_operators:
                        error = f"Unknown operator '{op}' for field '{key}' at '{path}.{key}'"
                        if strict:
                            raise WhereClauseError(
                                error,
                                where_input=where_obj,
                                field_name=key,
                                operator=op,
                                supported_operators=list(all_operators),
                            )
                        errors.append(error)

                    # Validate operator usage
                    field_type = _get_field_type(type_class, key)
                    if field_type:
                        op_errors = _validate_operator_for_type(
                            op,
                            op_value,
                            field_type,
                            f"{path}.{key}.{op}",
                        )
                        if op_errors and strict:
                            raise WhereClauseError(
                                op_errors[0],
                                where_input=where_obj,
                                field_name=key,
                                operator=op,
                            )
                        errors.extend(op_errors)

    return errors


def validate_selection_set(
    info: GraphQLResolveInfo,
    type_class: Optional[Type[Any]] = None,
    *,
    max_depth: int = 10,
    strict: bool = False,
) -> list[str]:
    """Validate that selected fields exist on type.

    This function checks that all fields requested in the GraphQL query
    exist on the target type and that the query doesn't exceed depth limits.

    Args:
        info: GraphQL resolve info containing the selection set
        type_class: The FraiseQL type to validate against (if None, uses return type)
        max_depth: Maximum allowed query depth
        strict: If True, raise exception on first error

    Returns:
        List of validation error messages (empty if valid)

    Raises:
        QueryValidationError: If strict=True and validation fails

    Example:
        >>> errors = validate_selection_set(info)
        >>> if errors:
        ...     print("Invalid fields:", errors)
    """
    errors = []

    # Extract type class from return type if not provided
    if type_class is None:
        # This would require more complex type extraction from GraphQL schema
        # For now, we'll skip validation if type_class is not provided
        return errors

    # Get selected fields from the query
    selected_fields = _extract_selected_fields(info)

    # Get available fields from the type
    available_fields = _get_type_fields(type_class)

    # Check each selected field
    invalid_fields = []
    for field_path in selected_fields:
        # For nested fields, only check the root field
        root_field = field_path.split(".")[0]

        if root_field not in available_fields and not root_field.startswith("__"):
            invalid_fields.append(root_field)

    if invalid_fields:
        error = f"Invalid fields requested: {', '.join(invalid_fields)}"
        if strict:
            raise QueryValidationError(
                error,
                type_name=type_class.__name__,
                invalid_fields=invalid_fields,
                valid_fields=list(available_fields),
                query_info=info,
            )
        errors.append(error)

    # Check query depth
    depth = _calculate_query_depth(info)
    if depth > max_depth:
        error = f"Query depth {depth} exceeds maximum allowed depth of {max_depth}"
        if strict:
            raise QueryValidationError(error, query_info=info)
        errors.append(error)

    return errors


def _get_type_fields(type_class: Type[Any]) -> set[str]:
    """Extract field names from a type class."""
    fields_set = set()

    # Handle dataclasses
    if is_dataclass(type_class):
        fields_set.update(f.name for f in fields(type_class))

    # Handle regular classes with type hints
    try:
        type_hints = get_type_hints(type_class)
        fields_set.update(type_hints.keys())
    except Exception:
        pass

    # Handle classes with __annotations__
    if hasattr(type_class, "__annotations__"):
        fields_set.update(type_class.__annotations__.keys())

    return fields_set


def _get_field_type(type_class: Type[Any], field_name: str) -> Optional[Type]:
    """Get the type of a specific field."""
    # Try type hints first
    try:
        type_hints = get_type_hints(type_class)
        if field_name in type_hints:
            return type_hints[field_name]
    except Exception:
        pass

    # Try dataclass fields
    if is_dataclass(type_class):
        for field in fields(type_class):
            if field.name == field_name:
                # Ensure we return a proper type object
                field_type = field.type
                if isinstance(field_type, type):
                    return field_type
                # Handle string annotations or forward references
                if isinstance(field_type, str):
                    return None
                return cast(Type[Any], field_type)

    # Try __annotations__
    if hasattr(type_class, "__annotations__") and field_name in type_class.__annotations__:
        return type_class.__annotations__[field_name]

    return None


def _validate_operator_for_type(
    operator: str,
    value: Any,
    field_type: Type,
    path: str,
) -> list[str]:
    """Validate that an operator is appropriate for a field type."""
    errors = []

    # Extract the base type if it's Optional or similar
    origin = get_origin(field_type)
    if origin is not None:
        args = get_args(field_type)
        if args:
            field_type = args[0]

    # String operators should only be used with string fields
    string_operators = {"_like", "_ilike", "_contains", "_starts_with", "_ends_with"}
    if operator in string_operators and field_type is not str:
        errors.append(
            f"String operator '{operator}' at '{path}' can only be used with string fields",
        )

    # Array operators need array values
    array_operators = {"_in", "_nin"}
    if operator in array_operators and not isinstance(value, list):
        errors.append(
            f"Array operator '{operator}' at '{path}' requires an array value",
        )

    # Null operator needs boolean value
    if operator == "_is_null" and not isinstance(value, bool):
        errors.append(
            f"Operator '_is_null' at '{path}' requires a boolean value",
        )

    return errors


def _extract_selected_fields(
    info: GraphQLResolveInfo,
    prefix: str = "",
    visited: Optional[set[str]] = None,
) -> set[str]:
    """Extract all selected field paths from GraphQL query."""
    if visited is None:
        visited = set()

    selected = set()

    if not info.field_nodes:
        return selected

    for field_node in info.field_nodes:
        if field_node.selection_set:
            for selection in field_node.selection_set.selections:
                if hasattr(selection, "name"):
                    field_name = selection.name.value
                    field_path = f"{prefix}.{field_name}" if prefix else field_name

                    # Avoid infinite recursion
                    if field_path not in visited:
                        visited.add(field_path)
                        selected.add(field_path)

                        # Recursively process nested selections
                        if hasattr(selection, "selection_set") and selection.selection_set:
                            # This would require more complex processing
                            # For now, we just track the field paths
                            pass

    return selected


def _calculate_query_depth(info: GraphQLResolveInfo) -> int:
    """Calculate the maximum depth of a GraphQL query."""

    def _get_depth(selection_set: Any, current_depth: int = 0) -> int:
        if not selection_set:
            return current_depth

        max_depth = current_depth

        for selection in selection_set.selections:
            if hasattr(selection, "selection_set") and selection.selection_set:
                depth = _get_depth(selection.selection_set, current_depth + 1)
                max_depth = max(max_depth, depth)

        return max_depth

    max_depth = 0
    for field_node in info.field_nodes:
        if field_node.selection_set:
            depth = _get_depth(field_node.selection_set, 1)
            max_depth = max(max_depth, depth)

    return max_depth


def validate_query_complexity(
    info: GraphQLResolveInfo,
    *,
    max_complexity: int = 1000,
    field_costs: Optional[dict[str, int]] = None,
) -> tuple[int, list[str]]:
    """Calculate and validate query complexity.

    This function calculates a complexity score for a GraphQL query
    based on the number of fields and their costs.

    Args:
        info: GraphQL resolve info
        max_complexity: Maximum allowed complexity score
        field_costs: Custom costs for specific fields (default: 1 per field)

    Returns:
        Tuple of (complexity_score, list_of_errors)

    Example:
        >>> complexity, errors = validate_query_complexity(info, max_complexity=100)
        >>> if errors:
        ...     raise QueryValidationError(f"Query too complex: {errors[0]}")
    """
    if field_costs is None:
        field_costs = {}

    # Default costs
    default_field_cost = 1
    list_multiplier = 10  # Lists are more expensive

    complexity = 0
    errors = []

    def _calculate_complexity(selection_set: Any, parent_multiplier: int = 1) -> int:
        if not selection_set:
            return 0

        total = 0

        for selection in selection_set.selections:
            if hasattr(selection, "name"):
                field_name = selection.name.value

                # Get field cost
                cost = field_costs.get(field_name, default_field_cost)

                # Apply parent multiplier (for nested lists)
                cost *= parent_multiplier

                total += cost

                # Process nested selections
                if hasattr(selection, "selection_set") and selection.selection_set:
                    # Check if this field returns a list
                    # This is simplified - in real implementation you'd check the schema
                    multiplier = list_multiplier if field_name.endswith("s") else 1
                    total += _calculate_complexity(
                        selection.selection_set,
                        parent_multiplier * multiplier,
                    )

        return total

    # Calculate complexity
    for field_node in info.field_nodes:
        if field_node.selection_set:
            complexity += _calculate_complexity(field_node.selection_set)

    # Validate complexity
    if complexity > max_complexity:
        errors.append(
            f"Query complexity {complexity} exceeds maximum allowed complexity of {max_complexity}",
        )

    return complexity, errors
