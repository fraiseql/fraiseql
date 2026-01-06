# Phase 3: Schema & GraphQL Generation

**Timeline:** Week 2, Days 1-3
**Risk Level:** MEDIUM (schema generation changes)
**Dependencies:** Phases 1-2
**Blocking:** Phases 4-5

---

## Objective

Update GraphQL schema generation to:
1. Generate union types for all mutations (`<Mutation>Result = Success | Error`)
2. Ensure Success types have non-nullable entity fields
3. Ensure Error types include `code: Int!` field
4. Update introspection/schema reflection
5. Handle backward compatibility for existing schemas

---

## Files to Modify

### Critical Files
1. `src/fraiseql/schema/mutation_schema_generator.py` - Mutation schema generation
2. `src/fraiseql/schema/types.py` - Type definitions for schema
3. `src/fraiseql/graphql/schema_builder.py` - GraphQL schema building

### Supporting Files
4. `src/fraiseql/schema/__init__.py` - Exports
5. `tests/integration/graphql/mutations/test_schema_generation.py` - Schema tests

---

## Implementation Steps

### Step 3.1: Update Mutation Schema Generator

**File:** `src/fraiseql/schema/mutation_schema_generator.py`

**Add union type generation:**

```python
from typing import Type, Any
from dataclasses import dataclass
import strawberry

@dataclass
class MutationSchema:
    """Schema definition for a mutation.

    v1.8.0: All mutations return union types.
    """
    mutation_name: str
    success_type: Type
    error_type: Type
    union_type: Type  # NEW in v1.8.0

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

        v1.8.0: Entity field is always non-nullable.
        """
        fields = []
        annotations = self.success_type.__annotations__

        for field_name, field_type in annotations.items():
            graphql_type = self._python_type_to_graphql(field_type)

            # v1.8.0: Entity field must be non-nullable
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

        v1.8.0: Must include code: Int! field.
        """
        fields = []
        annotations = self.error_type.__annotations__

        # Ensure code field exists
        if "code" not in annotations:
            raise ValueError(
                f"Error type {self.error_type.__name__} missing required 'code: int' field. "
                f"v1.8.0 requires Error types to include REST-like error codes."
            )

        for field_name, field_type in annotations.items():
            graphql_type = self._python_type_to_graphql(field_type)

            # code, status, message are required
            if field_name in ("code", "status", "message"):
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
                entity_name = mutation_lower[len(prefix):]
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
        if origin is typing.Union:
            # Filter out None
            non_none_args = [arg for arg in args if arg is not type(None)]

            if len(non_none_args) == 0:
                raise ValueError("Union type must have at least one non-None type")

            if len(non_none_args) == 1:
                # Optional[X] → X (nullable, no "!")
                inner_type = self._python_type_to_graphql(non_none_args[0])
                # Remove trailing "!" to make nullable
                return inner_type.rstrip("!")

            # Multiple non-None types: not supported in v1.8.0
            raise ValueError(
                f"Union types with multiple non-None types not supported: {python_type}. "
                f"GraphQL unions require separate type definitions."
            )

        # Handle basic types
        if python_type == int:
            return "Int!"
        if python_type == str:
            return "String!"
        if python_type == bool:
            return "Boolean!"
        if python_type == float:
            return "Float!"

        # Handle list types (list[X] or List[X])
        if origin is list:
            if not args:
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
            else:
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
            # Machine → "Machine" (nullable by default for custom types)
            return python_type.__name__

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
        components = snake_str.split('_')
        return components[0] + ''.join(x.title() for x in components[1:])


def generate_mutation_schema(
    mutation_name: str,
    success_type: Type,
    error_type: Type,
) -> MutationSchema:
    """Generate schema for a mutation.

    v1.8.0: Creates union type automatically.

    Args:
        mutation_name: Name of the mutation (e.g., "CreateMachine")
        success_type: Success type class
        error_type: Error type class

    Returns:
        MutationSchema with union type

    Raises:
        ValueError: If types don't conform to v1.8.0 requirements
    """
    # Validate Success type
    if not hasattr(success_type, "__annotations__"):
        raise ValueError(f"Success type {success_type.__name__} must have annotations")

    success_annotations = success_type.__annotations__

    # Find entity field
    entity_field = None
    for field in success_annotations:
        if field.lower() in ("entity", mutation_name.lower()):
            entity_field = field
            break

    if not entity_field:
        raise ValueError(
            f"Success type {success_type.__name__} must have entity field. "
            f"Expected 'entity' or '{mutation_name.lower()}'."
        )

    # Ensure entity is non-nullable
    entity_type = success_annotations[entity_field]
    if _is_optional(entity_type):
        raise ValueError(
            f"Success type {success_type.__name__} has nullable entity field '{entity_field}'. "
            f"v1.8.0 requires entity to be non-null in Success types. "
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
            f"v1.8.0 requires Error types to include REST-like error codes."
        )

    # Ensure status field exists
    if "status" not in error_annotations:
        raise ValueError(
            f"Error type {error_type.__name__} must have 'status: str' field."
        )

    # Ensure message field exists
    if "message" not in error_annotations:
        raise ValueError(
            f"Error type {error_type.__name__} must have 'message: str' field."
        )

    # Create union type
    union_name = f"{mutation_name}Result"
    union_type = strawberry.union(union_name, types=(success_type, error_type))

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
    if origin is typing.Union:
        args = typing.get_args(type_hint)
        return type(None) in args

    return False
```

