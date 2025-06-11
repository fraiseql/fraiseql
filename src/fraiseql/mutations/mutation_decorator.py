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
    ):
        self.mutation_class = mutation_class
        self.name = mutation_class.__name__
        self.schema = schema

        # Get type hints
        hints = get_type_hints(mutation_class)
        self.input_type = hints.get("input")
        self.success_type = hints.get("success")
        self.error_type = hints.get("error")

        if not self.input_type:
            raise TypeError(f"Mutation {self.name} must define 'input' type")
        if not self.success_type:
            raise TypeError(f"Mutation {self.name} must define 'success' type")
        if not self.error_type:
            raise TypeError(f"Mutation {self.name} must define 'error' type")

        # Derive function name from class name if not provided
        if function_name:
            self.function_name = function_name
        else:
            # Convert CamelCase to snake_case
            # CreateUser -> create_user
            self.function_name = _camel_to_snake(self.name)

    def create_resolver(self) -> Callable:
        """Create the GraphQL resolver function."""

        async def resolver(
            info: Any,
            input: Any,
        ) -> Any:
            """Auto-generated resolver for PostgreSQL mutation."""
            # Get database connection
            db = info.context.get("db")
            if not db:
                raise RuntimeError("No database connection in context")

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

        return resolver


def mutation(
    _cls: type[T] | None = None,
    *,
    function: str | None = None,
    schema: str = "graphql",
) -> type[T] | Callable[[type[T]], type[T]]:
    """Decorator to define a PostgreSQL function-based mutation.

    Args:
        function: Optional PostgreSQL function name (defaults to snake_case of class name)
        schema: PostgreSQL schema containing the function (defaults to "graphql")

    Example:
        @mutation
        class CreateUser:
            input: CreateUserInput
            success: CreateUserSuccess
            error: CreateUserError

    This will call the PostgreSQL function `graphql.create_user`.
    """

    def decorator(cls: type[T]) -> type[T]:
        # Create mutation definition
        definition = MutationDefinition(cls, function, schema)

        # Store definition on the class
        cls.__fraiseql_mutation__ = definition

        # Create and store resolver
        cls.__fraiseql_resolver__ = definition.create_resolver()

        return cls

    if _cls is None:
        return decorator
    else:
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
    elif hasattr(obj, "__dict__"):
        # Convert UUIDs to strings for JSON serialization
        result = {}
        for k, v in obj.__dict__.items():
            if not k.startswith("_"):
                if hasattr(v, "hex"):  # UUID
                    result[k] = str(v)
                else:
                    result[k] = v
        return result
    elif isinstance(obj, dict):
        return obj
    else:
        raise TypeError(f"Cannot convert {type(obj)} to dictionary")
