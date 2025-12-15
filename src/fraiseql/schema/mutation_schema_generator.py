"""GraphQL schema generation for FraiseQL mutations.

Generates union types for all mutations.
"""

import types
from dataclasses import dataclass
from typing import Annotated, Any, Type

from fraiseql.mutations.decorators import FraiseUnion


@dataclass
class MutationSchema:
    """Schema definition for a mutation.

    All mutations return union types.
    """

    mutation_name: str
    success_type: Type
    error_type: Type
    union_type: Any  # FraiseQL union type

    def to_graphql_sdl(self) -> str:
        """Generate GraphQL SDL for this mutation.

        Returns:
            GraphQL SDL string including union type
        """
        success_name = self.success_type.__name__
        error_name = self.error_type.__name__
        union_name = f"{self.mutation_name}Result"

        return f"""
union {union_name} = {success_name} | {error_name}

type {success_name} {{
  {self._generate_success_fields()}
}}

type {error_name} {{
  {self._generate_error_fields()}
}}

extend type Mutation {{
  {self._to_camel_case(self.mutation_name)}(input: {self.mutation_name}Input!): {union_name}!
}}
"""

    def _generate_success_fields(self) -> str:
        """Generate GraphQL fields for Success type.

        Entity field is always non-nullable.
        """
        fields = []
        annotations = self.success_type.__annotations__

        for field_name, field_type in annotations.items():
            graphql_type = self._python_type_to_graphql(field_type)

            # Entity field must be non-nullable
            if self._is_entity_field(field_name):
                if graphql_type.endswith("!"):
                    fields.append(f"  {self._to_camel_case(field_name)}: {graphql_type}")
                else:
                    # Force non-nullable
                    fields.append(f"  {self._to_camel_case(field_name)}: {graphql_type}!")
            else:
                fields.append(f"  {self._to_camel_case(field_name)}: {graphql_type}")

        return "\n".join(fields)

    def _generate_error_fields(self) -> str:
        """Generate GraphQL fields for Error type.

        Must include code: Int! field.
        """
        fields = []
        annotations = self.error_type.__annotations__

        # Ensure code field exists
        if "code" not in annotations:
            raise ValueError(
                f"Error type {self.error_type.__name__} missing required 'code: int' field. "
                f"Error types must include REST-like error codes."
            )

        for field_name, field_type in annotations.items():
            graphql_type = self._python_type_to_graphql(field_type)

            # code, status, message are required
            if field_name in ("code", "status", "message"):  # noqa: SIM102
                if not graphql_type.endswith("!"):
                    graphql_type += "!"

            fields.append(f"  {self._to_camel_case(field_name)}: {graphql_type}")

        return "\n".join(fields)

    def _is_entity_field(self, field_name: str) -> bool:
        """Check if field is the entity field.

        Entity field detection patterns (checked in order):
        1. Exact match: "entity"
        2. Mutation name match: "CreateMachine" → "machine" or "createmachine"
        3. Pluralized mutation name: "CreateMachine" → "machines"
        4. Common entity field names: "result", "data", "item"

        Examples:
            CreateMachine → "machine" matches
            CreatePost → "post" matches
            DeleteMachine → "machine" matches
            UpdateUser → "user" matches
            CreateMachines → "machines" matches (plural)
        """
        field_lower = field_name.lower()

        # Pattern 1: Exact "entity"
        if field_lower == "entity":
            return True

        # Pattern 2: Extract entity name from mutation name
        # "CreateMachine" → "machine", "DeletePost" → "post"
        mutation_lower = self.mutation_name.lower()

        # Remove common prefixes
        for prefix in ["create", "update", "delete", "upsert", "remove", "add"]:
            if mutation_lower.startswith(prefix):
                entity_name = mutation_lower[len(prefix) :]
                if field_lower == entity_name:
                    return True
                # Check plural: "machines" for "CreateMachines"
                if field_lower == entity_name + "s":
                    return True
                # Check singular: "machine" for "CreateMachines"
                if entity_name.endswith("s") and field_lower == entity_name[:-1]:
                    return True
                break

        # Pattern 3: Full mutation name match (fallback)
        if field_lower == mutation_lower:
            return True

        # Pattern 4: Common entity field names
        common_entity_names = ["result", "data", "item", "record"]
        if field_lower in common_entity_names:
            return True

        return False

    def _python_type_to_graphql(self, python_type: Any) -> str:
        """Convert Python type hint to GraphQL type string.

        Supports:
        - Basic types: int, str, bool, float
        - Optional types: X | None, Optional[X]
        - List types: list[X], List[X]
        - Dict types: dict[str, X] → JSON
        - Custom types: Machine, Cascade, etc.
        - Nested types: list[Machine | None], dict[str, list[int]]

        Args:
            python_type: Python type annotation

        Returns:
            GraphQL type string (e.g., "String!", "Machine", "[Machine!]!")

        Examples:
            int → "Int!"
            str → "String!"
            Machine → "Machine"
            Machine | None → "Machine"  (nullable)
            list[Machine] → "[Machine!]!"
            list[Machine | None] → "[Machine]!"  (nullable items)
            list[Machine] | None → "[Machine!]"  (nullable list)
            dict[str, Any] → "JSON"
        """
        import typing

        # Handle None type explicitly
        if python_type is type(None):
            raise ValueError("Cannot convert None type to GraphQL (use Optional or | None)")

        # Get origin and args for generic types
        origin = typing.get_origin(python_type)
        args = typing.get_args(python_type)

        # Handle Optional types (X | None or Optional[X])
        if origin is typing.Union or origin is types.UnionType:
            # Filter out None
            non_none_args = [arg for arg in args if arg is not type(None)]

            if len(non_none_args) == 0:
                raise ValueError("Union type must have at least one non-None type")

            if len(non_none_args) == 1:
                # Optional[X] → X (nullable, no "!")
                inner_type = self._python_type_to_graphql(non_none_args[0])
                # Remove trailing "!" to make nullable
                return inner_type.rstrip("!")

            # Multiple non-None types: not supported
            raise ValueError(
                f"Union types with multiple non-None types not supported: {python_type}. "
                f"GraphQL unions require separate type definitions."
            )

        # Handle basic types
        if python_type is int:
            return "Int!"
        if python_type is str:
            return "String!"
        if python_type is bool:
            return "Boolean!"
        if python_type is float:
            return "Float!"

        # Handle list types (list[X] or List[X])
        if origin is list or python_type is list:
            if not args and python_type is list:
                # Bare list without type parameter
                raise ValueError(
                    "List type must have element type: use list[X] instead of bare 'list'"
                )

            element_type = args[0]
            inner = self._python_type_to_graphql(element_type)

            # List items are non-null by default, list itself is non-null
            # list[Machine] → "[Machine!]!"
            # list[Machine | None] → "[Machine]!" (nullable items)
            if inner.endswith("!"):
                # Non-null items: [Machine!]!
                return f"[{inner}]!"
            # Nullable items: [Machine]!
            return f"[{inner}]!"

        # Handle dict types → JSON scalar
        if origin is dict:
            # dict[str, Any] → JSON
            # dict[str, int] → JSON (we lose type info but GraphQL doesn't support typed dicts)
            return "JSON"  # Assumes JSON scalar is registered

        # Handle custom types (dataclasses, Pydantic models, etc.)
        if hasattr(python_type, "__name__"):
            # Assume this is a GraphQL type with the same name
            # Machine → "Machine!" (non-nullable by default for custom types)
            return python_type.__name__ + "!"

        # Handle typing module generics without origin (rare edge case)
        if hasattr(python_type, "__origin__"):
            # This shouldn't happen if we've covered all cases above
            raise ValueError(
                f"Unsupported typing construct: {python_type}. "
                f"Origin: {typing.get_origin(python_type)}"
            )

        # Fallback for unknown types
        raise ValueError(
            f"Cannot convert Python type to GraphQL: {python_type}. "
            f"Supported types: int, str, bool, float, list[X], dict, Optional[X], custom classes."
        )

    def _to_camel_case(self, snake_str: str) -> str:
        """Convert snake_case to camelCase."""
        components = snake_str.split("_")
        return components[0] + "".join(x.title() for x in components[1:])