---

### Step 3.2: Update GraphQL Schema Builder

**File:** `src/fraiseql/graphql/schema_builder.py`

**Add union type registration:**

```python
from strawberry import Schema
from typing import Type, List

class FraiseQLSchemaBuilder:
    """Build GraphQL schema for FraiseQL mutations.

    v1.8.0: Automatically creates union types for all mutations.
    """

    def __init__(self):
        self.mutations: List[MutationSchema] = []
        self.types: List[Type] = []
        self.unions: List[Type] = []

    def add_mutation(
        self,
        mutation_name: str,
        success_type: Type,
        error_type: Type,
    ):
        """Add mutation to schema.

        v1.8.0: Automatically creates union type.

        Args:
            mutation_name: Name of the mutation
            success_type: Success type class
            error_type: Error type class
        """
        schema = generate_mutation_schema(
            mutation_name=mutation_name,
            success_type=success_type,
            error_type=error_type,
        )

        self.mutations.append(schema)
        self.types.extend([success_type, error_type])
        self.unions.append(schema.union_type)

    def build(self) -> Schema:
        """Build Strawberry GraphQL schema.

        Returns:
            Strawberry Schema with all mutations as unions
        """
        import strawberry

        # Create mutation class
        @strawberry.type
        class Mutation:
            pass

        # Add mutation fields to Mutation class
        for mutation_schema in self.mutations:
            mutation_name = mutation_schema.mutation_name
            field_name = self._to_camel_case(mutation_name)
            union_type = mutation_schema.union_type

            # Create resolver
            async def resolver(self, input: dict) -> union_type:
                # Execution happens via Rust pipeline
                pass

            # Add field to Mutation class
            setattr(Mutation, field_name, strawberry.field(resolver=resolver))

        # Build schema
        schema = strawberry.Schema(
            query=None,  # Query type defined elsewhere
            mutation=Mutation,
            types=self.types + self.unions,
        )

        return schema

    def _to_camel_case(self, snake_str: str) -> str:
        """Convert snake_case to camelCase."""
        components = snake_str.split('_')
        return components[0] + ''.join(x.title() for x in components[1:])
```

---

### Step 3.3: Add Schema Validation

**File:** `src/fraiseql/schema/validator.py` (NEW)

```python
"""Schema validation for v1.8.0 requirements."""

from typing import Type, List, Tuple

class SchemaValidator:
    """Validate FraiseQL schemas conform to v1.8.0 requirements."""

    @staticmethod
    def validate_mutation_types(
        mutation_name: str,
        success_type: Type,
        error_type: Type,
    ) -> List[str]:
        """Validate mutation types conform to v1.8.0.

        Returns:
            List of validation errors (empty if valid)
        """
        errors = []

        # Validate Success type
        errors.extend(SchemaValidator._validate_success_type(
            mutation_name, success_type
        ))

        # Validate Error type
        errors.extend(SchemaValidator._validate_error_type(
            error_type
        ))

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

        # Find entity field
        entity_field = None
        for field in annotations:
            if field.lower() in ("entity", mutation_name.lower()):
                entity_field = field
                break

        if not entity_field:
            errors.append(
                f"{success_type.__name__}: Missing entity field. "
                f"Expected 'entity' or '{mutation_name.lower()}'."
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
                    f"{error_type.__name__}: Missing required field '{field_name}: {expected_type.__name__}'"
                )
            else:
                actual_type = annotations[field_name]
                # Basic type check (doesn't handle complex generics)
                if actual_type != expected_type and \
                   getattr(actual_type, "__origin__", None) != expected_type:
                    errors.append(
                        f"{error_type.__name__}.{field_name}: Wrong type. "
                        f"Expected '{expected_type.__name__}', got '{actual_type}'"
                    )

        return errors


def _is_optional(type_hint: Any) -> bool:
    """Check if type hint is Optional."""
    import typing
    origin = typing.get_origin(type_hint)
    if origin is typing.Union:
        args = typing.get_args(type_hint)
        return type(None) in args
    return False
```

