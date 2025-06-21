"""PostgreSQL function-based mutation decorator."""

import re
from collections.abc import Callable
from typing import Any, TypeVar, get_type_hints

from fraiseql.mutations.parser import parse_mutation_result
from fraiseql.utils.casing import to_snake_case

T = TypeVar("T")


class MutationDefinition:
    """Definition of a PostgreSQL-backed mutation."""

    def __init__(
        self,
        mutation_class: type,
        function_name: str | None = None,
        schema: str = "graphql",
    ) -> None:
        self.mutation_class = mutation_class
        self.name = mutation_class.__name__
        self.schema = schema

        # Get type hints
        hints = get_type_hints(mutation_class)
        self.input_type = hints.get("input")
        self.success_type = hints.get("success")
        self.error_type = hints.get("error") or hints.get(
            "failure",
        )  # Support both 'error' and 'failure'

        if not self.input_type:
            msg = f"Mutation {self.name} must define 'input' type"
            raise TypeError(msg)
        if not self.success_type:
            msg = f"Mutation {self.name} must define 'success' type"
            raise TypeError(msg)
        if not self.error_type:
            msg = (
                f"Mutation {self.name} must define 'failure' type "
                "(or 'error' for backwards compatibility)"
            )
            raise TypeError(
                msg,
            )

        # Derive function name from class name if not provided
        if function_name:
            self.function_name = function_name
        else:
            # Convert CamelCase to snake_case
            # CreateUser -> create_user
            self.function_name = _camel_to_snake(self.name)

    def create_resolver(self) -> Callable:
        """Create the GraphQL resolver function."""

        async def resolver(info, input):
            """Auto-generated resolver for PostgreSQL mutation."""
            # Get database connection
            db = info.context.get("db")
            if not db:
                msg = "No database connection in context"
                raise RuntimeError(msg)

            # Convert input to dict
            input_data = _to_dict(input)

            # Call PostgreSQL function
            full_function_name = f"{self.schema}.{self.function_name}"
            result = await db.execute_function(full_function_name, input_data)

            # Parse result into Success or Error type
            return parse_mutation_result(
                result,
                self.success_type,
                self.error_type,
            )

        # Set metadata for GraphQL introspection
        resolver.__name__ = to_snake_case(self.name)
        resolver.__doc__ = self.mutation_class.__doc__ or f"Mutation for {self.name}"

        # Store mutation definition for schema building
        resolver.__fraiseql_mutation__ = self

        # Set proper annotations for the resolver
        # We use Union of success and error types as the return type
        from typing import Union

        if self.success_type and self.error_type:
            return_type = Union[self.success_type, self.error_type]
        else:
            return_type = self.success_type or self.error_type

        resolver.__annotations__ = {"input": self.input_type, "return": return_type}

        return resolver


def mutation(
    _cls: type[T] | Callable[..., Any] | None = None,
    *,
    function: str | None = None,
    schema: str = "graphql",
) -> type[T] | Callable[[type[T]], type[T]] | Callable[..., Any]:
    """Decorator to define a mutation.

    Supports two patterns:

    1. Simple function-based mutations (returns the type directly):
        @mutation
        async def create_user(info, input: CreateUserInput) -> User:
            # Your logic here
            return User(...)

    2. Class-based mutations with success/error handling:
        @mutation
        class CreateUser:
            input: CreateUserInput
            success: CreateUserSuccess
            error: CreateUserError

    Args:
        function: Optional PostgreSQL function name (defaults to snake_case of name)
        schema: PostgreSQL schema containing the function (defaults to "graphql")
    """

    def decorator(
        cls_or_fn: type[T] | Callable[..., Any],
    ) -> type[T] | Callable[..., Any]:
        # Import here to avoid circular imports
        from fraiseql.gql.schema_builder import SchemaRegistry

        registry = SchemaRegistry.get_instance()

        # Check if it's a function (simple mutation pattern)
        if callable(cls_or_fn) and not isinstance(cls_or_fn, type):
            # It's a function-based mutation
            fn = cls_or_fn

            # Store metadata for schema building
            fn.__fraiseql_mutation__ = True
            fn.__fraiseql_resolver__ = fn

            # Auto-register with schema
            registry.register_mutation(fn)

            return fn

        # Otherwise, it's a class-based mutation
        cls = cls_or_fn
        # Create mutation definition
        definition = MutationDefinition(cls, function, schema)

        # Store definition on the class
        cls.__fraiseql_mutation__ = definition

        # Create and store resolver
        cls.__fraiseql_resolver__ = definition.create_resolver()

        # Auto-register with schema
        registry.register_mutation(cls)

        return cls

    if _cls is None:
        return decorator
    return decorator(_cls)


def _camel_to_snake(name: str) -> str:
    """Convert CamelCase to snake_case."""
    # Insert underscore before uppercase letters (except at start)
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
    # Handle sequences of capitals
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()


def _to_dict(obj: Any) -> dict[str, Any]:
    """Convert an object to a dictionary."""
    if hasattr(obj, "to_dict"):
        return obj.to_dict()
    if hasattr(obj, "__dict__"):
        # Convert UUIDs to strings for JSON serialization
        result = {}
        for k, v in obj.__dict__.items():
            if not k.startswith("_"):
                if hasattr(v, "hex"):  # UUID
                    result[k] = str(v)
                else:
                    result[k] = v
        return result
    if isinstance(obj, dict):
        return obj
    msg = f"Cannot convert {type(obj)} to dictionary"
    raise TypeError(msg)
