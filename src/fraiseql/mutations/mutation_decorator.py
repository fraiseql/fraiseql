"""PostgreSQL function-based mutation decorator."""

import re
from collections.abc import Callable
from typing import Any, TypeVar, get_type_hints

from fraiseql.mutations.error_config import MutationErrorConfig
from fraiseql.mutations.parser import parse_mutation_result
from fraiseql.types.definitions import UNSET
from fraiseql.utils.casing import to_snake_case

T = TypeVar("T")


class MutationDefinition:
    """Definition of a PostgreSQL-backed mutation."""

    def __init__(
        self,
        mutation_class: type,
        function_name: str | None = None,
        schema: str | None = None,
        context_params: dict[str, str] | None = None,
        error_config: MutationErrorConfig | None = None,
    ) -> None:
        self.mutation_class = mutation_class
        self.name = mutation_class.__name__

        # Store the provided schema for lazy resolution
        self._provided_schema = schema
        self._resolved_schema = None  # Will be resolved lazily

        self.context_params = context_params or {}
        self.error_config = error_config

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

    @property
    def schema(self) -> str:
        """Get the schema, resolving it lazily if needed."""
        if self._resolved_schema is None:
            self._resolved_schema = self._resolve_schema(self._provided_schema)
        return self._resolved_schema

    @schema.setter
    def schema(self, value: str) -> None:
        """Allow setting the schema directly for testing."""
        self._resolved_schema = value

    def _resolve_schema(self, provided_schema: str | None) -> str:
        """Resolve the schema to use, considering defaults from config."""
        # If schema was explicitly provided, use it
        if provided_schema is not None:
            return provided_schema

        # Try to get default from registry config
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            if registry.config and hasattr(registry.config, "default_mutation_schema"):
                return registry.config.default_mutation_schema
        except ImportError:
            pass

        # Fall back to "public" as per feature requirements
        return "public"

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

            if self.context_params:
                # Extract context arguments
                context_args = []
                for context_key in self.context_params:
                    context_value = info.context.get(context_key)
                    if context_value is None:
                        msg = (
                            f"Required context parameter '{context_key}' "
                            f"not found in GraphQL context"
                        )
                        raise RuntimeError(msg)

                    # Extract specific field if it's a UserContext object
                    if hasattr(context_value, "user_id") and context_key == "user":
                        context_args.append(context_value.user_id)
                    else:
                        context_args.append(context_value)

                result = await db.execute_function_with_context(
                    full_function_name,
                    context_args,
                    input_data,
                )
            else:
                # Use original single-parameter function
                result = await db.execute_function(full_function_name, input_data)

            # Parse result into Success or Error type
            parsed_result = parse_mutation_result(
                result,
                self.success_type,
                self.error_type,
                self.error_config,
            )

            # Return the parsed result directly - let GraphQL handle object resolution
            # Serialization will be handled at the JSON encoding stage

            # DEBUG: Check if errors field is being set correctly for enterprise compatibility
            if hasattr(parsed_result, "errors") and hasattr(parsed_result, "__class__"):
                class_name = parsed_result.__class__.__name__
                errors_value = parsed_result.errors
                import logging

                logger = logging.getLogger(__name__)
                logger.info(f"Mutation result: {class_name}, errors={errors_value}")

                # CRITICAL FIX: Force populate errors for frontend compatibility
                if class_name.endswith("Error") and errors_value is None:
                    status = getattr(parsed_result, "status", "unknown")
                    message = getattr(parsed_result, "message", "Unknown error")

                    # Create error from status and message
                    if ":" in status:
                        error_code = 422
                        identifier = status.split(":", 1)[1]
                    else:
                        error_code = 500
                        identifier = "general_error"

                    error_obj = {
                        "code": error_code,
                        "identifier": identifier,
                        "message": message,
                        "details": {},
                    }

                    # Force set errors array
                    parsed_result.errors = [error_obj]
                    logger.info(f"FORCED errors population: {[error_obj]}")

            return parsed_result

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
    schema: str | None = None,
    context_params: dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None,
) -> type[T] | Callable[[type[T]], type[T]] | Callable[..., Any]:
    """Decorator to define GraphQL mutations with PostgreSQL function backing.

    This decorator supports both simple function-based mutations and sophisticated
    class-based mutations with structured success/error handling. Class-based mutations
    automatically call PostgreSQL functions and parse results into typed responses.

    Args:
        _cls: The mutation function or class to decorate (when used without parentheses)
        function: PostgreSQL function name (defaults to snake_case of class name)
        schema: PostgreSQL schema containing the function (defaults to "graphql")
        context_params: Maps GraphQL context keys to PostgreSQL function parameter names
        error_config: Optional configuration for error detection behavior

    Returns:
        Decorated mutation with automatic PostgreSQL function integration

    Examples:
        Simple function-based mutation::\

            @mutation
            async def create_user(info, input: CreateUserInput) -> User:
                db = info.context["db"]
                # Direct implementation with custom logic
                user_data = {
                    "name": input.name,
                    "email": input.email,
                    "created_at": datetime.utcnow()
                }
                result = await db.execute_raw(
                    "INSERT INTO users (data) VALUES ($1) RETURNING *",
                    user_data
                )
                return User(**result[0]["data"])

        Basic class-based mutation::\

            @mutation
            class CreateUser:
                input: CreateUserInput
                success: CreateUserSuccess
                error: CreateUserError

            # This automatically calls PostgreSQL function: graphql.create_user(input)
            # and parses the result into either CreateUserSuccess or CreateUserError

        Mutation with custom PostgreSQL function::\

            @mutation(function="register_new_user", schema="auth")
            class RegisterUser:
                input: RegistrationInput
                success: RegistrationSuccess
                error: RegistrationError

            # Calls: auth.register_new_user(input) instead of default name

        Mutation with context parameters::\

            @mutation(
                function="create_location",
                schema="app",
                context_params={
                    "tenant_id": "input_pk_organization",
                    "user": "input_created_by"
                }
            )
            class CreateLocation:
                input: CreateLocationInput
                success: CreateLocationSuccess
                error: CreateLocationError

            # Calls: app.create_location(tenant_id, user_id, input)
            # Where tenant_id comes from info.context["tenant_id"]
            # And user_id comes from info.context["user"].user_id

        Mutation with validation and error handling::\

            @fraise_input
            class UpdateUserInput:
                id: UUID
                name: str | None = None
                email: str | None = None

            @fraise_type
            class UpdateUserSuccess:
                user: User
                message: str

            @fraise_type
            class UpdateUserError:
                code: str
                message: str
                field: str | None = None

            @mutation
            async def update_user(info, input: UpdateUserInput) -> User:
                db = info.context["db"]
                user_context = info.context.get("user")

                # Authorization check
                if not user_context:
                    raise GraphQLError("Authentication required")

                # Validation
                if input.email and not is_valid_email(input.email):
                    raise GraphQLError("Invalid email format")

                # Update logic
                updates = {}
                if input.name:
                    updates["name"] = input.name
                if input.email:
                    updates["email"] = input.email

                if not updates:
                    raise GraphQLError("No fields to update")

                return await db.update_one("user_view", {"id": input.id}, updates)

        Multi-step mutation with transaction::\

            @mutation
            async def transfer_funds(
                info,
                input: TransferInput
            ) -> TransferResult:
                db = info.context["db"]

                async with db.transaction():
                    # Validate source account
                    source = await db.find_one(
                        "account_view",
                        {"id": input.source_account_id}
                    )
                    if not source or source.balance < input.amount:
                        raise GraphQLError("Insufficient funds")

                    # Validate destination account
                    dest = await db.find_one(
                        "account_view",
                        {"id": input.destination_account_id}
                    )
                    if not dest:
                        raise GraphQLError("Destination account not found")

                    # Perform transfer
                    await db.update_one(
                        "account_view",
                        {"id": source.id},
                        {"balance": source.balance - input.amount}
                    )
                    await db.update_one(
                        "account_view",
                        {"id": dest.id},
                        {"balance": dest.balance + input.amount}
                    )

                    # Log transaction
                    transfer = await db.create_one("transfer_view", {
                        "source_account_id": input.source_account_id,
                        "destination_account_id": input.destination_account_id,
                        "amount": input.amount,
                        "created_at": datetime.utcnow()
                    })

                    return TransferResult(
                        transfer=transfer,
                        new_source_balance=source.balance - input.amount,
                        new_dest_balance=dest.balance + input.amount
                    )

        Mutation with file upload handling::\

            @mutation
            async def upload_avatar(
                info,
                input: UploadAvatarInput  # Contains file: Upload field
            ) -> User:
                db = info.context["db"]
                storage = info.context["storage"]
                user_context = info.context["user"]

                if not user_context:
                    raise GraphQLError("Authentication required")

                # Process file upload
                file_content = await input.file.read()
                if len(file_content) > 5 * 1024 * 1024:  # 5MB limit
                    raise GraphQLError("File too large")

                # Store file
                file_url = await storage.store_user_avatar(
                    user_context.user_id,
                    file_content,
                    input.file.content_type
                )

                # Update user record
                return await db.update_one(
                    "user_view",
                    {"id": user_context.user_id},
                    {"avatar_url": file_url}
                )

    PostgreSQL Function Requirements:
        For class-based mutations, the PostgreSQL function should:

        1. Accept input as JSONB parameter
        2. Return a result with 'success' boolean field
        3. Include either 'data' field (success) or 'error' field (failure)

        Example PostgreSQL function::\

            CREATE OR REPLACE FUNCTION graphql.create_user(input jsonb)
            RETURNS jsonb
            LANGUAGE plpgsql
            AS $$
            DECLARE
                user_id uuid;
                result jsonb;
            BEGIN
                -- Insert user
                INSERT INTO users (name, email, created_at)
                VALUES (
                    input->>'name',
                    input->>'email',
                    now()
                )
                RETURNING id INTO user_id;

                -- Return success response
                result := jsonb_build_object(
                    'success', true,
                    'data', jsonb_build_object(
                        'id', user_id,
                        'name', input->>'name',
                        'email', input->>'email',
                        'message', 'User created successfully'
                    )
                );

                RETURN result;
            EXCEPTION
                WHEN unique_violation THEN
                    -- Return error response
                    result := jsonb_build_object(
                        'success', false,
                        'error', jsonb_build_object(
                            'code', 'EMAIL_EXISTS',
                            'message', 'Email address already exists',
                            'field', 'email'
                        )
                    );
                    RETURN result;
            END;
            $$;

    Notes:
        - Function-based mutations provide full control over implementation
        - Class-based mutations automatically integrate with PostgreSQL functions
        - Use transactions for multi-step operations to ensure data consistency
        - PostgreSQL functions handle validation and business logic at the database level
        - Context parameters enable tenant isolation and user tracking
        - Success/error types provide structured response handling
        - All mutations are automatically registered with the GraphQL schema
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
        definition = MutationDefinition(cls, function, schema, context_params, error_config)

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
    """Convert an object to a dictionary.

    UNSET values are excluded from the dictionary to enable partial updates.
    Only fields that were explicitly provided (including explicit None) are included.
    """
    if hasattr(obj, "to_dict"):
        return obj.to_dict()
    if hasattr(obj, "__dict__"):
        # Convert UUIDs to strings for JSON serialization
        result = {}
        for k, v in obj.__dict__.items():
            if not k.startswith("_"):
                if v is UNSET:
                    # Skip UNSET fields entirely - they weren't provided
                    continue
                if hasattr(v, "hex"):  # UUID
                    result[k] = str(v)
                elif hasattr(v, "isoformat"):  # date, datetime, time
                    result[k] = v.isoformat()
                else:
                    result[k] = v
        return result
    if isinstance(obj, dict):
        return obj
    msg = f"Cannot convert {type(obj)} to dictionary"
    raise TypeError(msg)