---

## Testing Strategy

### Step 3.4: Schema Generation Tests

**File:** `tests/integration/graphql/mutations/test_schema_generation.py`

```python
import pytest
from fraiseql.schema import generate_mutation_schema, SchemaValidator

class Machine:
    """Example entity type."""
    id: str
    name: str

class Cascade:
    """Cascade metadata."""
    status: str

class TestSchemaGenerationV190:
    """Test schema generation for v1.8.0."""

    def test_generate_union_type(self):
        """Schema generation creates union type."""
        class CreateMachineSuccess:
            __annotations__ = {
                "machine": Machine,
                "cascade": Cascade | None,
            }

        class CreateMachineError:
            __annotations__ = {
                "code": int,
                "status": str,
                "message": str,
                "cascade": Cascade | None,
            }

        schema = generate_mutation_schema(
            mutation_name="CreateMachine",
            success_type=CreateMachineSuccess,
            error_type=CreateMachineError,
        )

        assert schema.mutation_name == "CreateMachine"
        assert schema.success_type == CreateMachineSuccess
        assert schema.error_type == CreateMachineError
        assert schema.union_type is not None

        # Check SDL
        sdl = schema.to_graphql_sdl()
        assert "union CreateMachineResult = CreateMachineSuccess | CreateMachineError" in sdl


class TestTypeConversion:
    """Test _python_type_to_graphql with comprehensive examples."""

    def test_basic_types(self):
        """Convert basic Python types to GraphQL."""
        schema = MutationSchema(
            mutation_name="Test",
            success_type=type("S", (), {}),
            error_type=type("E", (), {}),
            union_type=type("U", (), {}),
        )

        # Basic types are non-null
        assert schema._python_type_to_graphql(int) == "Int!"
        assert schema._python_type_to_graphql(str) == "String!"
        assert schema._python_type_to_graphql(bool) == "Boolean!"
        assert schema._python_type_to_graphql(float) == "Float!"

    def test_optional_types(self):
        """Convert optional types (nullable)."""
        schema = MutationSchema(...)

        # Optional makes nullable (removes "!")
        assert schema._python_type_to_graphql(int | None) == "Int"
        assert schema._python_type_to_graphql(str | None) == "String"
        assert schema._python_type_to_graphql(Machine | None) == "Machine"

    def test_list_types(self):
        """Convert list types to GraphQL arrays."""
        schema = MutationSchema(...)

        # Non-null list with non-null items
        assert schema._python_type_to_graphql(list[int]) == "[Int!]!"
        assert schema._python_type_to_graphql(list[str]) == "[String!]!"
        assert schema._python_type_to_graphql(list[Machine]) == "[Machine!]!"

        # Non-null list with nullable items
        assert schema._python_type_to_graphql(list[int | None]) == "[Int]!"
        assert schema._python_type_to_graphql(list[Machine | None]) == "[Machine]!"

        # Nullable list with non-null items
        assert schema._python_type_to_graphql(list[int] | None) == "[Int!]"

        # Nullable list with nullable items
        assert schema._python_type_to_graphql(list[int | None] | None) == "[Int]"

    def test_dict_types(self):
        """Convert dict types to JSON scalar."""
        schema = MutationSchema(...)

        # All dict types become JSON
        assert schema._python_type_to_graphql(dict[str, Any]) == "JSON"
        assert schema._python_type_to_graphql(dict[str, int]) == "JSON"
        assert schema._python_type_to_graphql(dict[str, list[Machine]]) == "JSON"

    def test_custom_types(self):
        """Convert custom types (dataclasses, models) to GraphQL types."""
        schema = MutationSchema(...)

        # Custom types use their __name__
        assert schema._python_type_to_graphql(Machine) == "Machine"
        assert schema._python_type_to_graphql(Cascade) == "Cascade"

        class User:
            pass

        assert schema._python_type_to_graphql(User) == "User"

    def test_nested_optional_lists(self):
        """Handle complex nested types."""
        schema = MutationSchema(...)

        # list[list[int]]
        inner_list = list[int]  # [Int!]!
        outer_list = list[inner_list]  # [[Int!]!!]!
        # Note: This gets complex - the implementation may need adjustment
        # For v1.8.0, we'll focus on simple list[X] patterns

    def test_unsupported_types_raise_errors(self):
        """Unsupported types raise clear errors."""
        schema = MutationSchema(...)

        # Bare list without type parameter
        with pytest.raises(ValueError, match="List type must have element type"):
            schema._python_type_to_graphql(list)

        # Multiple non-None union types
        with pytest.raises(ValueError, match="multiple non-None types not supported"):
            schema._python_type_to_graphql(int | str)

        # None type directly
        with pytest.raises(ValueError, match="Cannot convert None type"):
            schema._python_type_to_graphql(type(None))


class TestEntityFieldDetection:
    """Test _is_entity_field with various patterns."""

    def test_exact_entity_match(self):
        """Field named 'entity' is always detected."""
        schema = MutationSchema(
            mutation_name="CreateMachine",
            success_type=type("S", (), {}),
            error_type=type("E", (), {}),
            union_type=type("U", (), {}),
        )

        assert schema._is_entity_field("entity") is True
        assert schema._is_entity_field("Entity") is True  # Case insensitive
        assert schema._is_entity_field("ENTITY") is True

    def test_mutation_name_derived(self):
        """Entity field derived from mutation name."""
        schema = MutationSchema(mutation_name="CreateMachine", ...)

        # "CreateMachine" → "machine"
        assert schema._is_entity_field("machine") is True
        assert schema._is_entity_field("Machine") is True

        schema = MutationSchema(mutation_name="DeletePost", ...)

        # "DeletePost" → "post"
        assert schema._is_entity_field("post") is True

        schema = MutationSchema(mutation_name="UpdateUser", ...)

        # "UpdateUser" → "user"
        assert schema._is_entity_field("user") is True

    def test_plural_entity_names(self):
        """Handle plural entity names."""
        schema = MutationSchema(mutation_name="CreateMachines", ...)

        # "CreateMachines" → "machines"
        assert schema._is_entity_field("machines") is True

        # Also accepts singular
        assert schema._is_entity_field("machine") is True

    def test_common_entity_field_names(self):
        """Recognize common patterns."""
        schema = MutationSchema(mutation_name="ProcessData", ...)

        assert schema._is_entity_field("result") is True
        assert schema._is_entity_field("data") is True
        assert schema._is_entity_field("item") is True
        assert schema._is_entity_field("record") is True

    def test_non_entity_fields(self):
        """Non-entity fields are not detected."""
        schema = MutationSchema(mutation_name="CreateMachine", ...)

        assert schema._is_entity_field("cascade") is False
        assert schema._is_entity_field("message") is False
        assert schema._is_entity_field("updated_fields") is False
        assert schema._is_entity_field("metadata") is False

    def test_success_type_entity_non_nullable(self):
        """Success type entity is generated as non-nullable."""
        class CreateMachineSuccess:
            __annotations__ = {
                "machine": Machine,  # Non-nullable
            }

        class CreateMachineError:
            __annotations__ = {
                "code": int,
                "status": str,
                "message": str,
            }

        schema = generate_mutation_schema("CreateMachine", CreateMachineSuccess, CreateMachineError)
        sdl = schema.to_graphql_sdl()

        # Check that machine field is non-nullable
        assert "machine: Machine!" in sdl

    def test_error_type_has_code_field(self):
        """Error type includes code field."""
        class CreateMachineSuccess:
            __annotations__ = {"machine": Machine}

        class CreateMachineError:
            __annotations__ = {
                "code": int,
                "status": str,
                "message": str,
            }

        schema = generate_mutation_schema("CreateMachine", CreateMachineSuccess, CreateMachineError)
        sdl = schema.to_graphql_sdl()

        # Check code field exists and is non-nullable
        assert "code: Int!" in sdl

    def test_nullable_entity_raises_error(self):
        """Nullable entity in Success type raises error."""
        class CreateMachineSuccess:
            __annotations__ = {
                "machine": Machine | None,  # ❌ Nullable
            }

        class CreateMachineError:
            __annotations__ = {
                "code": int,
                "status": str,
                "message": str,
            }

        with pytest.raises(ValueError, match="nullable entity"):
            generate_mutation_schema("CreateMachine", CreateMachineSuccess, CreateMachineError)

    def test_missing_code_field_raises_error(self):
        """Missing code field in Error type raises error."""
        class CreateMachineSuccess:
            __annotations__ = {"machine": Machine}

        class CreateMachineError:
            __annotations__ = {
                # Missing "code": int
                "status": str,
                "message": str,
            }

        with pytest.raises(ValueError, match="code"):
            generate_mutation_schema("CreateMachine", CreateMachineSuccess, CreateMachineError)


class TestSchemaValidator:
    """Test schema validation."""

    def test_valid_mutation_types(self):
        """Valid mutation types pass validation."""
        class CreateMachineSuccess:
            __annotations__ = {"machine": Machine}

        class CreateMachineError:
            __annotations__ = {
                "code": int,
                "status": str,
                "message": str,
            }

        errors = SchemaValidator.validate_mutation_types(
            "CreateMachine", CreateMachineSuccess, CreateMachineError
        )
        assert errors == []

    def test_nullable_entity_fails_validation(self):
        """Nullable entity fails validation."""
        class CreateMachineSuccess:
            __annotations__ = {"machine": Machine | None}

        class CreateMachineError:
            __annotations__ = {"code": int, "status": str, "message": str}

        errors = SchemaValidator.validate_mutation_types(
            "CreateMachine", CreateMachineSuccess, CreateMachineError
        )
        assert len(errors) > 0
        assert any("non-null" in err for err in errors)

    def test_missing_code_fails_validation(self):
        """Missing code field fails validation."""
        class CreateMachineSuccess:
            __annotations__ = {"machine": Machine}

        class CreateMachineError:
            __annotations__ = {"status": str, "message": str}

        errors = SchemaValidator.validate_mutation_types(
            "CreateMachine", CreateMachineSuccess, CreateMachineError
        )
        assert len(errors) > 0
        assert any("code" in err for err in errors)
```