def generate_mutation_schema(
    mutation_name: str,
    success_type: Type,
    error_type: Type,
) -> MutationSchema:
    """Generate schema for a mutation.

    Creates union type automatically.

    Args:
        mutation_name: Name of the mutation (e.g., "CreateMachine")
        success_type: Success type class
        error_type: Error type class

    Returns:
        MutationSchema with union type

    Raises:
        ValueError: If types don't conform to requirements
    """
    # Validate Success type
    if not hasattr(success_type, "__annotations__"):
        raise ValueError(f"Success type {success_type.__name__} must have annotations")

    success_annotations = success_type.__annotations__

    # Create temporary schema to use _is_entity_field method
    temp_schema = MutationSchema(
        mutation_name=mutation_name,
        success_type=success_type,
        error_type=error_type,
        union_type=None,  # Temporary
    )

    # Find entity field
    entity_field = None
    for field in success_annotations:
        if temp_schema._is_entity_field(field):
            entity_field = field
            break

    if not entity_field:
        raise ValueError(
            f"Success type {success_type.__name__} must have entity field. "
            f"Expected 'entity', field derived from mutation name, or common entity name."
        )

    # Ensure entity is non-nullable
    entity_type = success_annotations[entity_field]
    if _is_optional(entity_type):
        raise ValueError(
            f"Success type {success_type.__name__} has nullable entity field '{entity_field}'. "
            f"Entity must be non-null in Success types. "
            f"Change type from '{entity_type}' to non-nullable."
        )

    # Validate Error type
    if not hasattr(error_type, "__annotations__"):
        raise ValueError(f"Error type {error_type.__name__} must have annotations")

    error_annotations = error_type.__annotations__

    # Ensure code field exists
    if "code" not in error_annotations:
        raise ValueError(
            f"Error type {error_type.__name__} must have 'code: int' field. "
            f"Error types must include REST-like error codes."
        )

    # Ensure status field exists
    if "status" not in error_annotations:
        raise ValueError(f"Error type {error_type.__name__} must have 'status: str' field.")

    # Ensure message field exists
    if "message" not in error_annotations:
        raise ValueError(f"Error type {error_type.__name__} must have 'message: str' field.")

    # Create union type using FraiseQL's union system
    union_name = f"{mutation_name}Result"
    union_type = Annotated[success_type | error_type, FraiseUnion(union_name)]

    return MutationSchema(
        mutation_name=mutation_name,
        success_type=success_type,
        error_type=error_type,
        union_type=union_type,
    )


def _is_optional(type_hint: Any) -> bool:
    """Check if type hint is Optional (includes None)."""
    import typing

    # Check for X | None (Python 3.10+)
    origin = typing.get_origin(type_hint)
    if origin is typing.Union or origin is types.UnionType:
        args = typing.get_args(type_hint)
        return type(None) in args

    return False