---

## Verification Checklist

### Code Changes
- [ ] `mutation_schema_generator.py` - Add union type generation
- [ ] `mutation_schema_generator.py` - Validate entity non-nullable
- [ ] `mutation_schema_generator.py` - Validate code field in Error
- [ ] `schema_builder.py` - Register union types
- [ ] `validator.py` - Create schema validator
- [ ] All schema generation code updated

### Testing
- [ ] New test: `test_generate_union_type`
- [ ] New test: `test_success_type_entity_non_nullable`
- [ ] New test: `test_error_type_has_code_field`
- [ ] New test: `test_nullable_entity_raises_error`
- [ ] New test: `test_missing_code_field_raises_error`
- [ ] All schema tests pass

### GraphQL SDL
- [ ] Union types generated correctly
- [ ] Entity fields are non-nullable
- [ ] Error types include code field
- [ ] SDL validates with GraphQL spec

---

## Expected SDL Output

**Before (v1.7.x):**
```graphql
type CreateMachineSuccess {
  machine: Machine    # Nullable ❌
  message: String!
  cascade: Cascade
}

type CreateMachineError {
  message: String!
  errors: [Error!]!
}

type Mutation {
  createMachine(input: CreateMachineInput!): CreateMachineSuccess!
}
```

**After (v1.8.0):**
```graphql
union CreateMachineResult = CreateMachineSuccess | CreateMachineError

type CreateMachineSuccess {
  machine: Machine!   # Always non-null ✅
  cascade: Cascade
}

type CreateMachineError {
  code: Int!          # NEW: REST-like code ✅
  status: String!
  message: String!
  cascade: Cascade
}

type Mutation {
  createMachine(input: CreateMachineInput!): CreateMachineResult!
}
```

---

## Type Conversion Examples Reference

### Complete Type Mapping Table

| Python Type | GraphQL Type | Notes |
|-------------|--------------|-------|
| `int` | `Int!` | Non-null integer |
| `str` | `String!` | Non-null string |
| `bool` | `Boolean!` | Non-null boolean |
| `float` | `Float!` | Non-null float |
| `int \| None` | `Int` | Nullable integer |
| `str \| None` | `String` | Nullable string |
| `Machine` | `Machine` | Custom type (nullable by default) |
| `list[int]` | `[Int!]!` | Non-null list of non-null ints |
| `list[int \| None]` | `[Int]!` | Non-null list of nullable ints |
| `list[int] \| None` | `[Int!]` | Nullable list of non-null ints |
| `list[Machine]` | `[Machine!]!` | Non-null list of non-null Machines |
| `list[Machine \| None]` | `[Machine]!` | Non-null list of nullable Machines |
| `dict[str, Any]` | `JSON` | JSON scalar (any dict) |
| `dict[str, int]` | `JSON` | JSON scalar (typed dict loses type info) |

### Real-World Success Type Examples

```python
# Example 1: Simple entity
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # → machine: Machine! in GraphQL
    cascade: Cascade | None = None  # → cascade: Cascade in GraphQL

# Generated SDL:
# type CreateMachineSuccess {
#   machine: Machine!
#   cascade: Cascade
# }

# Example 2: Entity with nested types
@fraiseql.success
class CreatePostSuccess:
    post: Post  # → post: Post!
    tags: list[Tag] | None = None  # → tags: [Tag!] in GraphQL
    cascade: Cascade | None = None

# Generated SDL:
# type CreatePostSuccess {
#   post: Post!
#   tags: [Tag!]
#   cascade: Cascade
# }

# Example 3: Entity with metadata
@fraiseql.success
class BulkImportSuccess:
    result: ImportResult  # → result: ImportResult!
    created_ids: list[str]  # → createdIds: [String!]!
    skipped_count: int  # → skippedCount: Int!
    metadata: dict[str, Any] | None = None  # → metadata: JSON

# Generated SDL:
# type BulkImportSuccess {
#   result: ImportResult!
#   createdIds: [String!]!
#   skippedCount: Int!
#   metadata: JSON
# }

# Example 4: Nullable list items
@fraiseql.success
class GetMachinesSuccess:
    machines: list[Machine | None]  # → machines: [Machine]! (items can be null)
    total: int  # → total: Int!

# Generated SDL:
# type GetMachinesSuccess {
#   machines: [Machine]!
#   total: Int!
# }
```

### Real-World Error Type Examples

```python
# Example 1: Basic error
@fraiseql.failure
class CreateMachineError:
    code: int  # → code: Int!
    status: str  # → status: String!
    message: str  # → message: String!
    cascade: Cascade | None = None  # → cascade: Cascade

# Generated SDL:
# type CreateMachineError {
#   code: Int!
#   status: String!
#   message: String!
#   cascade: Cascade
# }

# Example 2: Error with validation details
@fraiseql.failure
class ValidationError:
    code: int  # Always 422 for validation
    status: str  # e.g., "noop:invalid_fields"
    message: str  # Human-readable
    field_errors: list[FieldError] | None = None  # → fieldErrors: [FieldError!]
    cascade: Cascade | None = None

# Generated SDL:
# type ValidationError {
#   code: Int!
#   status: String!
#   message: String!
#   fieldErrors: [FieldError!]
#   cascade: Cascade
# }

# Example 3: Error with metadata
@fraiseql.failure
class ImportError:
    code: int
    status: str
    message: str
    failed_rows: list[int] | None = None  # → failedRows: [Int!]
    error_details: dict[str, Any] | None = None  # → errorDetails: JSON
```

### Entity Field Detection Examples

```python
# Pattern 1: Exact "entity" match
@fraiseql.success
class CreateMachineSuccess:
    entity: Machine  # ✅ Detected as entity field

# Pattern 2: Mutation name match
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Detected (CreateMachine → machine)

@fraiseql.success
class DeletePostSuccess:
    post: Post  # ✅ Detected (DeletePost → post)

# Pattern 3: Plural handling
@fraiseql.success
class CreateMachinesSuccess:
    machines: list[Machine]  # ✅ Detected (CreateMachines → machines)

# Pattern 4: Common patterns
@fraiseql.success
class ProcessDataSuccess:
    result: ProcessResult  # ✅ Detected (common pattern)

@fraiseql.success
class FetchItemSuccess:
    item: Item  # ✅ Detected (common pattern)

# ❌ NOT detected as entity fields:
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Entity
    cascade: Cascade | None  # ❌ Not entity (CASCADE metadata)
    message: str  # ❌ Not entity (metadata)
    updated_fields: list[str]  # ❌ Not entity (metadata)
```

### Edge Cases and Error Handling

```python
# Edge Case 1: Unsupported union types
@fraiseql.success
class CreateMachineSuccess:
    result: Machine | Post  # ❌ Raises ValueError
    # "Union types with multiple non-None types not supported"
    # Fix: Use single type or create separate GraphQL union

# Edge Case 2: Bare list without type
@fraiseql.success
class CreateMachineSuccess:
    items: list  # ❌ Raises ValueError
    # "List type must have element type: use list[X]"
    # Fix: items: list[Machine]

# Edge Case 3: None type directly
@fraiseql.success
class CreateMachineSuccess:
    machine: type(None)  # ❌ Raises ValueError
    # "Cannot convert None type to GraphQL"
    # Fix: machine: Machine | None

# Edge Case 4: Missing entity field
@fraiseql.success
class CreateMachineSuccess:
    cascade: Cascade | None = None
    # ❌ Raises ValueError during validation
    # "Missing entity field. Expected 'entity' or 'machine'."
    # Fix: Add machine: Machine field

# Edge Case 5: Nullable entity in Success type
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None  # ❌ Raises ValueError
    # "Success type has nullable entity field"
    # Fix: machine: Machine (remove | None)

# Edge Case 6: Missing code field in Error type
@fraiseql.failure
class CreateMachineError:
    status: str
    message: str
    # ❌ Raises ValueError
    # "Error type must have 'code: int' field"
    # Fix: Add code: int
```

### Smoke Tests for Phase 3

Run these quick tests after implementing each step:

```python
# Smoke Test 1: Basic schema generation
from fraiseql.schema import generate_mutation_schema

class Machine:
    id: str

class CreateMachineSuccess:
    __annotations__ = {"machine": Machine}

class CreateMachineError:
    __annotations__ = {"code": int, "status": str, "message": str}

schema = generate_mutation_schema(
    "CreateMachine",
    CreateMachineSuccess,
    CreateMachineError
)

assert schema.union_type is not None
assert "union CreateMachineResult" in schema.to_graphql_sdl()
print("✅ Smoke Test 1: Basic schema generation passed")

# Smoke Test 2: Type conversion
schema_obj = schema  # MutationSchema instance
assert schema_obj._python_type_to_graphql(int) == "Int!"
assert schema_obj._python_type_to_graphql(str | None) == "String"
assert schema_obj._python_type_to_graphql(list[int]) == "[Int!]!"
print("✅ Smoke Test 2: Type conversion passed")

# Smoke Test 3: Entity field detection
assert schema_obj._is_entity_field("machine") is True
assert schema_obj._is_entity_field("cascade") is False
assert schema_obj._is_entity_field("entity") is True
print("✅ Smoke Test 3: Entity field detection passed")

# Smoke Test 4: Validation
from fraiseql.schema import SchemaValidator

errors = SchemaValidator.validate_mutation_types(
    "CreateMachine",
    CreateMachineSuccess,
    CreateMachineError
)
assert errors == []
print("✅ Smoke Test 4: Validation passed")

# Smoke Test 5: Error detection
class BadSuccess:
    __annotations__ = {"machine": Machine | None}  # Nullable entity

errors = SchemaValidator.validate_mutation_types(
    "CreateMachine",
    BadSuccess,
    CreateMachineError
)
assert len(errors) > 0
assert any("non-null" in err for err in errors)
print("✅ Smoke Test 5: Error detection passed")

print("\n🎉 All Phase 3 smoke tests passed!")
```

---

## Next Steps

Once Phase 3 is complete:
1. **Run smoke tests** - Verify basic functionality immediately
2. **Validate generated schemas** - Check GraphQL spec compliance
3. **Test introspection queries** - Ensure schema discovery works
4. **Run full test suite** - `uv run pytest tests/integration/graphql/mutations/test_schema_generation.py`
5. **Commit changes**: `git commit -m "feat(schema)!: union types for mutations (v1.8.0) [SCHEMA]"`
6. **Proceed to Phase 4**: Testing & Documentation

**Blocking:** Testing (Phase 4) depends on schema generation working correctly.

**If Issues Arise:**
- Review type conversion examples above
- Check entity field detection patterns
- Run smoke tests to isolate problems
- Consult `TestTypeConversion` and `TestEntityFieldDetection` test cases
